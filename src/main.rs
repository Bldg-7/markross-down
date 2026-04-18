use anyhow::{Context, Result};
use std::path::PathBuf;

mod app;
mod clipboard;
mod document;
mod editor;
mod parser;
mod render;
mod terminal;
mod watcher;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    let document = match path {
        Some(p) => document::Document::open(p).context("failed to open file")?,
        None => document::Document::empty(),
    };

    let editor = editor::Editor::new(document);
    let watcher = editor
        .document
        .path
        .as_ref()
        .and_then(|p| watcher::FileWatcher::new(p).ok());
    let mut app = app::App::new(editor, watcher);

    let mut tui = terminal::enter()?;
    let run_result = app.run(&mut tui);
    let leave_result = terminal::leave(&mut tui);
    run_result.and(leave_result)
}
