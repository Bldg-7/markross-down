use anyhow::{Context, Result};
use std::path::PathBuf;

mod app;
mod clipboard;
mod document;
mod editor;
mod render;
mod terminal;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    let document = match path {
        Some(p) => document::Document::open(p).context("failed to open file")?,
        None => document::Document::empty(),
    };

    let editor = editor::Editor::new(document);
    let mut app = app::App::new(editor);

    let mut tui = terminal::enter()?;
    let run_result = app.run(&mut tui);
    let leave_result = terminal::leave(&mut tui);
    run_result.and(leave_result)
}
