use std::io::{self, Write};

use colored::Colorize;
use futures::StreamExt;
use tokio::sync::mpsc::UnboundedSender;

use crate::agent::planner::{ExecutionPlan, Planner};
use crate::llm::{
    ChatOptions, ChatResponse, FinishReason, LLMProvider, Message, StreamChunk, ToolCall,
};
use crate::tools::permission::{Permission, PermissionManager};
use crate::tools::ToolRegistry;
use super::{AgentConfig, AgentEvent, AgentResult, ConfirmationDetails, ConfirmationType, StopReason};

/// The core agent loop: plan → action → observe.
/// When `event_tx` is Some, streaming events are sent for TUI display.
pub async fn run_agent_loop(
    provider: &dyn LLMProvider,
    initial_messages: &[Message],
    model: &str,
    config: &AgentConfig,
    tool_registry: &ToolRegistry,
    permission_manager: &PermissionManager,
    event_tx: Option<UnboundedSender<AgentEvent>>,
) -> AgentResult {
    let mut total_tokens: u32 = 0;
    let mut tools_called: Vec<String> = Vec::new();
    let mut active_plan: Option<ExecutionPlan> = None;
    let planner = if config.enable_planning { Some(Planner::new()) } else { None };

    let options = ChatOptions {
        model: model.to_string(),
        temperature: Some(config.temperature),
        max_tokens: Some(config.max_tokens),
        tools: Some(tool_registry.to_definitions()),
        system: None,
    };

    // Build system prompt with memory context
    let memory_context = if config.enable_memory {
        config.memory_store.as_ref()
            .map(|m| m.build_context_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Add planning instruction if enabled
    let planning_instruction = if config.enable_planning {
        "\n\nWhen given a complex task, use the 'plan' tool first to create a structured plan. \
         Then execute the plan step by step, marking each step complete after it's done. \
         For simple one-shot queries, skip planning and respond directly."
    } else {
        ""
    };

    // Build enhanced messages with system context
    let mut enhanced_messages = initial_messages.to_vec();
    if !memory_context.is_empty() || !planning_instruction.is_empty() {
        let combined = format!("{}{}", memory_context, planning_instruction);
        if let Some(first) = enhanced_messages.first_mut() {
            if first.role == crate::llm::Role::System {
                first.content.push_str(&combined);
            }
        } else if !combined.is_empty() {
            enhanced_messages.insert(0, Message::system(&combined));
        }
    }

    let use_streaming = event_tx.is_some();

    for turn in 0..config.max_turns {
        // Inject active plan context before each LLM call
        if let Some(ref plan) = active_plan {
            let plan_ctx = Planner::format_plan_context(plan);
            if let Some(first) = enhanced_messages.first_mut() {
                if first.role == crate::llm::Role::System {
                    // Replace previous plan context to keep it fresh
                    if let Some(pos) = first.content.find("\n\n## Current Execution Plan") {
                        first.content.truncate(pos);
                    }
                    first.content.push_str(&plan_ctx);
                }
            }
        }

        let response: ChatResponse = if use_streaming {
            match provider.stream(&enhanced_messages, &options).await {
                Ok(stream) => {
                    let result = accumulate_stream(stream, event_tx.as_ref().unwrap()).await;
                    match result {
                        Ok(resp) => resp,
                        Err(e) => {
                            let _ = event_tx.as_ref().unwrap()
                                .send(AgentEvent::AgentError(e.to_string()));
                            return AgentResult {
                                content: format!("Error: {}", e),
                                turns: turn,
                                total_tokens,
                                tools_called,
                                stop_reason: StopReason::Error(e.to_string()),
                            };
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx.as_ref().unwrap()
                        .send(AgentEvent::AgentError(e.to_string()));
                    return AgentResult {
                        content: format!("Error calling LLM: {}", e),
                        turns: turn,
                        total_tokens,
                        tools_called,
                        stop_reason: StopReason::Error(e.to_string()),
                    };
                }
            }
        } else {
            match provider.chat(&enhanced_messages, &options).await {
                Ok(r) => r,
                Err(e) => {
                    return AgentResult {
                        content: format!("Error calling LLM: {}", e),
                        turns: turn,
                        total_tokens,
                        tools_called,
                        stop_reason: StopReason::Error(e.to_string()),
                    };
                }
            }
        };

        total_tokens += response.usage.total_tokens;

        if let Some(ref tool_calls) = response.tool_calls {
            if tool_calls.is_empty() {
                if let Some(ref tx) = event_tx {
                    let _ = tx.send(AgentEvent::AgentDone {
                        content: response.content.clone(),
                        turns: turn + 1,
                        total_tokens,
                    });
                }
                return AgentResult {
                    content: response.content,
                    turns: turn + 1,
                    total_tokens,
                    tools_called,
                    stop_reason: StopReason::Completed,
                };
            }

            let mut assistant_msg = Message::assistant(&response.content);
            assistant_msg.tool_calls = Some(tool_calls.clone());
            enhanced_messages.push(assistant_msg);

            for tc in tool_calls {
                if config.show_tool_calls {
                    let args_preview: String = tc.function.arguments.chars().take(100).collect();
                    if let Some(ref tx) = event_tx {
                        let _ = tx.send(AgentEvent::ToolCallStart {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            args: args_preview,
                        });
                    } else {
                        println!(
                            "  {} {}",
                            "Tool:".yellow().bold(),
                            format!("{}", tc.function.name).cyan()
                        );
                        io::stdout().flush().ok();
                    }
                }

                let params: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();

                let permission = permission_manager.check(&tc.function.name, &params);
                let tool_result = match permission {
                    Permission::Deny => {
                        crate::tools::ToolResult::err(format!(
                            "Tool '{}' is not allowed by the permission manager.",
                            tc.function.name
                        ))
                    }
                    Permission::Confirm => {
                        // If we have a TUI channel, request confirmation with diff details
                        if let Some(ref tx) = event_tx {
                            let (confirm_tx, confirm_rx) = tokio::sync::oneshot::channel();
                            let details = build_confirmation_details(&tc.function.name, &params);
                            let _ = tx.send(AgentEvent::ConfirmRequest {
                                details,
                                response_tx: confirm_tx,
                            });
                            match confirm_rx.await {
                                Ok(true) => execute_tool(tool_registry, &tc).await,
                                Ok(false) => crate::tools::ToolResult::err(
                                    format!("User denied permission for tool '{}'", tc.function.name)
                                ),
                                Err(_) => crate::tools::ToolResult::err(String::from("Permission confirmation cancelled.")),
                            }
                        } else {
                            // No TUI — auto-allow in non-interactive mode
                            execute_tool(tool_registry, &tc).await
                        }
                    }
                    Permission::Allow => {
                        execute_tool(tool_registry, &tc).await
                    }
                };

                // Parse plan if this was a plan tool call
                if tc.function.name == "plan" && tool_result.success && planner.is_some() {
                    if let Ok(args) = serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                        active_plan = planner.as_ref().and_then(|p| p.parse_from_tool_result(&args));
                    }
                }

                tools_called.push(tc.function.name.clone());

                if let Some(ref tx) = event_tx {
                    let _ = tx.send(AgentEvent::ToolCallEnd {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        result: if tool_result.success {
                            tool_result.content.clone()
                        } else {
                            tool_result.error.clone().unwrap_or_default()
                        },
                        success: tool_result.success,
                    });
                }

                enhanced_messages.push(Message {
                    role: crate::llm::Role::Tool,
                    content: if tool_result.success {
                        tool_result.content.clone()
                    } else {
                        format!(
                            "Error: {}",
                            tool_result.error.as_deref().unwrap_or("unknown error")
                        )
                    },
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tc.function.name.clone()),
                });
            }

            continue;
        }

        // No tool calls — agent is done
        let stop_reason = match response.finish_reason {
            FinishReason::Stop => StopReason::Completed,
            FinishReason::Length => StopReason::MaxTokens,
            FinishReason::ContentFilter => StopReason::Refusal,
            _ => StopReason::Completed,
        };

        if let Some(ref tx) = event_tx {
            let _ = tx.send(AgentEvent::AgentDone {
                content: response.content.clone(),
                turns: turn + 1,
                total_tokens,
            });
        }

        enhanced_messages.push(Message::assistant(&response.content));
        return AgentResult {
            content: response.content,
            turns: turn + 1,
            total_tokens,
            tools_called,
            stop_reason,
        };
    }

    if let Some(ref tx) = event_tx {
        let _ = tx.send(AgentEvent::AgentDone {
            content: "Maximum turns reached.".into(),
            turns: config.max_turns,
            total_tokens,
        });
    }

    AgentResult {
        content: "Maximum conversation turns reached.".into(),
        turns: config.max_turns,
        total_tokens,
        tools_called,
        stop_reason: StopReason::MaxTurns,
    }
}

async fn execute_tool(
    registry: &ToolRegistry,
    tool_call: &ToolCall,
) -> crate::tools::ToolResult {
    let tool = match registry.get(&tool_call.function.name) {
        Some(t) => t,
        None => {
            return crate::tools::ToolResult::err(format!(
                "Unknown tool: {}",
                tool_call.function.name
            ));
        }
    };

    let params: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();

    tool.execute(params).await
}

/// Accumulate a streaming response into a ChatResponse, sending text deltas to the TUI.
async fn accumulate_stream(
    mut stream: Box<dyn futures::Stream<Item = Result<StreamChunk, crate::llm::LLMError>> + Send + Unpin>,
    event_tx: &UnboundedSender<AgentEvent>,
) -> Result<ChatResponse, crate::llm::LLMError> {
    let mut content = String::new();
    let mut finish_reason = FinishReason::Stop;
    let usage = crate::llm::Usage::default();

    // Accumulate tool calls from deltas
    let mut tool_call_deltas: std::collections::HashMap<u32, ToolCallBuilder> =
        std::collections::HashMap::new();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => {
                if let Some(ref text) = chunk.content {
                    content.push_str(text);
                    let _ = event_tx.send(AgentEvent::TextDelta(text.clone()));
                }

                if let Some(ref tc_delta) = chunk.tool_call {
                    let entry = tool_call_deltas
                        .entry(tc_delta.index)
                        .or_insert_with(ToolCallBuilder::new);
                    if let Some(ref id) = tc_delta.id {
                        entry.id = id.clone();
                    }
                    if let Some(ref name) = tc_delta.name {
                        entry.name = name.clone();
                    }
                    if let Some(ref args) = tc_delta.arguments {
                        entry.arguments.push_str(args);
                    }
                }

                if let Some(ref reason) = chunk.finish_reason {
                    finish_reason = reason.clone();
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Build tool calls from accumulated deltas
    let tool_calls: Option<Vec<ToolCall>> = if tool_call_deltas.is_empty() {
        None
    } else {
        let mut calls: Vec<ToolCall> = tool_call_deltas
            .into_values()
            .map(|b| ToolCall {
                id: b.id,
                call_type: "function".to_string(),
                function: crate::llm::FunctionCall {
                    name: b.name,
                    arguments: b.arguments,
                },
            })
            .collect();
        calls.sort_by_key(|c| c.id.clone());
        Some(calls)
    };

    Ok(ChatResponse {
        content,
        tool_calls,
        finish_reason,
        usage,
    })
}

struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCallBuilder {
    fn new() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        }
    }
}

/// Build confirmation details with file diff context for TUI display
fn build_confirmation_details(tool_name: &str, params: &serde_json::Value) -> ConfirmationDetails {
    let file_path = params["file_path"].as_str().map(String::from);
    let command = params["command"].as_str().map(String::from);

    let (operation, summary) = match tool_name {
        "write" => {
            let path = file_path.as_deref().unwrap_or("unknown");
            let content = params["content"].as_str().unwrap_or("");
            let preview: String = content.chars().take(100).collect();
            (
                ConfirmationType::WriteFile,
                format!("Write {} ({} bytes): {}", path, content.len(), preview),
            )
        }
        "edit" => {
            let path = file_path.as_deref().unwrap_or("unknown");
            let old = params["old_string"].as_str().unwrap_or("");
            let new = params["new_string"].as_str().unwrap_or("");
            (
                ConfirmationType::EditFile,
                format!("Edit {}: replace '{}' → '{}'", path, old, new),
            )
        }
        "bash" => {
            let cmd = command.as_deref().unwrap_or("unknown");
            (
                ConfirmationType::RunCommand,
                format!("Run: {}", cmd),
            )
        }
        "webfetch" => {
            let url = params["url"].as_str().unwrap_or("unknown");
            (ConfirmationType::WebFetch, format!("Fetch URL: {}", url))
        }
        _ => (
            ConfirmationType::Generic,
            format!("{}: {:?}", tool_name, params),
        ),
    };

    // Read existing file content for diff preview
    let old_content = if matches!(operation, ConfirmationType::WriteFile | ConfirmationType::EditFile) {
        file_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok())
    } else {
        None
    };

    let new_content = match tool_name {
        "write" => params["content"].as_str().map(String::from),
        "edit" => {
            if let (Some(path), Some(old_str), Some(new_str)) = (
                file_path.as_ref(),
                params["old_string"].as_str(),
                params["new_string"].as_str(),
            ) {
                std::fs::read_to_string(path).ok().map(|content| {
                    let replace_all = params["replace_all"].as_bool().unwrap_or(false);
                    if replace_all {
                        content.replace(old_str, new_str)
                    } else {
                        content.replacen(old_str, new_str, 1)
                    }
                })
            } else {
                None
            }
        }
        _ => None,
    };

    ConfirmationDetails {
        tool_name: tool_name.to_string(),
        summary,
        file_path,
        old_content,
        new_content,
        operation,
    }
}
