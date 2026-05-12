use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::agent::{AgentEvent, ConfirmationDetails, ConfirmationType};
use crate::tui::widgets;

/// A chat message in the TUI
#[derive(Debug, Clone)]
pub struct ChatBubble {
    pub role: BubbleRole,
    pub content: String,
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BubbleRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub name: String,
    pub status: ToolStatus,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Running,
    Success,
    Failed,
}

/// Input mode for the TUI
#[derive(Debug)]
pub enum InputMode {
    /// Typing a message
    Normal,
    /// Agent is processing
    Processing,
    /// Waiting for user to confirm a tool
    Confirm {
        details: ConfirmationDetails,
        response_tx: Option<tokio::sync::oneshot::Sender<bool>>,
    },
}

impl PartialEq for InputMode {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other),
            (InputMode::Normal, InputMode::Normal) |
            (InputMode::Processing, InputMode::Processing) |
            (InputMode::Confirm { .. }, InputMode::Confirm { .. })
        )
    }
}



impl Clone for InputMode {
    fn clone(&self) -> Self {
        match self {
            InputMode::Normal => InputMode::Normal,
            InputMode::Processing => InputMode::Processing,
            InputMode::Confirm { .. } => InputMode::Confirm {
                // Can't clone sender; create a dummy that won't be used for sending
                details: ConfirmationDetails {
                    tool_name: String::new(),
                    summary: String::new(),
                    file_path: None,
                    old_content: None,
                    new_content: None,
                    operation: ConfirmationType::Generic,
                },
                response_tx: None,
            },
        }
    }
}

/// The main TUI application state
pub struct App {
    /// Chat message history
    pub messages: Vec<ChatBubble>,
    /// Current input text
    pub input: String,
    /// Input mode
    pub input_mode: InputMode,
    /// Character index in input
    pub cursor_pos: usize,
    /// Scroll position in chat history
    pub scroll_offset: u16,
    /// Current model name
    pub model: String,
    /// Current provider name
    pub provider: String,
    /// Token count for current session
    pub token_count: u32,
    /// Turn count for current agent run
    pub turn_count: u32,
    /// Streaming text buffer (during agent processing)
    pub streaming_text: String,
    /// Active tool calls being visualized
    pub active_tools: Vec<ToolCallRecord>,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Receiver for agent events
    pub agent_rx: Option<UnboundedReceiver<AgentEvent>>,
    /// Sender for user messages (clone to pass to agent task)
    pub agent_tx: Option<UnboundedSender<String>>,
    /// Error message to display
    pub error_msg: Option<String>,
}

impl App {
    pub fn new(model: String, provider: String) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            input_mode: InputMode::Normal,
            cursor_pos: 0,
            scroll_offset: 0,
            model,
            provider,
            token_count: 0,
            turn_count: 0,
            streaming_text: String::new(),
            active_tools: Vec::new(),
            should_quit: false,
            agent_rx: None,
            agent_tx: None,
            error_msg: None,
        }
    }

    /// Add a user message to the chat
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatBubble {
            role: BubbleRole::User,
            content,
            tool_calls: Vec::new(),
        });
    }

    /// Begin streaming — reset stream state
    pub fn start_streaming(&mut self) {
        self.streaming_text.clear();
        self.active_tools.clear();
        self.input_mode = InputMode::Processing;
        self.error_msg = None;
    }

    /// Process an agent event
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta(text) => {
                self.streaming_text.push_str(&text);
            }
            AgentEvent::ToolCallStart { id: _, name, args } => {
                self.active_tools.push(ToolCallRecord {
                    name,
                    status: ToolStatus::Running,
                    preview: args.chars().take(100).collect(),
                });
            }
            AgentEvent::ToolCallEnd { id: _, name, result, success } => {
                if let Some(tool) = self.active_tools.iter_mut().find(|t| t.name == name) {
                    tool.status = if success { ToolStatus::Success } else { ToolStatus::Failed };
                    tool.preview = result.chars().take(200).collect();
                }
            }
            AgentEvent::AgentDone { content, turns, total_tokens } => {
                let tool_calls: Vec<ToolCallRecord> =
                    std::mem::take(&mut self.active_tools);

                let final_content = if content.is_empty() {
                    std::mem::take(&mut self.streaming_text)
                } else {
                    content
                };

                self.messages.push(ChatBubble {
                    role: BubbleRole::Assistant,
                    content: final_content,
                    tool_calls,
                });

                self.streaming_text.clear();
                self.turn_count = turns;
                self.token_count += total_tokens;
                self.input_mode = InputMode::Normal;
            }
            AgentEvent::AgentError(err) => {
                self.error_msg = Some(err);
                self.messages.push(ChatBubble {
                    role: BubbleRole::System,
                    content: self.error_msg.clone().unwrap_or_default(),
                    tool_calls: Vec::new(),
                });
                self.streaming_text.clear();
                self.active_tools.clear();
                self.input_mode = InputMode::Normal;
            }
            AgentEvent::ConfirmRequest { details, response_tx } => {
                self.input_mode = InputMode::Confirm {
                    details,
                    response_tx: Some(response_tx),
                };
            }
        }
    }

    /// Move cursor left in input
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right in input
    pub fn cursor_right(&mut self) {
        let len = self.input.chars().count();
        if self.cursor_pos < len {
            self.cursor_pos += 1;
        }
    }

    /// Move cursor to start of input
    pub fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Move cursor to end of input
    pub fn cursor_end(&mut self) {
        self.cursor_pos = self.input.chars().count();
    }

    /// Insert character at cursor position
    pub fn input_char(&mut self, c: char) {
        let pos = self.input.char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len());
        self.input.insert(pos, c);
        self.cursor_pos += 1;
    }

    /// Delete character before cursor
    pub fn delete_backward(&mut self) {
        if self.cursor_pos > 0 {
            let pos = self.input.char_indices()
                .nth(self.cursor_pos - 1)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(pos);
            self.cursor_pos -= 1;
        }
    }

    /// Delete character at cursor
    pub fn delete_forward(&mut self) {
        let len = self.input.chars().count();
        if self.cursor_pos < len {
            let pos = self.input.char_indices()
                .nth(self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(self.input.len());
            self.input.remove(pos);
        }
    }

    /// Get the current input content and reset it
    pub fn take_input(&mut self) -> String {
        self.cursor_pos = 0;
        std::mem::take(&mut self.input)
    }

    /// Add a system message to the chat
    fn add_system_message(&mut self, content: String) {
        self.messages.push(ChatBubble {
            role: BubbleRole::System,
            content,
            tool_calls: Vec::new(),
        });
    }

    /// Handle a slash command. Returns true if the command was handled.
    fn handle_slash_command(&mut self, input: &str) -> bool {
        let parts: Vec<&str> = input.splitn(3, ' ').collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "/help" => {
                let help = [
                    "Slash Commands:",
                    "  /help          Show this help",
                    "  /clear         Reset the conversation context",
                    "  /model <name>  Switch the current model",
                    "  /provider <p>  Switch provider (openai, anthropic, deepseek, ollama)",
                    "  /tools         List available tools",
                    "  /models        List available models",
                    "  /save          Save the current session",
                    "  /exit, /quit   Exit RustCC",
                ].join("\n");
                self.add_system_message(help);
                true
            }
            "/clear" | "/reset" => {
                self.messages.clear();
                self.token_count = 0;
                self.turn_count = 0;
                self.add_system_message("Session reset. Context cleared.".into());
                true
            }
            "/model" => {
                if parts.len() >= 2 {
                    let model = parts[1].to_string();
                    self.add_system_message(format!("Switched model to: {}", model));
                    self.model = model;
                } else {
                    self.add_system_message(format!("Current model: {}\nUsage: /model <model-name>", self.model));
                }
                true
            }
            "/provider" => {
                if parts.len() >= 2 {
                    self.provider = parts[1].to_string();
                    self.add_system_message(format!("Switched provider to: {}", self.provider));
                } else {
                    self.add_system_message(format!("Current provider: {}\nUsage: /provider <openai|anthropic|deepseek|ollama>", self.provider));
                }
                true
            }
            "/tools" => {
                self.add_system_message("Available tools: read, write, edit, grep, glob, bash, webfetch, websearch, plan".into());
                true
            }
            "/models" => {
                self.add_system_message(format!("Current model: {} (provider: {})", self.model, self.provider));
                true
            }
            "/save" => {
                self.add_system_message("Session save triggered.".into());
                // Send a save signal through the agent channel
                if let Some(ref tx) = self.agent_tx {
                    let _ = tx.send("__rustcc_save__".into());
                }
                true
            }
            "/exit" | "/quit" | "/q" => {
                self.should_quit = true;
                true
            }
            _ => false,
        }
    }
}

/// Run the TUI application
pub fn run_tui(
    mut app: App,
    agent_tx: UnboundedSender<String>,
    agent_rx: UnboundedReceiver<AgentEvent>,
) -> io::Result<()> {
    // Set up terminal
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;

    app.agent_rx = Some(agent_rx);
    app.agent_tx = Some(agent_tx);

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Main event loop
    let result = run_app_loop(&mut terminal, &mut app);

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
    )?;

    result
}

fn run_app_loop<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        // Draw the current frame
        terminal.draw(|f| draw_ui(f, app))?;

        // Drain agent events into a buffer, then process
        let mut pending_events: Vec<AgentEvent> = Vec::new();
        if let Some(ref mut rx) = app.agent_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => pending_events.push(event),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        app.error_msg = Some("Agent channel disconnected".into());
                        break;
                    }
                }
            }
        }
        for event in pending_events {
            app.handle_agent_event(event);
        }

        // Only block on keyboard events when not processing
        let timeout = if app.input_mode == InputMode::Processing {
            std::time::Duration::from_millis(50)
        } else {
            std::time::Duration::from_millis(200)
        };

        if event::poll(timeout)? {
            let ev = event::read()?;
            if handle_input_event(ev, app) {
                break; // quit
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Handle a single input event. Returns true if app should quit.
fn handle_input_event(ev: Event, app: &mut App) -> bool {
    match ev {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            // Global shortcuts
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.should_quit = true;
                return true;
            }
            // Handle Esc for confirm mode
            if key.code == KeyCode::Esc {
                if matches!(app.input_mode, InputMode::Confirm { .. }) {
                    // Will be handled in the confirm branch below
                } else if app.input_mode == InputMode::Processing {
                    app.error_msg = Some("Agent cancelled.".into());
                    app.input_mode = InputMode::Normal;
                    app.streaming_text.clear();
                    app.active_tools.clear();
                    return false;
                } else {
                    app.input_mode = InputMode::Normal;
                    return false;
                }
            }

            let current_mode = std::mem::replace(&mut app.input_mode, InputMode::Normal);
            match current_mode {
                InputMode::Confirm { details, response_tx } => {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Some(tx) = response_tx {
                                let _ = tx.send(true);
                            }
                            app.input_mode = InputMode::Processing;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            if let Some(tx) = response_tx {
                                let _ = tx.send(false);
                            }
                            app.input_mode = InputMode::Processing;
                        }
                        _ => {
                            // Put back if no decision made
                            app.input_mode = InputMode::Confirm { details, response_tx };
                        }
                    }
                    return false;
                }
                InputMode::Processing => {
                    // During processing, only Esc cancels (handled above)
                    app.input_mode = current_mode;
                }
                InputMode::Normal => {
                    match key.code {
                        KeyCode::Enter => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                app.input_char('\n');
                            } else if !app.input.trim().is_empty() {
                                let msg = app.take_input();
                                if msg.starts_with('/') && app.handle_slash_command(&msg) {
                                    // Handled locally
                                } else {
                                    app.add_user_message(msg.clone());
                                    app.start_streaming();
                                    // Send to agent
                                    if let Some(ref tx) = app.agent_tx {
                                        let _ = tx.send(msg);
                                    }
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            app.input_char(c);
                        }
                        KeyCode::Backspace => {
                            app.delete_backward();
                        }
                        KeyCode::Delete => {
                            app.delete_forward();
                        }
                        KeyCode::Left => {
                            app.cursor_left();
                        }
                        KeyCode::Right => {
                            app.cursor_right();
                        }
                        KeyCode::Home => {
                            app.cursor_home();
                        }
                        KeyCode::End => {
                            app.cursor_end();
                        }
                        KeyCode::Up => {
                            if app.scroll_offset > 0 {
                                app.scroll_offset -= 1;
                            }
                        }
                        KeyCode::Down => {
                            app.scroll_offset += 1;
                        }
                        KeyCode::PageUp => {
                            app.scroll_offset = app.scroll_offset.saturating_sub(10);
                        }
                        KeyCode::PageDown => {
                            app.scroll_offset += 10;
                        }
                        _ => {}
                    }
                }
            }
        }
        Event::Resize(_, _) => {
            // Terminal was resized — ratatui handles re-render automatically
        }
        _ => {}
    }
    false
}

/// Draw the entire UI
fn draw_ui(f: &mut Frame, app: &App) {
    let area = f.area();

    // Main vertical layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Status bar
            Constraint::Min(0),     // Messages
            Constraint::Length(3),  // Input area
        ])
        .split(area);

    // Status bar
    widgets::draw_status_bar(f, main_chunks[0], app);

    // Messages area
    widgets::draw_messages(f, main_chunks[1], app);

    // Input area
    widgets::draw_input(f, main_chunks[2], app);
}

// Drawing delegated to widget modules
