// tui/app.rs — Main TUI application state and event loop
//
// The TUI runs a synchronous crossterm event loop on the main thread.
// Agent events arrive through a std::sync::mpsc channel and are
// drained each frame. User input is forwarded to the agent via a
// tokio::sync::mpsc channel.

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use mengxi_agent::AgentEvent;

use super::agent_bridge;
use super::layout;

// ---------------------------------------------------------------------------
// Chat message types
// ---------------------------------------------------------------------------

/// Messages stored in the chat history.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
    System(String),
    ToolCall { name: String, status: ToolStatus },
}

#[derive(Debug, Clone)]
pub enum ToolStatus {
    Running,
    Success(#[allow(dead_code)] String),
    Error(#[allow(dead_code)] String),
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// The main TUI application.
pub struct App {
    /// Chat message history.
    pub messages: Vec<ChatMessage>,
    /// Current input text.
    pub input: String,
    /// Scroll offset for the chat panel.
    pub scroll_offset: usize,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Currently active panel (0 = chat, 1 = input).
    pub active_panel: usize,
    /// Last result data for the result panel.
    pub last_result: Option<String>,
    /// Command history for Up/Down navigation.
    pub history: Vec<String>,
    /// Current position in command history (-1 means not navigating).
    pub history_pos: isize,

    // --- Agent integration ---
    /// Channel to send user messages to the background agent task.
    user_msg_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    /// Channel to receive agent events (polled via try_recv).
    agent_event_rx: Option<std::sync::mpsc::Receiver<AgentEvent>>,
    /// Whether the agent is currently generating a response.
    generating: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            messages: vec![ChatMessage::System(
                "Welcome to mengxi chat. Type a message or /help for commands.".to_string(),
            )],
            input: String::new(),
            scroll_offset: 0,
            should_quit: false,
            active_panel: 1,
            last_result: None,
            history: Vec::new(),
            history_pos: -1,
            user_msg_tx: None,
            agent_event_rx: None,
            generating: false,
        }
    }

    /// Inject agent communication channels.
    pub fn with_agent_channels(
        mut self,
        user_msg_tx: tokio::sync::mpsc::UnboundedSender<String>,
        agent_event_rx: std::sync::mpsc::Receiver<AgentEvent>,
    ) -> Self {
        self.user_msg_tx = Some(user_msg_tx);
        self.agent_event_rx = Some(agent_event_rx);
        self
    }

    // -----------------------------------------------------------------------
    // Agent event handling
    // -----------------------------------------------------------------------

    /// Drain all pending agent events and update TUI state.
    pub fn drain_agent_events(&mut self) {
        if let Some(rx) = self.agent_event_rx.as_mut() {
            let mut events = Vec::new();
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
            for event in events {
                self.handle_agent_event(event);
            }
        }
    }

    /// Process a single agent event.
    fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Started | AgentEvent::TurnStart { .. } | AgentEvent::TurnEnd { .. } => {
                // No visual update needed for these
            }
            AgentEvent::TextDelta(delta) => {
                // Append streaming text to the last assistant message
                if let Some(ChatMessage::Assistant(ref mut text)) = self.messages.last_mut() {
                    text.push_str(&delta);
                }
            }
            AgentEvent::ToolCallStart { name, .. } => {
                self.messages.push(ChatMessage::ToolCall {
                    name,
                    status: ToolStatus::Running,
                });
            }
            AgentEvent::ToolCallEnd { name, result, .. } => {
                // Find the matching running tool call and update its status
                for msg in self.messages.iter_mut().rev() {
                    if let ChatMessage::ToolCall { name: ref n, status } = msg {
                        if n == &name && matches!(status, ToolStatus::Running) {
                            *status = if result.success {
                                ToolStatus::Success(result.content_preview)
                            } else {
                                ToolStatus::Error(result.content_preview)
                            };
                            break;
                        }
                    }
                }
            }
            AgentEvent::Done { response } => {
                self.generating = false;
                // Update result panel with the final response
                self.last_result = Some(response);
            }
            AgentEvent::Error(msg) => {
                self.messages.push(ChatMessage::System(format!("Error: {}", msg)));
                self.generating = false;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Key handling
    // -----------------------------------------------------------------------

    /// Handle a key event.
    pub fn handle_key(&mut self, key: event::KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Tab => {
                self.active_panel = if self.active_panel == 0 { 1 } else { 0 };
            }
            KeyCode::Enter => {
                if !self.input.is_empty() {
                    let text = self.input.clone();
                    self.input.clear();
                    self.history.push(text.clone());
                    self.history_pos = -1;
                    self.messages.push(ChatMessage::User(text.clone()));
                    // Push placeholder for streaming assistant response
                    self.messages.push(ChatMessage::Assistant(String::new()));
                    self.generating = true;
                    // Send to agent
                    if let Some(tx) = &self.user_msg_tx {
                        let _ = tx.send(text);
                    }
                }
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Up => {
                if self.active_panel == 1 && !self.history.is_empty() {
                    // Navigate command history
                    if self.history_pos < 0 {
                        self.history_pos = self.history.len() as isize - 1;
                    } else if self.history_pos > 0 {
                        self.history_pos -= 1;
                    }
                    if let Some(cmd) = self.history.get(self.history_pos as usize) {
                        self.input = cmd.clone();
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if self.active_panel == 1 && self.history_pos >= 0 {
                    self.history_pos += 1;
                    if self.history_pos as usize >= self.history.len() {
                        self.history_pos = -1;
                        self.input.clear();
                    } else if let Some(cmd) = self.history.get(self.history_pos as usize) {
                        self.input = cmd.clone();
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Terminal setup and main loop
// ---------------------------------------------------------------------------

/// RAII guard that restores the terminal on drop (even on panic).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Run the TUI application with agent integration.
pub fn run(provider: &str, model: &str) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;

    let _guard = TerminalGuard; // restores terminal on drop

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create tokio runtime for the agent
    let rt = tokio::runtime::Runtime::new()
        .map_err(io::Error::other)?;

    // Create channels
    let (user_msg_tx, user_msg_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<AgentEvent>();

    // Start the agent background task
    agent_bridge::spawn_agent_task(&rt, provider, model, user_msg_rx, event_tx);

    // Create app with agent channels
    let mut app = App::new().with_agent_channels(user_msg_tx, event_rx);

    let tick_rate = std::time::Duration::from_millis(50); // Faster polling for streaming

    let result = run_loop(&mut terminal, &mut app, tick_rate);

    // Explicit cleanup (guard also handles this on drop)
    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick_rate: std::time::Duration,
) -> io::Result<()> {
    loop {
        // Drain agent events and update state
        app.drain_agent_events();

        terminal.draw(|f| layout::draw(f, app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
