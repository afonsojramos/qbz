// crates/qbzd/src/tui/widgets.rs — reusable ratatui primitives for the setup TUI.
//
// Two interactive sub-widgets (a filterable select popup and a line input) plus
// the pure render helpers every screen shares (field rows, group headers, the
// bottom help bar, centered modals, spinner). No screen logic lives here —
// screens own their staged state and drive these.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::theme;
use super::strings;

/// Focused-row emphasis: accent reversed (§1.2 — reverse is terminal-theme
/// agnostic, so the selection reads even on monochrome/serial).
pub fn focus_style() -> Style {
    theme::selection()
}

/// Mask a secret for display (token/paste rows never render plaintext).
pub fn mask(s: &str) -> String {
    "•".repeat(s.chars().count())
}

// ============================ field rows ============================

/// A single label/value row rendered as three aligned spans (label · value ·
/// widget hint). When `focused` the whole row becomes one accent bar (the reverse
/// bar wins over per-value tone so it reads uniformly). Otherwise: a disabled row
/// dims and appends `(reason)`; an enabled `[toggle]` value is tinted ok (on) or
/// dim (off); the `widget` hint is always dim. Meaning never rides on color alone
/// — the on/off text, `(reason)` and hint all stay legible without it.
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
    // Fixed gutters so labels/values line up regardless of per-span styling.
    let label_txt = format!("  {label:<20} ");
    let value_txt = format!("{:<32} ", truncate(&shown_value, 32));
    let widget_txt = widget.to_string();

    if focused {
        let mut sel = focus_style();
        if !enabled {
            sel = sel.patch(theme::dim());
        }
        return Line::from(vec![
            Span::styled(label_txt, sel),
            Span::styled(value_txt, sel),
            Span::styled(widget_txt, sel),
        ]);
    }

    let label_style = if enabled { Style::default() } else { theme::dim() };
    let value_style = if !enabled {
        theme::dim()
    } else if widget == "[toggle]" {
        toggle_tone(value)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(label_txt, label_style),
        Span::styled(value_txt, value_style),
        Span::styled(widget_txt, theme::dim()),
    ])
}

/// ok for an on/enabled toggle, dim for off — the text ("on"/"off") carries the
/// meaning; the tint only reinforces it.
fn toggle_tone(value: &str) -> Style {
    if value == "off" {
        theme::dim()
    } else {
        theme::ok()
    }
}

/// A plain, non-interactive info/action line (e.g. Account's status row or an
/// action item). `focused` gives it the accent bar.
pub fn action_line(text: &str, focused: bool, enabled: bool) -> Line<'static> {
    let style = if focused {
        focus_style()
    } else if !enabled {
        theme::dim()
    } else {
        Style::default()
    };
    Line::from(Span::styled(format!("  {text}"), style))
}

pub fn blank() -> Line<'static> {
    Line::from("")
}

/// A dim, wrapped note under a field (previews, hints).
pub fn note_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(format!("    {text}"), theme::dim()))
}

/// A warn-tinted note (LAN exposure, DSD/auth safety, unknown-key preservation).
/// The copy already reads as a warning; the tint is a second channel, not the
/// only one.
pub fn warn_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(format!("    {text}"), theme::warn()))
}

/// An error-tinted note (rejected bind/port). Same rule: the text stands alone.
pub fn err_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(format!("    {text}"), theme::err()))
}

// ============================ section boxes ============================

/// One titled, rounded section box's worth of content. `active` (the group that
/// owns the focused field) borders in accent; the rest border dim.
pub struct Section {
    pub title: String,
    pub active: bool,
    pub lines: Vec<Line<'static>>,
}

impl Section {
    pub fn new(title: impl Into<String>, active: bool, lines: Vec<Line<'static>>) -> Self {
        Self { title: title.into(), active, lines }
    }
}

/// Stack titled, rounded section boxes top-to-bottom in `area`. Each box is sized
/// to its content (+2 for the border); a trailing filler keeps them compact at
/// the top rather than stretching. The active box borders + titles in accent.
pub fn sections(f: &mut Frame, area: Rect, secs: &[Section]) {
    if secs.is_empty() {
        return;
    }
    let mut constraints: Vec<Constraint> = secs
        .iter()
        .map(|s| Constraint::Length(s.lines.len() as u16 + 2))
        .collect();
    constraints.push(Constraint::Min(0));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (i, sec) in secs.iter().enumerate() {
        let (border_style, title_style) = if sec.active {
            (theme::accent(), theme::accent_bold())
        } else {
            (theme::dim(), theme::dim())
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(Line::from(Span::styled(format!(" {} ", sec.title), title_style)));
        let inner = block.inner(chunks[i]);
        f.render_widget(block, chunks[i]);
        f.render_widget(Paragraph::new(sec.lines.clone()), inner);
    }
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

/// True only for tokens that are real key glyphs in the hint vocabulary: a
/// single character (`s`, `r`, `/`, `?`, `q`, `y`, `d`, `p`, …) or a named key.
/// Instructional words ("type") are NOT keys — their segment stays dim.
fn is_key_glyph(token: &str) -> bool {
    token.chars().count() == 1
        || matches!(
            token,
            "Esc" | "Enter" | "Tab" | "Shift-Tab" | "up/down" | "left/right" | "up" | "down"
        )
}

/// Split a `key desc · key desc` hint into accent-key / dim-description spans.
/// Segments are separated by ` · `; a segment's leading token is accent-tinted
/// only when it is a real key glyph — otherwise the whole segment is dim.
pub fn help_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, seg) in text.split(" · ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        match seg.split_once(' ') {
            Some((key, rest)) if is_key_glyph(key) => {
                spans.push(Span::styled(key.to_string(), theme::accent()));
                spans.push(Span::styled(format!(" {rest}"), theme::dim()));
            }
            None if is_key_glyph(seg) => {
                spans.push(Span::styled(seg.to_string(), theme::accent()));
            }
            _ => spans.push(Span::styled(seg.to_string(), theme::dim())),
        }
    }
    spans
}

/// The bottom help bar (one line): accent key glyphs, dim descriptions.
pub fn help_bar(f: &mut Frame, area: Rect, text: &str) {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(help_spans(text));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
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
    let block = titled_block(title);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut text: Vec<Line> = body.lines().map(|l| Line::from(l.to_string())).collect();
    text.push(blank());
    text.push(Line::from(help_spans(hint)));
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
}

/// A rounded, accent-bordered block with an accent-bold title — the shared frame
/// for modals, popups and panels.
fn titled_block(title: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .title(Line::from(Span::styled(format!(" {title} "), theme::accent_bold())))
}

/// A centered scrollable panel (help overlay, import summary, result panel).
pub fn panel(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>, scroll: u16) {
    let rect = centered_rect(area.width.saturating_sub(6).max(40), area.height.saturating_sub(4).max(10), area);
    f.render_widget(Clear, rect);
    let block = titled_block(title);
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

/// Busy overlay: a small centered spinner + label (§5.5). The spinner glyph is
/// accent; the label plain.
pub fn busy_overlay(f: &mut Frame, area: Rect, label: &str, tick: u64) {
    // Layout parity with the pre-theme overlay: body = "<spinner> <label>"
    // (label + 2 chars) + 6 → total rect width = label + 8.
    let width = label.chars().count() as u16 + 8;
    let rect = centered_rect(width, 5, area);
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let line = Line::from(vec![
        Span::styled(format!("{} ", spinner_frame(tick)), theme::accent()),
        Span::raw(label.to_string()),
    ]);
    f.render_widget(
        Paragraph::new(line).alignment(Alignment::Center),
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
                    lines.push(Line::from(Span::styled(h.clone(), theme::accent_bold())));
                }
            }
            let style = if *i == self.idx {
                sel_line = lines.len() as u16;
                focus_style()
            } else {
                Style::default()
            };
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
        let block = titled_block(&self.title);
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
        f.render_widget(Paragraph::new(Line::from(help_spans(&hint))), chunks[1]);
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
