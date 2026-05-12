use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::llm::ProviderType;
use crate::storage::config::{Config, OpenAIConfig, AnthropicConfig, DeepSeekConfig, OllamaConfig};

/// Setup wizard step
#[derive(Debug, Clone, PartialEq)]
enum Step {
    Welcome,
    Theme,
    Mode,
    CloudProvider,
    CloudApiKey,
    CloudModel,
    LocalUrl,
    LocalModel,
    Done,
}

/// Setup state
pub struct SetupWizard {
    step: Step,
    theme: String,
    mode: String,
    provider: String,
    api_key: String,
    model: String,
    ollama_url: String,
    cursor_pos: usize,
    error_msg: Option<String>,
    should_quit: bool,
}

impl SetupWizard {
    pub fn new() -> Self {
        Self {
            step: Step::Welcome,
            theme: "dark".into(),
            mode: String::new(),
            provider: String::new(),
            api_key: String::new(),
            model: String::new(),
            ollama_url: "http://localhost:11434".into(),
            cursor_pos: 0,
            error_msg: None,
            should_quit: false,
        }
    }

    fn next_step(&mut self) {
        self.error_msg = None;
        self.cursor_pos = 0;
        self.step = match &self.step {
            Step::Welcome => Step::Theme,
            Step::Theme => Step::Mode,
            Step::Mode => {
                if self.mode == "local" { Step::LocalUrl }
                else if self.mode == "hybrid" {
                    // Set defaults for both
                    self.provider = "openai".into();
                    Step::CloudProvider
                } else { Step::CloudProvider }
            }
            Step::CloudProvider => Step::CloudApiKey,
            Step::CloudApiKey => {
                if self.api_key.trim().is_empty() {
                    self.error_msg = Some("API key cannot be empty.".into());
                    return;
                }
                Step::CloudModel
            }
            Step::CloudModel => Step::Done,
            Step::LocalUrl => Step::LocalModel,
            Step::LocalModel => {
                if self.model.trim().is_empty() {
                    self.error_msg = Some("Model name cannot be empty.".into());
                    return;
                }
                Step::Done
            }
            Step::Done => Step::Done,
        };
    }

    fn prev_step(&mut self) {
        self.error_msg = None;
        self.cursor_pos = 0;
        self.step = match &self.step {
            Step::Welcome => Step::Welcome,
            Step::Theme => Step::Welcome,
            Step::Mode => Step::Theme,
            Step::CloudProvider => Step::Mode,
            Step::CloudApiKey => Step::CloudProvider,
            Step::CloudModel => Step::CloudApiKey,
            Step::LocalUrl => Step::Mode,
            Step::LocalModel => Step::LocalUrl,
            Step::Done => {
                if self.mode == "local" { Step::LocalModel }
                else { Step::CloudModel }
            }
        };
    }

    /// Build the final config from setup choices
    fn build_config(&self) -> Config {
        let mut config = Config::default();
        config.ui.theme = self.theme.clone();

        // Remove default provider configs
        config.providers.openai = None;
        config.providers.anthropic = None;
        config.providers.deepseek = None;
        config.providers.ollama = None;

        match self.mode.as_str() {
            "local" => {
                config.general.default_provider = ProviderType::Ollama;
                config.general.default_model = self.model.clone();
                config.mode = Some(crate::storage::config::OperationMode::Local);
                config.providers.ollama = Some(OllamaConfig {
                    base_url: self.ollama_url.clone(),
                    model: self.model.clone(),
                });
            }
            "hybrid" => {
                // Set cloud as primary, local as fallback
                self.setup_cloud_provider(&mut config);
                config.providers.ollama = Some(OllamaConfig {
                    base_url: "http://localhost:11434".into(),
                    model: "codellama".into(),
                });
                config.mode = Some(crate::storage::config::OperationMode::Hybrid);
            }
            _ => {
                // Cloud-only
                self.setup_cloud_provider(&mut config);
                config.mode = Some(crate::storage::config::OperationMode::Cloud);
            }
        }

        config
    }

    fn setup_cloud_provider(&self, config: &mut Config) {
        match self.provider.as_str() {
            "openai" => {
                config.general.default_provider = ProviderType::OpenAI;
                config.general.default_model = if self.model.is_empty() { "gpt-4o".into() } else { self.model.clone() };
                config.providers.openai = Some(OpenAIConfig {
                    api_key: self.api_key.clone(),
                    base_url: None,
                    model: config.general.default_model.clone(),
                });
            }
            "anthropic" => {
                config.general.default_provider = ProviderType::Anthropic;
                config.general.default_model = if self.model.is_empty() { "claude-sonnet-4-6".into() } else { self.model.clone() };
                config.providers.anthropic = Some(AnthropicConfig {
                    api_key: self.api_key.clone(),
                    model: config.general.default_model.clone(),
                });
            }
            "deepseek" => {
                config.general.default_provider = ProviderType::DeepSeek;
                config.general.default_model = if self.model.is_empty() { "deepseek-chat".into() } else { self.model.clone() };
                config.providers.deepseek = Some(DeepSeekConfig {
                    api_key: self.api_key.clone(),
                    model: config.general.default_model.clone(),
                });
            }
            _ => {}
        }
    }
}

/// Run the setup wizard. Returns Some(config) if completed, None if cancelled.
pub fn run_setup() -> io::Result<Option<Config>> {
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut wizard = SetupWizard::new();
    let result = run_wizard_loop(&mut terminal, &mut wizard);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
    )?;

    result
}

fn run_wizard_loop<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    wizard: &mut SetupWizard,
) -> io::Result<Option<Config>> {
    loop {
        terminal.draw(|f| draw_setup(f, wizard))?;

        if wizard.should_quit {
            return Ok(None);
        }

        if wizard.step == Step::Done {
            return Ok(Some(wizard.build_config()));
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            let ev = event::read()?;
            handle_setup_input(ev, wizard);
        }
    }
}

fn handle_setup_input(ev: Event, wizard: &mut SetupWizard) {
    match ev {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                wizard.should_quit = true;
                return;
            }

            match wizard.step {
                Step::Welcome => {
                    if key.code == KeyCode::Enter {
                        wizard.next_step();
                    }
                }
                Step::Theme => {
                    match key.code {
                        KeyCode::Char('1') | KeyCode::Char('d') => {
                            wizard.theme = "dark".into();
                            wizard.next_step();
                        }
                        KeyCode::Char('2') | KeyCode::Char('l') => {
                            wizard.theme = "light".into();
                            wizard.next_step();
                        }
                        _ => {}
                    }
                }
                Step::Mode => {
                    match key.code {
                        KeyCode::Char('1') => {
                            wizard.mode = "cloud".into();
                            wizard.next_step();
                        }
                        KeyCode::Char('2') => {
                            wizard.mode = "local".into();
                            wizard.next_step();
                        }
                        KeyCode::Char('3') => {
                            wizard.mode = "hybrid".into();
                            wizard.next_step();
                        }
                        _ => {}
                    }
                }
                Step::CloudProvider => {
                    match key.code {
                        KeyCode::Char('1') => {
                            wizard.provider = "openai".into();
                            wizard.next_step();
                        }
                        KeyCode::Char('2') => {
                            wizard.provider = "anthropic".into();
                            wizard.next_step();
                        }
                        KeyCode::Char('3') => {
                            wizard.provider = "deepseek".into();
                            wizard.next_step();
                        }
                        KeyCode::Esc | KeyCode::Backspace => wizard.prev_step(),
                        _ => {}
                    }
                }
                Step::CloudApiKey => {
                    match key.code {
                        KeyCode::Enter => wizard.next_step(),
                        KeyCode::Esc => wizard.prev_step(),
                        KeyCode::Char(c) => wizard.api_key.push(c),
                        KeyCode::Backspace => { wizard.api_key.pop(); }
                        _ => {}
                    }
                }
                Step::CloudModel => {
                    match key.code {
                        KeyCode::Enter => wizard.next_step(),
                        KeyCode::Esc => wizard.prev_step(),
                        KeyCode::Char(c) => wizard.model.push(c),
                        KeyCode::Backspace => { wizard.model.pop(); }
                        _ => {}
                    }
                }
                Step::LocalUrl => {
                    match key.code {
                        KeyCode::Enter => wizard.next_step(),
                        KeyCode::Esc => wizard.prev_step(),
                        KeyCode::Char(c) => wizard.ollama_url.push(c),
                        KeyCode::Backspace => { wizard.ollama_url.pop(); }
                        _ => {}
                    }
                }
                Step::LocalModel => {
                    match key.code {
                        KeyCode::Enter => wizard.next_step(),
                        KeyCode::Esc => wizard.prev_step(),
                        KeyCode::Char(c) => wizard.model.push(c),
                        KeyCode::Backspace => { wizard.model.pop(); }
                        _ => {}
                    }
                }
                Step::Done => {}
            }
        }
        _ => {}
    }
}

fn draw_setup(f: &mut Frame, wizard: &SetupWizard) {
    let area = f.area();
    let bg = Color::Rgb(18, 18, 24);

    // Center the dialog
    let dialog_w = 60.min(area.width.saturating_sub(4));
    let dialog_h = 18.min(area.height.saturating_sub(4));
    let dialog_x = (area.width.saturating_sub(dialog_w)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_h)) / 2;
    let dialog_area = Rect {
        x: dialog_x,
        y: dialog_y,
        width: dialog_w,
        height: dialog_h,
    };

    f.render_widget(Clear, dialog_area);

    let border_style = Style::default()
        .fg(Color::Rgb(99, 102, 241))
        .bg(bg);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(bg))
        .title(" RustCC Setup Wizard ")
        .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(inner);

    // Content area
    let content = match wizard.step {
        Step::Welcome => welcome_content(),
        Step::Theme => theme_content(wizard),
        Step::Mode => mode_content(wizard),
        Step::CloudProvider => cloud_provider_content(wizard),
        Step::CloudApiKey => api_key_content(wizard),
        Step::CloudModel => model_content(wizard, true),
        Step::LocalUrl => local_url_content(wizard),
        Step::LocalModel => model_content(wizard, false),
        Step::Done => done_content(wizard),
    };

    let paragraph = Paragraph::new(content)
        .style(Style::default().bg(bg))
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, chunks[0]);

    // Help bar
    let help_text = match wizard.step {
        Step::Welcome => "Press Enter to begin setup",
        Step::Theme => "Press 1-2 to select, or d/l",
        Step::Mode => "Press 1-3 to select mode",
        Step::CloudProvider => "Press 1-3 to select provider, Esc to go back",
        Step::CloudApiKey | Step::CloudModel | Step::LocalUrl | Step::LocalModel => {
            "Type value, Enter to confirm, Esc to go back"
        }
        Step::Done => "Setup complete! Press Enter to start.",
    };

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(help_text, Style::default().fg(Color::DarkGray).bg(bg)),
    ]))
    .style(Style::default().bg(bg));
    f.render_widget(hint, chunks[2]);

    // Error message
    if let Some(ref err) = wizard.error_msg {
        let err_p = Paragraph::new(Line::from(vec![
            Span::styled(err.clone(), Style::default().fg(Color::Red).bg(bg)),
        ]))
        .style(Style::default().bg(bg));
        f.render_widget(err_p, chunks[3]);
    }

    // Progress indicator
    let steps = ["Welcome", "Theme", "Mode", "Provider", "Key", "Model", "Done"];
    let current = match wizard.step {
        Step::Welcome => 0,
        Step::Theme => 1,
        Step::Mode => 2,
        Step::CloudProvider | Step::LocalUrl => 3,
        Step::CloudApiKey => 4,
        Step::CloudModel | Step::LocalModel => 5,
        Step::Done => 6,
    };
    let progress: Vec<Span> = steps.iter().enumerate().map(|(i, s)| {
        if i == current {
            Span::styled(format!(" ●{} ", s), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        } else if i < current {
            Span::styled(format!(" ✓{} ", s), Style::default().fg(Color::Green))
        } else {
            Span::styled(format!(" ○{} ", s), Style::default().fg(Color::DarkGray))
        }
    }).collect();

    let progress_bar = Paragraph::new(Line::from(progress))
        .style(Style::default().bg(bg));
    f.render_widget(progress_bar, chunks[1]);
}

fn welcome_content() -> Text<'static> {
    Text::from(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Welcome to RustCC!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  RustCC is a high-performance AI Agent CLI tool.", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  It helps with software engineering tasks using LLMs.", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  You'll configure:", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("    1. UI Theme", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("    2. Deployment mode (Cloud / Local / Hybrid)", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("    3. Model provider and API key", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press Enter to get started →", Style::default().fg(Color::Yellow)),
        ]),
    ])
}

fn theme_content(wizard: &SetupWizard) -> Text<'static> {
    let dark_style = if wizard.theme == "dark" {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let light_style = if wizard.theme == "light" {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let dark_prefix = if wizard.theme == "dark" { "  ▶" } else { "   " };
    let light_prefix = if wizard.theme == "light" { "  ▶" } else { "   " };

    Text::from(vec![
        Line::from(vec![
            Span::styled("  Choose UI Theme", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 1. Dark  (recommended for terminals) ✓", dark_prefix), dark_style),
        ]),
        Line::from(vec![
            Span::styled(format!("{} 2. Light (for bright environments) ✓", light_prefix), light_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Theme can be changed later in config.toml", Style::default().fg(Color::DarkGray)),
        ]),
    ])
}

fn mode_content(wizard: &SetupWizard) -> Text<'static> {
    let style_for = |current: bool| -> Style {
        if current { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) }
        else { Style::default().fg(Color::Gray) }
    };
    let prefix = |current: bool| -> &str { if current { "  ▶" } else { "   " } };

    Text::from(vec![
        Line::from(vec![
            Span::styled("  Choose Deployment Mode", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 1. Cloud API", prefix(wizard.mode == "cloud")), style_for(wizard.mode == "cloud")),
        ]),
        Line::from(vec![
            Span::styled("     Use OpenAI, Anthropic, or DeepSeek API", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 2. Local (Ollama)", prefix(wizard.mode == "local")), style_for(wizard.mode == "local")),
        ]),
        Line::from(vec![
            Span::styled("     Run models locally, zero data leaves your machine", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 3. Hybrid", prefix(wizard.mode == "hybrid")), style_for(wizard.mode == "hybrid")),
        ]),
        Line::from(vec![
            Span::styled("     Simple queries → local, complex → cloud (auto-routed)", Style::default().fg(Color::DarkGray)),
        ]),
    ])
}

fn cloud_provider_content(wizard: &SetupWizard) -> Text<'static> {
    let style_for = |current: bool| -> Style {
        if current { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) }
        else { Style::default().fg(Color::Gray) }
    };
    let prefix = |current: bool| -> &str { if current { "  ▶" } else { "   " } };

    Text::from(vec![
        Line::from(vec![
            Span::styled("  Choose Cloud Provider", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 1. OpenAI", prefix(wizard.provider == "openai")), style_for(wizard.provider == "openai")),
        ]),
        Line::from(vec![
            Span::styled("     GPT-4o, GPT-4o-mini, GPT-4-turbo", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 2. Anthropic", prefix(wizard.provider == "anthropic")), style_for(wizard.provider == "anthropic")),
        ]),
        Line::from(vec![
            Span::styled("     Claude Opus 4.7, Sonnet 4.6, Haiku 4.5", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} 3. DeepSeek", prefix(wizard.provider == "deepseek")), style_for(wizard.provider == "deepseek")),
        ]),
        Line::from(vec![
            Span::styled("     DeepSeek-Chat, DeepSeek-Reasoner (OpenAI-compat)", Style::default().fg(Color::DarkGray)),
        ]),
    ])
}

fn api_key_content(wizard: &SetupWizard) -> Text<'static> {
    let masked = if wizard.api_key.is_empty() {
        "(type your API key)".to_string()
    } else {
        let visible = &wizard.api_key[..4.min(wizard.api_key.len())];
        format!("{}...", visible)
    };

    Text::from(vec![
        Line::from(vec![
            Span::styled(format!("  Enter {} API Key", wizard.provider.to_uppercase()), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  Key: {}", masked), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  The key is encrypted and stored locally.", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("  Get your key from the provider's dashboard.", Style::default().fg(Color::DarkGray)),
        ]),
    ])
}

fn model_content(wizard: &SetupWizard, is_cloud: bool) -> Text<'static> {
    let default_models = if is_cloud {
        match wizard.provider.as_str() {
            "openai" => "gpt-4o, gpt-4o-mini, gpt-4-turbo",
            "anthropic" => "claude-sonnet-4-6, claude-opus-4-7, claude-haiku-4-5",
            "deepseek" => "deepseek-chat, deepseek-reasoner",
            _ => "(enter model name)",
        }
    } else {
        "codellama, llama3, mistral, gemma"
    };

    let current = if wizard.model.is_empty() { "(press Enter for default)" } else { &wizard.model };

    Text::from(vec![
        Line::from(vec![
            Span::styled("  Choose Model", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  Model: {}", current), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  Available: {}", default_models), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("  Press Enter to use the default, or type a custom model name.", Style::default().fg(Color::DarkGray)),
        ]),
    ])
}

fn local_url_content(wizard: &SetupWizard) -> Text<'static> {
    Text::from(vec![
        Line::from(vec![
            Span::styled("  Configure Ollama", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  URL: {}", wizard.ollama_url), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Default Ollama server: http://localhost:11434", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("  Install Ollama: https://ollama.com/download", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("  Press Enter to keep the default, or type a custom URL.", Style::default().fg(Color::DarkGray)),
        ]),
    ])
}

fn done_content(wizard: &SetupWizard) -> Text<'static> {
    let mode_label = match wizard.mode.as_str() {
        "local" => "Local (Ollama)",
        "hybrid" => "Hybrid (Local + Cloud)",
        _ => "Cloud API",
    };
    let provider_label = if wizard.mode == "local" {
        "Ollama".to_string()
    } else {
        wizard.provider.to_uppercase()
    };

    Text::from(vec![
        Line::from(vec![
            Span::styled("  ✓ Setup Complete!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Configuration Summary:", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("    Theme:    {}", wizard.theme), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled(format!("    Mode:     {}", mode_label), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled(format!("    Provider: {}", provider_label), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled(format!("    Model:    {}", if wizard.model.is_empty() { "(default)" } else { &wizard.model }), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Config saved. Launching RustCC...", Style::default().fg(Color::Yellow)),
        ]),
    ])
}
