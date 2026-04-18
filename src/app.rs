use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;

use crate::editor::input::{key_to_action, mouse_to_action};
use crate::editor::{ActionOutcome, Editor, MergeAction};
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
                        if self.editor.merge.is_some() {
                            if let Some(action) = merge_key_to_action(key) {
                                self.editor.merge_action(action);
                            }
                        } else if let Some(action) = key_to_action(key) {
                            if let ActionOutcome::Quit = self.editor.apply(action)? {
                                return Ok(());
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        if self.editor.merge.is_some() {
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
                        if self.editor.merge.is_none() {
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
}

fn merge_key_to_action(key: KeyEvent) -> Option<MergeAction> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(MergeAction::Next),
        KeyCode::Char('k') | KeyCode::Up => Some(MergeAction::Prev),
        KeyCode::Char('m') if !shift => Some(MergeAction::PickMine),
        KeyCode::Char('t') if !shift => Some(MergeAction::PickTheirs),
        KeyCode::Char('M') | KeyCode::Char('m') if shift => Some(MergeAction::AllMine),
        KeyCode::Char('T') | KeyCode::Char('t') if shift => Some(MergeAction::AllTheirs),
        KeyCode::Enter => Some(MergeAction::Apply),
        KeyCode::Esc => Some(MergeAction::Abort),
        _ => None,
    }
}
