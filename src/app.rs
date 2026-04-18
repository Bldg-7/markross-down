use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

use crate::editor::input::{key_to_action, mouse_to_action};
use crate::editor::{ActionOutcome, Editor};
use crate::render;
use crate::terminal::Tui;
use crate::watcher::FileWatcher;

pub struct App {
    pub editor: Editor,
    pub watcher: Option<FileWatcher>,
}

impl App {
    pub fn new(editor: Editor, watcher: Option<FileWatcher>) -> Self {
        Self { editor, watcher }
    }

    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        loop {
            terminal.draw(|f| render::draw(&mut self.editor, f))?;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if self.editor.pending_reload.is_some() {
                            self.handle_reload_key(key.code);
                        } else if let Some(action) = key_to_action(key) {
                            if let ActionOutcome::Quit = self.editor.apply(action)? {
                                return Ok(());
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        if self.editor.pending_reload.is_some() {
                            continue;
                        }
                        let area = self.editor.content_area;
                        if let Some(action) = mouse_to_action(mouse, area) {
                            if let ActionOutcome::Quit = self.editor.apply(action)? {
                                return Ok(());
                            }
                        }
                    }
                    Event::Paste(text) => {
                        if self.editor.pending_reload.is_none() {
                            self.editor.paste_text(&text);
                        }
                    }
                    Event::Resize(_, _) | Event::Key(_) | Event::FocusGained
                    | Event::FocusLost => {}
                }
            }
            if let Some(w) = &self.watcher {
                while let Ok(e) = w.rx.try_recv() {
                    self.editor.handle_watch_event(e);
                }
            }
        }
    }

    fn handle_reload_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('r') | KeyCode::Char('R') => self.editor.accept_reload(),
            KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Esc => {
                self.editor.reject_reload()
            }
            _ => {
                self.editor.status =
                    Some("press R to reload or I to keep mine".into());
            }
        }
    }
}
