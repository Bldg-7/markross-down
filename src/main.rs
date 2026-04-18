use anyhow::{Context, Result};
use std::path::PathBuf;

mod app;
mod clipboard;
mod config;
mod document;
mod editor;
mod keybind;
mod merge;
mod parser;
mod plugin;
mod render;
mod terminal;
mod theme;
mod watcher;

#[cfg(test)]
mod visual_tests;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    let document = match path {
        Some(p) => document::Document::open(p).context("failed to open file")?,
        None => document::Document::empty(),
    };

    let loaded = config::load();
    let plugins = config::resolve_plugins(&loaded.config.plugins);
    let host = plugin::PluginHost::new(plugins);
    let (keybinds, keybind_errors) =
        keybind::KeybindTable::from_config(&loaded.config.keybinds);

    let mut editor = editor::Editor::build(
        document,
        host,
        loaded.config.theme.clone(),
        keybinds,
    );

    let startup_status = match &loaded.source {
        config::ConfigSource::File(p) if keybind_errors.is_empty() => {
            Some(format!("config: {}", p.display()))
        }
        config::ConfigSource::File(p) => Some(format!(
            "config: {} — keybind errors: {}",
            p.display(),
            keybind_errors.join("; ")
        )),
        config::ConfigSource::Defaults => None,
        config::ConfigSource::Error { path, message } => Some(format!(
            "config error at {}: {message} — using defaults",
            path.display()
        )),
    };
    editor.status = startup_status;

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
