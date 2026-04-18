use ropey::Rope;

use crate::document::line_len_no_newline;

#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    /// Column the user "wants" — preserved across vertical movement.
    pub desired_col: usize,
}

impl Cursor {
    pub fn clamp(&mut self, rope: &Rope) {
        let max_line = rope.len_lines().saturating_sub(1);
        self.line = self.line.min(max_line);
        self.col = self.col.min(line_len_no_newline(rope, self.line));
    }

    pub fn char_offset(&self, rope: &Rope) -> usize {
        rope.line_to_char(self.line) + self.col
    }

    pub fn move_left(&mut self, rope: &Rope) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = line_len_no_newline(rope, self.line);
        }
        self.desired_col = self.col;
    }

    pub fn move_right(&mut self, rope: &Rope) {
        let line_len = line_len_no_newline(rope, self.line);
        if self.col < line_len {
            self.col += 1;
        } else if self.line + 1 < rope.len_lines() {
            self.line += 1;
            self.col = 0;
        }
        self.desired_col = self.col;
    }

    pub fn move_up(&mut self, rope: &Rope) {
        if self.line > 0 {
            self.line -= 1;
            let line_len = line_len_no_newline(rope, self.line);
            self.col = self.desired_col.min(line_len);
        }
    }

    pub fn move_down(&mut self, rope: &Rope) {
        let max_line = rope.len_lines().saturating_sub(1);
        if self.line < max_line {
            self.line += 1;
            let line_len = line_len_no_newline(rope, self.line);
            self.col = self.desired_col.min(line_len);
        }
    }

    pub fn move_home(&mut self) {
        self.col = 0;
        self.desired_col = 0;
    }

    pub fn move_end(&mut self, rope: &Rope) {
        self.col = line_len_no_newline(rope, self.line);
        self.desired_col = self.col;
    }
}
