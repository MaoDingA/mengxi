// tui/app.rs — Main TUI application state and event loop

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use super::layout;

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
    Success(String),
    Error(String),
}

/// The main TUI application.
pub struct App {
    /// Chat message history.
    pub messages: Vec<ChatMessage>,
    /// Current input text.
    pub input: String,
    /// Scroll offset for the chat panel.
    pub scroll_offset: u16,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Currently active panel (0 = chat, 1 = input).
    pub active_panel: usize,
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
        }
    }

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
                    self.messages.push(ChatMessage::User(text));
                    // Echo a placeholder assistant response
                    self.messages.push(ChatMessage::Assistant(
                        "[Agent not yet connected]".to_string(),
                    ));
                }
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Up => {
                if self.active_panel == 0 && self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => {
                self.scroll_offset += 1;
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll_offset += 10;
            }
            _ => {}
        }
    }
}

/// Run the TUI application.
pub fn run() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run event loop
    let mut app = App::new();
    let tick_rate = std::time::Duration::from_millis(100);

    let result = run_loop(&mut terminal, &mut app, tick_rate);

    // Restore terminal
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
