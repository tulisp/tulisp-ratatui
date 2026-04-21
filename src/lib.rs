//! Minimum tulisp bindings for ratatui.
//!
//! Exposes just enough to build a live read/write TUI from lisp:
//!   (tui/init)                         -> terminal handle
//!   (tui/restore)                      -> leaves raw mode, shows cursor
//!   (tui/size term)                    -> (cols . rows)
//!   (tui/draw term widgets)            -> widgets is a list of widget handles
//!   (tui/poll-event timeout-ms)        -> nil, a key symbol, or (mouse-EV X Y)
//!
//! Widget constructors (each returns an opaque widget handle). The trailing
//! STYLE argument is an optional alist; see `parse_styles` for recognized keys.
//!   (tui/paragraph x y w h title text &optional style)
//!   (tui/list x y w h title items &optional selected style)
//!   (tui/gauge x y w h title ratio &optional label style)

use std::fmt;
use std::io::stdout;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::execute;
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span as TextSpan, Text};
use ratatui::widgets::{Block, Borders, Gauge, List as ListWidget, ListItem, ListState, Paragraph};
use tulisp::{Error, Shared, SharedMut, TulispContext, TulispConvertible, TulispObject, list};

// ---------------------------------------------------------------------------
// Terminal wrapper
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Terminal {
    // `tulisp::SharedMut` transparently becomes `Arc<RwLock<..>>` when
    // tulisp's `sync` feature is enabled (e.g. via tulisp-async), and
    // `Rc<RefCell<..>>` otherwise. Same `.borrow()`/`.borrow_mut()`
    // surface either way.
    inner: SharedMut<Option<DefaultTerminal>>,
}

impl fmt::Display for Terminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#<tui/terminal>")
    }
}

impl TulispConvertible for Terminal {
    fn from_tulisp(value: &TulispObject) -> Result<Self, Error> {
        value
            .as_any()?
            .downcast_ref::<Terminal>()
            .cloned()
            .ok_or_else(|| Error::type_mismatch(format!("Expected tui/terminal, got {value}")))
    }

    fn into_tulisp(self) -> TulispObject {
        Shared::new(self).into()
    }
}

// ---------------------------------------------------------------------------
// Widget wrapper
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct Styles {
    text: Style,
    title: Style,
    border: Style,
    highlight: Style,
}

#[derive(Clone, Debug)]
enum WidgetKind {
    Paragraph {
        title: Option<String>,
        /// Each outer entry is a line; each inner entry is a styled span.
        lines: Vec<Vec<(String, Style)>>,
    },
    List {
        title: Option<String>,
        /// Each item is a sequence of (text, style) spans rendered on one line.
        items: Vec<Vec<(String, Style)>>,
        selected: Option<usize>,
    },
    Gauge {
        title: Option<String>,
        ratio: f64,
        label: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Widget {
    area: Rect,
    kind: WidgetKind,
    styles: Styles,
}

impl fmt::Display for Widget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#<tui/widget>")
    }
}

impl TulispConvertible for Widget {
    fn from_tulisp(value: &TulispObject) -> Result<Self, Error> {
        value
            .as_any()?
            .downcast_ref::<Widget>()
            .cloned()
            .ok_or_else(|| Error::type_mismatch(format!("Expected tui/widget, got {value}")))
    }

    fn into_tulisp(self) -> TulispObject {
        Shared::new(self).into()
    }
}

impl Widget {
    fn block(&self, title: Option<&String>) -> Option<Block<'_>> {
        title.map(|t| {
            Block::default()
                .title(ratatui::text::Span::styled(t.clone(), self.styles.title))
                .borders(Borders::ALL)
                .border_style(self.styles.border)
        })
    }

    fn render(&self, frame: &mut Frame) {
        match &self.kind {
            WidgetKind::Paragraph { title, lines } => {
                let text: Text = lines
                    .iter()
                    .map(|spans| {
                        Line::from(
                            spans
                                .iter()
                                .map(|(t, s)| TextSpan::styled(t.clone(), *s))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .into();
                let mut p = Paragraph::new(text).style(self.styles.text);
                if let Some(b) = self.block(title.as_ref()) {
                    p = p.block(b);
                }
                frame.render_widget(p, self.area);
            }
            WidgetKind::List {
                title,
                items,
                selected,
            } => {
                let items: Vec<ListItem> = items
                    .iter()
                    .map(|spans| {
                        let line = Line::from(
                            spans
                                .iter()
                                .map(|(text, style)| TextSpan::styled(text.clone(), *style))
                                .collect::<Vec<_>>(),
                        );
                        ListItem::new(line)
                    })
                    .collect();
                let highlight = if self.styles.highlight == Style::default() {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    self.styles.highlight
                };
                let mut list = ListWidget::new(items)
                    .style(self.styles.text)
                    .highlight_symbol("> ")
                    .highlight_style(highlight);
                if let Some(b) = self.block(title.as_ref()) {
                    list = list.block(b);
                }
                let mut state = ListState::default();
                state.select(*selected);
                frame.render_stateful_widget(list, self.area, &mut state);
            }
            WidgetKind::Gauge {
                title,
                ratio,
                label,
            } => {
                let mut g = Gauge::default()
                    .gauge_style(self.styles.text)
                    .ratio(ratio.clamp(0.0, 1.0));
                if let Some(l) = label {
                    g = g.label(l.clone());
                }
                if let Some(b) = self.block(title.as_ref()) {
                    g = g.block(b);
                }
                frame.render_widget(g, self.area);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rect_from(x: i64, y: i64, w: i64, h: i64) -> Rect {
    Rect::new(
        x.max(0) as u16,
        y.max(0) as u16,
        w.max(0) as u16,
        h.max(0) as u16,
    )
}

fn opt_string(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn widgets_from_list(list: &TulispObject) -> Result<Vec<Widget>, Error> {
    let mut out = Vec::new();
    for item in list.base_iter() {
        out.push(Widget::from_tulisp(&item)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Style parsing
// ---------------------------------------------------------------------------

fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

fn parse_color(obj: &TulispObject) -> Result<Color, Error> {
    let name = obj.as_symbol().or_else(|_| obj.as_string())?;
    if let Some(c) = parse_hex_color(&name) {
        return Ok(c);
    }
    let c = match name.as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark-gray" | "dark-grey" => Color::DarkGray,
        "light-red" => Color::LightRed,
        "light-green" => Color::LightGreen,
        "light-yellow" => Color::LightYellow,
        "light-blue" => Color::LightBlue,
        "light-magenta" => Color::LightMagenta,
        "light-cyan" => Color::LightCyan,
        "white" => Color::White,
        "reset" => Color::Reset,
        other => {
            return Err(Error::lisp_error(format!("unknown color: {}", other)));
        }
    };
    Ok(c)
}

fn parse_modifier(obj: &TulispObject) -> Result<Modifier, Error> {
    let mut m = Modifier::empty();
    for item in obj.base_iter() {
        let name = item.as_symbol().or_else(|_| item.as_string())?;
        let bit = match name.as_str() {
            "bold" => Modifier::BOLD,
            "dim" => Modifier::DIM,
            "italic" => Modifier::ITALIC,
            "underline" | "underlined" => Modifier::UNDERLINED,
            "slow-blink" => Modifier::SLOW_BLINK,
            "rapid-blink" => Modifier::RAPID_BLINK,
            "reversed" | "reverse" => Modifier::REVERSED,
            "hidden" => Modifier::HIDDEN,
            "crossed-out" | "strikethrough" => Modifier::CROSSED_OUT,
            other => {
                return Err(Error::lisp_error(format!("unknown modifier: {}", other)));
            }
        };
        m |= bit;
    }
    // Bare symbol case: base_iter on a non-list yields nothing, so fall back.
    if m.is_empty() && !obj.null() {
        let name = obj.as_symbol().or_else(|_| obj.as_string())?;
        m = match name.as_str() {
            "bold" => Modifier::BOLD,
            "dim" => Modifier::DIM,
            "italic" => Modifier::ITALIC,
            "underline" | "underlined" => Modifier::UNDERLINED,
            "reversed" | "reverse" => Modifier::REVERSED,
            "hidden" => Modifier::HIDDEN,
            "crossed-out" | "strikethrough" => Modifier::CROSSED_OUT,
            other => {
                return Err(Error::lisp_error(format!("unknown modifier: {}", other)));
            }
        };
    }
    Ok(m)
}

fn apply_style_field(style: &mut Style, key: &str, value: &TulispObject) -> Result<(), Error> {
    match key {
        "fg" => *style = style.fg(parse_color(value)?),
        "bg" => *style = style.bg(parse_color(value)?),
        "modifier" | "modifiers" => *style = style.add_modifier(parse_modifier(value)?),
        _ => {}
    }
    Ok(())
}

/// Parse a flat style alist (no prefix keys like `title-` / `border-`).
fn parse_flat_style(obj: &TulispObject) -> Result<Style, Error> {
    let mut s = Style::default();
    if obj.null() {
        return Ok(s);
    }
    for pair in obj.base_iter() {
        let key = pair.car()?.as_symbol()?;
        let val = pair.cdr()?;
        apply_style_field(&mut s, &key, &val)?;
    }
    Ok(s)
}

/// Parse paragraph text. Accepts:
///   - a plain string (split on `\n`, each line rendered unstyled)
///   - a list of lines, where each line is either a string or a list of spans
///     (each span a string or `(TEXT . STYLE-ALIST)`, same shape as list items)
fn parse_paragraph_text(obj: &TulispObject) -> Result<Vec<Vec<(String, Style)>>, Error> {
    if let Ok(s) = obj.as_string() {
        if s.is_empty() {
            return Ok(vec![Vec::new()]);
        }
        return Ok(s
            .split('\n')
            .map(|l| vec![(l.to_string(), Style::default())])
            .collect());
    }
    let mut lines = Vec::new();
    for line in obj.base_iter() {
        lines.push(parse_list_item(&line)?);
    }
    Ok(lines)
}

/// Parse a single list item into a sequence of (text, style) spans. Accepts:
///   - a plain string (one unstyled span)
///   - a list of spans, where each span is either a string or
///     `(TEXT . STYLE-ALIST)` cons
fn parse_list_item(obj: &TulispObject) -> Result<Vec<(String, Style)>, Error> {
    if let Ok(s) = obj.as_string() {
        return Ok(vec![(s, Style::default())]);
    }
    let mut spans = Vec::new();
    for span in obj.base_iter() {
        if let Ok(s) = span.as_string() {
            spans.push((s, Style::default()));
        } else {
            let text = span.car()?.as_string()?;
            let style = parse_flat_style(&span.cdr()?)?;
            spans.push((text, style));
        }
    }
    Ok(spans)
}

/// Parse an alist of style fields. Recognized keys:
///   fg, bg, modifier                — apply to widget text/body
///   title-fg, title-bg, title-modifier
///   border-fg, border-bg, border-modifier
///   highlight-fg, highlight-bg, highlight-modifier
fn parse_styles(obj: Option<TulispObject>) -> Result<Styles, Error> {
    let mut s = Styles::default();
    let Some(alist) = obj else {
        return Ok(s);
    };
    if alist.null() {
        return Ok(s);
    }
    for pair in alist.base_iter() {
        let key = pair.car()?.as_symbol()?;
        let val = pair.cdr()?;
        if let Some(rest) = key.strip_prefix("title-") {
            apply_style_field(&mut s.title, rest, &val)?;
        } else if let Some(rest) = key.strip_prefix("border-") {
            apply_style_field(&mut s.border, rest, &val)?;
        } else if let Some(rest) = key.strip_prefix("highlight-") {
            apply_style_field(&mut s.highlight, rest, &val)?;
        } else {
            apply_style_field(&mut s.text, &key, &val)?;
        }
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// Event naming
// ---------------------------------------------------------------------------

fn key_symbol_name(code: KeyCode, mods: KeyModifiers) -> String {
    let base = match code {
        KeyCode::Char(c) => format!("char-{}", c),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "escape".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "back-tab".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "page-up".into(),
        KeyCode::PageDown => "page-down".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::F(n) => format!("f{}", n),
        _ => "unknown".into(),
    };
    let mut name = String::new();
    if mods.contains(KeyModifiers::CONTROL) {
        name.push_str("C-");
    }
    if mods.contains(KeyModifiers::ALT) {
        name.push_str("M-");
    }
    if mods.contains(KeyModifiers::SHIFT)
        && !matches!(code, KeyCode::Char(c) if c.is_ascii_uppercase())
    {
        name.push_str("S-");
    }
    name.push_str(&base);
    name
}

fn mouse_symbol_name(kind: MouseEventKind) -> Option<&'static str> {
    Some(match kind {
        MouseEventKind::Down(MouseButton::Left) => "mouse-left",
        MouseEventKind::Down(MouseButton::Right) => "mouse-right",
        MouseEventKind::Down(MouseButton::Middle) => "mouse-middle",
        MouseEventKind::ScrollUp => "mouse-scroll-up",
        MouseEventKind::ScrollDown => "mouse-scroll-down",
        MouseEventKind::ScrollLeft => "mouse-scroll-left",
        MouseEventKind::ScrollRight => "mouse-scroll-right",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Leaves raw mode and shows the cursor. Safe to call without an active terminal.
pub fn restore() {
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
}

/// Registers all `tui/*` functions on `ctx`.
pub fn register(ctx: &mut TulispContext) {
    ctx.defun("tui/init", || -> Result<TulispObject, Error> {
        let term = ratatui::try_init().map_err(|e| Error::lisp_error(format!("tui/init: {e}")))?;
        execute!(stdout(), EnableMouseCapture)
            .map_err(|e| Error::lisp_error(format!("tui/init: {e}")))?;
        Ok(Terminal {
            inner: SharedMut::new(Some(term)),
        }
        .into_tulisp())
    });

    ctx.defun("tui/restore", || -> Result<TulispObject, Error> {
        restore();
        Ok(TulispObject::nil())
    });

    ctx.defun(
        "tui/size",
        |term: Terminal| -> Result<TulispObject, Error> {
            let borrow = term.inner.borrow();
            let t = borrow
                .as_ref()
                .ok_or_else(|| Error::lisp_error("tui/size: terminal was closed".to_string()))?;
            let size = t
                .size()
                .map_err(|e| Error::lisp_error(format!("tui/size: {e}")))?;
            Ok(TulispObject::cons(
                (size.width as i64).into(),
                (size.height as i64).into(),
            ))
        },
    );

    ctx.defun(
        "tui/draw",
        |term: Terminal, widgets: TulispObject| -> Result<TulispObject, Error> {
            let widgets = widgets_from_list(&widgets)?;
            let mut borrow = term.inner.borrow_mut();
            let t = borrow
                .as_mut()
                .ok_or_else(|| Error::lisp_error("tui/draw: terminal was closed".to_string()))?;
            t.draw(|frame| {
                for w in &widgets {
                    w.render(frame);
                }
            })
            .map_err(|e| Error::lisp_error(format!("tui/draw: {e}")))?;
            Ok(TulispObject::nil())
        },
    );

    ctx.defun(
        "tui/poll-event",
        |ctx: &mut TulispContext, timeout_ms: i64| -> Result<TulispObject, Error> {
            let dur = Duration::from_millis(timeout_ms.max(0) as u64);
            let has =
                event::poll(dur).map_err(|e| Error::lisp_error(format!("tui/poll-event: {e}")))?;
            if !has {
                return Ok(TulispObject::nil());
            }
            let ev = event::read()
                .map_err(|e| Error::lisp_error(format!("tui/poll-event: {e}")))?;
            match ev {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    Ok(ctx.intern(&key_symbol_name(k.code, k.modifiers)))
                }
                Event::Mouse(m) => {
                    let Some(name) = mouse_symbol_name(m.kind) else {
                        return Ok(TulispObject::nil());
                    };
                    let sym = ctx.intern(name);
                    list!(sym, (m.column as i64).into(), (m.row as i64).into())
                }
                _ => Ok(TulispObject::nil()),
            }
        },
    );

    ctx.defun(
        "tui/paragraph",
        |x: i64,
         y: i64,
         w: i64,
         h: i64,
         title: String,
         text: TulispObject,
         style: Option<TulispObject>|
         -> Result<TulispObject, Error> {
            Ok(Widget {
                area: rect_from(x, y, w, h),
                kind: WidgetKind::Paragraph {
                    title: opt_string(title),
                    lines: parse_paragraph_text(&text)?,
                },
                styles: parse_styles(style)?,
            }
            .into_tulisp())
        },
    );

    ctx.defun(
        "tui/list",
        |x: i64,
         y: i64,
         w: i64,
         h: i64,
         title: String,
         items: TulispObject,
         selected: Option<i64>,
         style: Option<TulispObject>|
         -> Result<TulispObject, Error> {
            let mut parsed = Vec::new();
            for item in items.base_iter() {
                parsed.push(parse_list_item(&item)?);
            }
            Ok(Widget {
                area: rect_from(x, y, w, h),
                kind: WidgetKind::List {
                    title: opt_string(title),
                    items: parsed,
                    selected: selected.and_then(|s| if s < 0 { None } else { Some(s as usize) }),
                },
                styles: parse_styles(style)?,
            }
            .into_tulisp())
        },
    );

    ctx.defun(
        "tui/gauge",
        |x: i64,
         y: i64,
         w: i64,
         h: i64,
         title: String,
         ratio: f64,
         label: Option<String>,
         style: Option<TulispObject>|
         -> Result<TulispObject, Error> {
            Ok(Widget {
                area: rect_from(x, y, w, h),
                kind: WidgetKind::Gauge {
                    title: opt_string(title),
                    ratio,
                    label: label.and_then(opt_string),
                },
                styles: parse_styles(style)?,
            }
            .into_tulisp())
        },
    );
}
