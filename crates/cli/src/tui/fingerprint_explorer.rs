// tui/fingerprint_explorer.rs — Interactive fingerprint strip explorer
//
// Loads a fingerprint strip PNG and displays:
// - Top: colored strip representation (scrollable)
// - Middle-left: scene boundaries
// - Middle-right: color mood timeline
// - Bottom: color DNA stats

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::path::Path;

use mengxi_core::color_dna;
use mengxi_core::color_mood::{self, MoodCategory};
use mengxi_core::movie_fingerprint;
use mengxi_core::scene_boundary;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Fingerprint explorer application state.
pub struct ExplorerApp {
    /// Strip pixel data as interleaved f64 RGB [0.0, 1.0].
    strip_data: Vec<f64>,
    /// Strip width (number of frames).
    strip_width: usize,
    /// Strip height.
    strip_height: usize,
    /// Color DNA analysis result.
    color_dna: Option<color_dna::ColorDna>,
    /// Detected scene boundaries.
    scene_boundaries: Vec<scene_boundary::SceneBoundary>,
    /// Color mood timeline segments.
    mood_segments: Vec<color_mood::MoodSegment>,
    /// Horizontal scroll offset for the strip view.
    scroll_x: usize,
    /// Vertical scroll offset for info panels.
    scroll_y: usize,
    /// Currently active panel (0=strip, 1=scenes, 2=mood, 3=dna).
    active_panel: usize,
    /// Whether to quit.
    should_quit: bool,
    /// Status message.
    status: String,
}

impl ExplorerApp {
    /// Create a new explorer from a strip PNG path.
    pub fn new(strip_path: &Path) -> Result<Self, String> {
        let (w, h, data) = movie_fingerprint::read_strip_png(strip_path)
            .map_err(|e| format!("failed to read strip: {}", e))?;

        let dna = color_dna::extract_color_dna(&data, w, h)
            .map_err(|e| format!("DNA extraction failed: {}", e))
            .ok();

        let boundaries = scene_boundary::detect_scene_boundaries(&data, w, h, 0.3, 5, 50)
            .unwrap_or_default();

        let boundary_frames: Vec<usize> = boundaries.iter().map(|b| b.frame_idx).collect();
        let mood_segments = color_mood::compute_mood_timeline(&data, w, h, &boundary_frames)
            .unwrap_or_default();

        Ok(Self {
            strip_data: data,
            strip_width: w,
            strip_height: h,
            color_dna: dna,
            scene_boundaries: boundaries,
            mood_segments,
            scroll_x: 0,
            scroll_y: 0,
            active_panel: 0,
            should_quit: false,
            status: format!("Loaded {}x{} strip ({} frames)", w, h, w),
        })
    }

    fn scroll_left(&mut self, amount: usize) {
        self.scroll_x = self.scroll_x.saturating_sub(amount);
    }

    fn scroll_right(&mut self, amount: usize) {
        let max_x = self.strip_width.saturating_sub(1);
        if self.scroll_x < max_x {
            self.scroll_x = (self.scroll_x + amount).min(max_x);
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_y = self.scroll_y.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll_y = self.scroll_y.saturating_add(1);
    }

    fn next_panel(&mut self) {
        self.active_panel = (self.active_panel + 1) % 4;
        self.status = format!("Panel: {}", self.panel_name());
    }

    fn prev_panel(&mut self) {
        self.active_panel = (self.active_panel + 3) % 4;
        self.status = format!("Panel: {}", self.panel_name());
    }

    fn panel_name(&self) -> &'static str {
        match self.active_panel {
            0 => "Fingerprint Strip",
            1 => "Scene Boundaries",
            2 => "Color Mood",
            3 => "Color DNA",
            _ => "Unknown",
        }
    }

    /// Handle a key event. Returns true if the app should continue.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return true;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                false
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.scroll_left(1);
                true
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.scroll_right(1);
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down();
                true
            }
            KeyCode::PageUp => {
                self.scroll_left(20);
                true
            }
            KeyCode::PageDown => {
                self.scroll_right(20);
                true
            }
            KeyCode::Home => {
                self.scroll_x = 0;
                true
            }
            KeyCode::End => {
                self.scroll_x = self.strip_width.saturating_sub(1);
                true
            }
            KeyCode::Tab => {
                self.next_panel();
                true
            }
            KeyCode::BackTab => {
                self.prev_panel();
                true
            }
            _ => true,
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Draw the full explorer layout.
pub fn draw(f: &mut Frame, app: &ExplorerApp) {
    let size = f.area();

    // Layout: strip | info panels | status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),      // Fingerprint strip
            Constraint::Min(10),        // Info panels
            Constraint::Length(1),      // Status bar
        ])
        .split(size);

    draw_strip_panel(f, app, chunks[0]);

    // Info area: split into scenes (left) and mood+DNA (right)
    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),  // Scene boundaries
            Constraint::Percentage(60),  // Mood + DNA
        ])
        .split(chunks[1]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),  // Color mood
            Constraint::Percentage(50),  // Color DNA
        ])
        .split(info_chunks[1]);

    draw_scene_panel(f, app, info_chunks[0]);
    draw_mood_panel(f, app, right_chunks[0]);
    draw_dna_panel(f, app, right_chunks[1]);
    draw_status_bar(f, app, chunks[2]);
}

fn draw_strip_panel(f: &mut Frame, app: &ExplorerApp, area: Rect) {
    let border_style = if app.active_panel == 0 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let visible_width = (area.width as usize).saturating_sub(2); // minus borders
    let strip_height = (area.height as usize).saturating_sub(2);
    let mut lines: Vec<Line> = Vec::with_capacity(strip_height);

    for _row in 0..strip_height {
        let mut spans: Vec<Span> = Vec::with_capacity(visible_width);
        for col in 0..visible_width {
            let frame_idx = app.scroll_x + col;
            if frame_idx < app.strip_width {
                // Sample the middle row of the strip for this frame column
                let sample_y = (app.strip_height / 2).min(app.strip_height - 1);
                let idx = (sample_y * app.strip_width + frame_idx) * 3;
                if idx + 2 < app.strip_data.len() {
                    let r = (app.strip_data[idx] * 255.0).round() as u8;
                    let g = (app.strip_data[idx + 1] * 255.0).round() as u8;
                    let b = (app.strip_data[idx + 2] * 255.0).round() as u8;
                    spans.push(Span::styled(
                        "█",
                        Style::default().fg(Color::Rgb(r, g, b)),
                    ));
                } else {
                    spans.push(Span::raw(" "));
                }
            } else {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    let title = format!(
        " Fingerprint Strip ({}x{}, frame {}/{}) ",
        app.strip_width, app.strip_height,
        app.scroll_x, app.strip_width.saturating_sub(1),
    );

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title).style(border_style));

    f.render_widget(paragraph, area);
}

fn draw_scene_panel(f: &mut Frame, app: &ExplorerApp, area: Rect) {
    let border_style = if app.active_panel == 1 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("{} scene boundaries detected", app.scene_boundaries.len()),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (i, b) in app.scene_boundaries.iter().enumerate() {
        let is_visible = b.frame_idx >= app.scroll_x
            && b.frame_idx < app.scroll_x + area.width as usize;
        let marker = if is_visible { "►" } else { " " };
        let style = if is_visible {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", marker), style),
            Span::styled(
                format!("Boundary {}: frame {} (conf {:.3})", i + 1, b.frame_idx, b.confidence),
                style,
            ),
        ]));
    }

    if app.scene_boundaries.is_empty() {
        lines.push(Line::from(Span::styled(
            "No scene boundaries detected.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Scenes ")
                .style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_y as u16, 0));

    f.render_widget(paragraph, area);
}

fn draw_mood_panel(f: &mut Frame, app: &ExplorerApp, area: Rect) {
    let border_style = if app.active_panel == 2 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let mut lines: Vec<Line> = Vec::new();

    // Draw mood bar (colored by mood)
    let mut bar_spans: Vec<Span> = Vec::new();
    for seg in &app.mood_segments {
        let color = match seg.mood {
            MoodCategory::Dark => Color::DarkGray,
            MoodCategory::Vivid => Color::Magenta,
            MoodCategory::Warm => Color::Red,
            MoodCategory::Cool => Color::Blue,
            MoodCategory::Neutral => Color::White,
        };
        let len = seg.end_frame.saturating_sub(seg.start_frame).max(1);
        let chars = (((len as f64 / app.strip_width as f64) * (area.width as f64 - 2.0))
            .round() as usize).max(1);
        bar_spans.push(Span::styled(
            "█".repeat(chars),
            Style::default().fg(color),
        ));
    }
    lines.push(Line::from(bar_spans));
    lines.push(Line::from(""));

    // Legend
    lines.push(Line::from(vec![
        Span::styled("■ Dark  ", Style::default().fg(Color::DarkGray)),
        Span::styled("■ Vivid ", Style::default().fg(Color::Magenta)),
        Span::styled("■ Warm  ", Style::default().fg(Color::Red)),
        Span::styled("■ Cool  ", Style::default().fg(Color::Blue)),
        Span::styled("■ Neutral", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(""));

    // Segment details
    for seg in &app.mood_segments {
        lines.push(Line::from(format!(
            "  Frames {}-{}: {} ({})",
            seg.start_frame,
            seg.end_frame,
            seg.mood.description_zh(),
            match seg.mood {
                MoodCategory::Dark => "Dark",
                MoodCategory::Vivid => "Vivid",
                MoodCategory::Warm => "Warm",
                MoodCategory::Cool => "Cool",
                MoodCategory::Neutral => "Neutral",
            },
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Color Mood ")
                .style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_y as u16, 0));

    f.render_widget(paragraph, area);
}

fn draw_dna_panel(f: &mut Frame, app: &ExplorerApp, area: Rect) {
    let border_style = if app.active_panel == 3 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let mut lines: Vec<Line> = Vec::new();

    if let Some(dna) = &app.color_dna {
        lines.push(Line::from(Span::styled(
            "Color DNA Profile",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "  Average: L={:.3}  a={:.3}  b={:.3}",
            dna.avg_l, dna.avg_a, dna.avg_b,
        )));
        lines.push(Line::from(format!(
            "  Contrast={:.3}  Warmth={:.3}  Saturation={:.3}",
            dna.contrast, dna.warmth, dna.saturation,
        )));
        lines.push(Line::from(""));

        // Hue distribution bar
        lines.push(Line::from(Span::styled(
            "Hue Distribution (12 bins):",
            Style::default().add_modifier(Modifier::DIM),
        )));

        let max_val = dna.hue_distribution.iter().cloned().fold(0.0_f64, f64::max);
        if max_val > 0.0 {
            let bar_width = (area.width as usize).saturating_sub(4);
            let mut bar_spans: Vec<Span> = Vec::new();
            for (i, &val) in dna.hue_distribution.iter().enumerate() {
                let chars = ((val / max_val) * bar_width as f64 / 12.0).round() as usize;
                // Color by hue angle
                let hue = (i as f64 / 12.0) * 360.0;
                let color = hue_to_ratatui_color(hue);
                bar_spans.push(Span::styled(
                    "█".repeat(chars.max(1)),
                    Style::default().fg(color),
                ));
            }
            lines.push(Line::from(bar_spans));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Color DNA not available",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Color DNA ")
                .style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_y as u16, 0));

    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, app: &ExplorerApp, area: Rect) {
    let help = format!(
        " {} | ←→ scroll | Tab panel | Home/End jump | q quit | {}",
        app.panel_name(),
        app.status,
    );
    let paragraph = Paragraph::new(Line::from(Span::styled(
        help,
        Style::default().fg(Color::Black).bg(Color::DarkGray),
    )));
    f.render_widget(paragraph, area);
}

/// Convert a hue angle (0-360) to a ratatui Color.
fn hue_to_ratatui_color(hue: f64) -> Color {
    let (r, g, b) = hsv_to_rgb(hue, 0.8, 0.9);
    Color::Rgb(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// Simple HSV to RGB conversion.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let c = v * s;
    let h = (h % 360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r1 + m, g1 + m, b1 + m)
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the fingerprint explorer TUI.
pub fn run(strip_path: &Path) -> Result<(), String> {
    let app = ExplorerApp::new(strip_path)?;

    enable_raw_mode().map_err(|e| format!("failed to enable raw mode: {}", e))?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)
        .map_err(|e| format!("failed to enter alternate screen: {}", e))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| format!("failed to create terminal: {}", e))?;

    let mut app = app;
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode().ok();
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut ExplorerApp,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|f| draw(f, app))
            .map_err(|e| format!("draw error: {}", e))?;

        if app.should_quit {
            return Ok(());
        }

        if event::poll(std::time::Duration::from_millis(100))
            .map_err(|e| format!("poll error: {}", e))?
        {
            if let Event::Key(key) = event::read().map_err(|e| format!("event error: {}", e))? {
                app.handle_key(key);
            }
        }
    }
}
