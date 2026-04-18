use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use std::time::Duration;

use crate::editor::input::{key_to_action, mouse_to_action};
use crate::editor::{ActionOutcome, Editor};
use crate::render;
use crate::terminal::Tui;

pub struct App {
    pub editor: Editor,
}

impl App {
    pub fn new(editor: Editor) -> Self {
        Self { editor }
    }

    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        loop {
            terminal.draw(|f| render::draw(&mut self.editor, f))?;
            if event::poll(Duration::from_millis(500))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if let Some(action) = key_to_action(key) {
                            if let ActionOutcome::Quit = self.editor.apply(action)? {
                                return Ok(());
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        let area = self.editor.content_area;
                        if let Some(action) = mouse_to_action(mouse, area) {
                            if let ActionOutcome::Quit = self.editor.apply(action)? {
                                return Ok(());
                            }
                        }
                    }
                    Event::Paste(text) => {
                        self.editor.paste_text(&text);
                    }
                    Event::Resize(_, _) | Event::Key(_) | Event::FocusGained
                    | Event::FocusLost => {}
                }
            }
        }
    }
}
