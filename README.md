# tulisp-ratatui

Minimal [tulisp](https://github.com/shsms/tulisp) bindings for
[ratatui](https://ratatui.rs), so lisp code can drive a live TUI.

Terminal setup, keyboard and mouse polling, and a small set of widget
constructors are exposed as `tui/*` functions. You own the main loop; this
crate just draws frames and hands you events.

## Usage

```rust
use tulisp::TulispContext;

let mut ctx = TulispContext::new();
tulisp_ratatui::register(&mut ctx);

// Run your lisp program. Call `tulisp_ratatui::restore()` on error paths
// so the terminal doesn't stay in raw mode if your program panics.
let res = ctx.eval_string(include_str!("ui.lisp"));
if res.is_err() {
    tulisp_ratatui::restore();
}
```

Run `cargo run --example hello` for a full demo that exercises multi-span
list rows, styled paragraph lines, hex-colour styles, mouse scroll/click,
and keyboard navigation. The lisp program lives in
[`examples/demo.lisp`](examples/demo.lisp); the Rust entry point
[`examples/hello.rs`](examples/hello.rs) just registers the bindings and
loads it.

## Lisp API

### Terminal

```
(tui/init)                 ;; enter raw mode + mouse capture, return handle
(tui/restore)              ;; leave raw mode, show cursor
(tui/size term)            ;; -> (cols . rows)
(tui/draw term widgets)    ;; widgets is a list of widget handles
(tui/poll-event timeout-ms) ;; -> nil | key-symbol | (mouse-kind x y)
```

Key events arrive as interned symbols: `char-q`, `C-char-c`, `up`, `down`,
`page-up`, `home`, `f5`, etc. Modifier prefixes are `C-`, `M-`, `S-`
(shift is omitted for uppercase letters, since the shifted char already
encodes it).

Mouse events arrive as `(kind x y)` lists, with `kind` one of:
`mouse-left`, `mouse-right`, `mouse-middle`, `mouse-scroll-up`,
`mouse-scroll-down`, `mouse-scroll-left`, `mouse-scroll-right`.

### Widgets

Each constructor returns an opaque widget handle. The trailing `style` arg
is an optional alist (see **Styles** below).

```
(tui/paragraph x y w h title text &optional style)
(tui/list      x y w h title items &optional selected style)
(tui/gauge     x y w h title ratio &optional label style)
```

**`text`** (paragraph) is either:
- a plain string (split on `\n` into unstyled lines), or
- a list of lines, where each line is either a string or a list of spans
  (spans use the same shape as list items — see below).

**`items`** (list) is a list where each item is either:
- a plain string (one unstyled span), or
- a list of spans. Each span is either a string or a `(TEXT . STYLE-ALIST)`
  cons, so you can colour different columns of the same row independently.

**`selected`** is a 0-based index or `nil`.

### Styles

Style alists are plain `(key . value)` pairs. Recognised keys, grouped by
which part of the widget they target:

| Prefix       | Keys                                           |
|--------------|------------------------------------------------|
| (body)       | `fg`, `bg`, `modifier`                         |
| `title-`     | `title-fg`, `title-bg`, `title-modifier`       |
| `border-`    | `border-fg`, `border-bg`, `border-modifier`    |
| `highlight-` | `highlight-fg`, `highlight-bg`, `highlight-modifier` |

**Colours** are either a named symbol (`red`, `green`, `dark-gray`,
`light-cyan`, `reset`, …) or a 6-digit hex string like `"#ff6d7e"` for
full RGB.

**Modifiers** are a symbol or a list of symbols: `bold`, `dim`, `italic`,
`underline`, `reversed`, `hidden`, `crossed-out`, `slow-blink`,
`rapid-blink`.

`highlight-*` only applies to the list widget's selected row. If you leave
them unset, the row falls back to `REVERSED`.

Example:

```lisp
(tui/list 0 0 40 10 "items"
          '((("first"  . ((fg . "#baa0f8")))
             (" — "    . ((fg . "#8b9798")))
             ("styled" . ((fg . "#a2e57b") (modifier . bold))))
            "plain string row")
          0
          '((border-fg . "#7cd5f1")
            (title-fg  . "#7cd5f1")
            (title-modifier . bold)
            (highlight-bg  . "#3a4449")))
```

## License

GPL-3.0, matching tulisp.
