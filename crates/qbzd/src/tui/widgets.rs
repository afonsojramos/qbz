// crates/qbzd/src/tui/widgets.rs — reusable ratatui primitives for the setup TUI.
//
// Two interactive sub-widgets (a filterable select popup and a line input) plus
// the pure render helpers every screen shares (field rows, group headers, the
// bottom help bar, centered modals, spinner). No screen logic lives here —
// screens own their staged state and drive these.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::strings;

/// Focused-row emphasis (reverse video — terminal-theme agnostic, §1.2).
pub fn focus_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}
pub fn dim_style() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

/// Mask a secret for display (token/paste rows never render plaintext).
pub fn mask(s: &str) -> String {
    "•".repeat(s.chars().count())
}

// ============================ field rows ============================

/// A single label/value row. `focused` reverses it; `enabled = false` dims it and
/// appends `(reason)`; `widget` is a trailing `[toggle]`/`[select]`/… hint.
pub fn field_line(
    label: &str,
    value: &str,
    focused: bool,
    enabled: bool,
    reason: Option<&str>,
    widget: &str,
) -> Line<'static> {
    let shown_value = match reason {
        Some(r) if !enabled => format!("{value}  ({r})"),
        _ => value.to_string(),
    };
    // label padded to a fixed gutter so values line up.
    let text = format!(
        "  {:<20} {:<32} {}",
        label,
        truncate(&shown_value, 32),
        widget
    );
    let mut style = Style::default();
    if !enabled {
        style = style.add_modifier(Modifier::DIM);
    }
    if focused {
        style = style.add_modifier(Modifier::REVERSED);
    }
    Line::from(Span::styled(text, style))
}

/// A plain, non-interactive info/action line (e.g. Account's status row or an
/// action item). `focused` reverses it.
pub fn action_line(text: &str, focused: bool, enabled: bool) -> Line<'static> {
    let mut style = Style::default();
    if !enabled {
        style = style.add_modifier(Modifier::DIM);
    }
    if focused {
        style = style.add_modifier(Modifier::REVERSED);
    }
    Line::from(Span::styled(format!("  {text}"), style))
}

/// A bold group header (`OUTPUT`, `QUALITY`, …).
pub fn group_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

pub fn blank() -> Line<'static> {
    Line::from("")
}

/// A dim, wrapped note under a field (previews, warnings).
pub fn note_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("    {text}"),
        Style::default().add_modifier(Modifier::DIM),
    ))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

// ============================ help bar ============================

/// The bottom help bar (one line, dim), drawn into `area`.
pub fn help_bar(f: &mut Frame, area: Rect, text: &str) {
    let p = Paragraph::new(Line::from(Span::styled(
        format!(" {text}"),
        Style::default().add_modifier(Modifier::DIM),
    )));
    f.render_widget(p, area);
}

// ============================ centered modal ============================

/// Center a fixed-size rect inside `area` (clamped to fit).
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

/// A centered, bordered modal with a title, wrapped body and a hint footer.
pub fn modal(f: &mut Frame, area: Rect, title: &str, body: &str, hint: &str) {
    let lines = body.lines().count().max(1) as u16;
    let height = lines + 4; // border(2) + body + spacer + hint
    let width = body
        .lines()
        .chain(std::iter::once(title))
        .chain(std::iter::once(hint))
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(20) as u16
        + 6;
    let rect = centered_rect(width.max(28), height.max(6), area);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut text: Vec<Line> = body.lines().map(|l| Line::from(l.to_string())).collect();
    text.push(blank());
    text.push(Line::from(Span::styled(
        hint.to_string(),
        Style::default().add_modifier(Modifier::DIM),
    )));
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
}

/// A centered scrollable panel (help overlay, import summary, result panel).
pub fn panel(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>, scroll: u16) {
    let rect = centered_rect(area.width.saturating_sub(6).max(40), area.height.saturating_sub(4).max(10), area);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(lines).scroll((scroll, 0)).wrap(Wrap { trim: false }),
        inner,
    );
}

/// Spinner glyph for the given tick (§5.5 worker spinner).
pub fn spinner_frame(tick: u64) -> char {
    const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    FRAMES[(tick as usize) % FRAMES.len()]
}

/// Busy overlay: a small centered spinner + label (§5.5).
pub fn busy_overlay(f: &mut Frame, area: Rect, label: &str, tick: u64) {
    let body = format!("{} {}", spinner_frame(tick), label);
    let rect = centered_rect(body.chars().count() as u16 + 6, 5, area);
    f.render_widget(Clear, rect);
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(body).alignment(Alignment::Center),
        inner,
    );
}

// ============================ select popup ============================

#[derive(Debug, Clone)]
pub struct SelectPopup {
    pub title: String,
    pub options: Vec<String>,
    /// Absolute index into `options` of the current selection.
    pub idx: usize,
    /// When true, printable keys filter the list (device picker, §3.2.2).
    pub filterable: bool,
    pub filter: String,
    /// Parallel to `options`: a bold section header rendered ABOVE that option
    /// when set (device-picker grouping, §3.2.2). Shown only when unfiltered.
    pub headers: Vec<Option<String>>,
}

pub enum SelectOutcome {
    Pending,
    Chosen(usize),
    Cancelled,
}

impl SelectPopup {
    pub fn new(title: &str, options: Vec<String>, selected: usize, filterable: bool) -> Self {
        let last = options.len().saturating_sub(1);
        let headers = vec![None; options.len()];
        Self {
            title: title.to_string(),
            options,
            idx: selected.min(last),
            filterable,
            filter: String::new(),
            headers,
        }
    }

    /// Attach parallel section headers (device picker). No-op if the length
    /// mismatches the option count.
    pub fn with_headers(mut self, headers: Vec<Option<String>>) -> Self {
        if headers.len() == self.options.len() {
            self.headers = headers;
        }
        self
    }

    /// Indices of options currently visible under the filter.
    pub fn visible(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.options.len()).collect();
        }
        let needle = self.filter.to_ascii_lowercase();
        (0..self.options.len())
            .filter(|i| self.options[*i].to_ascii_lowercase().contains(&needle))
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SelectOutcome {
        match key.code {
            KeyCode::Up => {
                self.step(-1);
                SelectOutcome::Pending
            }
            KeyCode::Down => {
                self.step(1);
                SelectOutcome::Pending
            }
            // j/k move ONLY on non-filterable popups; on a filterable one they
            // are filter characters.
            KeyCode::Char('k') if !self.filterable => {
                self.step(-1);
                SelectOutcome::Pending
            }
            KeyCode::Char('j') if !self.filterable => {
                self.step(1);
                SelectOutcome::Pending
            }
            KeyCode::Enter => {
                if self.visible().is_empty() {
                    SelectOutcome::Cancelled
                } else {
                    SelectOutcome::Chosen(self.idx)
                }
            }
            KeyCode::Esc => {
                if self.filterable && !self.filter.is_empty() {
                    self.filter.clear();
                    self.reselect_first();
                    SelectOutcome::Pending
                } else {
                    SelectOutcome::Cancelled
                }
            }
            KeyCode::Char('/') if self.filterable => {
                self.filter.clear();
                self.reselect_first();
                SelectOutcome::Pending
            }
            KeyCode::Backspace if self.filterable => {
                self.filter.pop();
                self.reselect_first();
                SelectOutcome::Pending
            }
            KeyCode::Char(c) if self.filterable => {
                self.filter.push(c);
                self.reselect_first();
                SelectOutcome::Pending
            }
            _ => SelectOutcome::Pending,
        }
    }

    /// Move the selection by `delta` within the visible (filtered) set, wrapping.
    fn step(&mut self, delta: isize) {
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        let cur = vis.iter().position(|i| *i == self.idx).unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(vis.len() as isize) as usize;
        self.idx = vis[next];
    }

    fn reselect_first(&mut self) {
        if let Some(first) = self.visible().first().copied() {
            self.idx = first;
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let vis = self.visible();
        let show_headers = self.filter.is_empty();
        let mut lines: Vec<Line> = Vec::new();
        let mut sel_line: u16 = 0;
        for i in &vis {
            if show_headers {
                if let Some(Some(h)) = self.headers.get(*i) {
                    lines.push(Line::from(Span::styled(
                        h.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                }
            }
            let mut style = Style::default();
            if *i == self.idx {
                style = style.add_modifier(Modifier::REVERSED);
                sel_line = lines.len() as u16;
            }
            lines.push(Line::from(Span::styled(
                format!("  {}", self.options[*i]),
                style,
            )));
        }
        if vis.is_empty() {
            lines.push(note_line("(no matches)"));
        }
        let hint = if self.filterable {
            format!(
                "{}   [{}]",
                strings::HELP_FILTER,
                if self.filter.is_empty() { "type to filter".into() } else { self.filter.clone() }
            )
        } else {
            strings::HELP_SELECT.to_string()
        };

        let height = (lines.len() as u16 + 4)
            .min(area.height.saturating_sub(2))
            .max(6);
        let width = self
            .options
            .iter()
            .map(|o| o.chars().count())
            .chain(std::iter::once(hint.chars().count()))
            .chain(std::iter::once(self.title.chars().count()))
            .max()
            .unwrap_or(20) as u16
            + 8;
        let rect = centered_rect(width.min(area.width.saturating_sub(2)).max(24), height, area);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", self.title));
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let list_h = inner.height.saturating_sub(1);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(list_h), Constraint::Length(1)])
            .split(inner);
        // Scroll so the selected line (accounting for header rows) stays visible.
        let scroll = sel_line.saturating_sub(list_h.saturating_sub(1));
        f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), chunks[0]);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint,
                Style::default().add_modifier(Modifier::DIM),
            ))),
            chunks[1],
        );
    }
}

// ============================ line input ============================

#[derive(Debug, Clone, Default)]
pub struct TextInput {
    pub buf: String,
    pub masked: bool,
}

pub enum InputOutcome {
    Pending,
    Accepted,
    Cancelled,
}

impl TextInput {
    pub fn new(initial: &str, masked: bool) -> Self {
        Self { buf: initial.to_string(), masked }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> InputOutcome {
        match key.code {
            KeyCode::Enter => InputOutcome::Accepted,
            KeyCode::Esc => InputOutcome::Cancelled,
            KeyCode::Backspace => {
                self.buf.pop();
                InputOutcome::Pending
            }
            KeyCode::Char(c) => {
                self.buf.push(c);
                InputOutcome::Pending
            }
            _ => InputOutcome::Pending,
        }
    }

    pub fn display(&self) -> String {
        if self.masked {
            mask(&self.buf)
        } else {
            self.buf.clone()
        }
    }
}
