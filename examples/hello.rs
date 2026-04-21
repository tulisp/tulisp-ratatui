//! Run: `cargo run --example hello`
//! Press q (or C-c) to quit. Use ↑/↓, page-up/down, home/end, or scroll/click.

use tulisp::TulispContext;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = TulispContext::new();
    tulisp_ratatui::register(&mut ctx);

    let program = include_str!("demo.lisp");
    let res = ctx.eval_string(program);

    // Always try to restore even on error.
    tulisp_ratatui::restore();
    res.map_err(|e| format!("lisp error:\n{}", e.format(&ctx)))?;
    Ok(())
}
