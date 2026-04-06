//! Input handling — multi-line editor with history.
//! Mirrors Claude Code's components/PromptInput.tsx.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Editor state for the input box.
#[derive(Debug, Default, Clone)]
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
    pub history: Vec<String>,
    pub history_pos: Option<usize>,
}

/// What the REPL loop should do after processing a key event.
#[derive(Debug, PartialEq)]
pub enum InputAction {
    Continue,
    Submit(String),
    Quit,
    Clear,
    Interrupt,
    PasteFromClipboard,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> InputAction {
        match (key.code, key.modifiers) {
            // Submit on Enter
            (KeyCode::Enter, KeyModifiers::NONE) => {
                let text = self.buffer.trim().to_string();
                if text.is_empty() {
                    return InputAction::Continue;
                }
                // Save to history (avoid duplicates at top)
                if self.history.last().map(|s| s.as_str()) != Some(&text) {
                    self.history.push(text.clone());
                }
                self.buffer.clear();
                self.cursor = 0;
                self.history_pos = None;
                InputAction::Submit(text)
            }

            // Newline: Shift+Enter or Alt+Enter
            (KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::ALT) => {
                self.insert('\n');
                InputAction::Continue
            }

            // Quit
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => InputAction::Quit,

            // Interrupt (cancel current query)
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.buffer.clear();
                self.cursor = 0;
                InputAction::Interrupt
            }

            // Clear screen
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => InputAction::Clear,

            // Backspace
            (KeyCode::Backspace, _) => {
                if self.cursor > 0 {
                    let char_boundary = self.prev_char_boundary();
                    self.buffer.drain(char_boundary..self.cursor);
                    self.cursor = char_boundary;
                }
                InputAction::Continue
            }

            // Delete
            (KeyCode::Delete, _) => {
                if self.cursor < self.buffer.len() {
                    let next = self.next_char_boundary();
                    self.buffer.drain(self.cursor..next);
                }
                InputAction::Continue
            }

            // Arrow keys for cursor movement
            (KeyCode::Left, _) => {
                self.cursor = self.prev_char_boundary();
                InputAction::Continue
            }
            (KeyCode::Right, _) => {
                self.cursor = self.next_char_boundary();
                InputAction::Continue
            }
            (KeyCode::Home, _) | (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                self.cursor = 0;
                InputAction::Continue
            }
            (KeyCode::End, _) | (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                self.cursor = self.buffer.len();
                InputAction::Continue
            }

            // History navigation
            (KeyCode::Up, _) => {
                self.history_prev();
                InputAction::Continue
            }
            (KeyCode::Down, _) => {
                self.history_next();
                InputAction::Continue
            }

            // Word delete (Ctrl+W)
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                self.delete_word_back();
                InputAction::Continue
            }

            // Regular character input
            (KeyCode::Char(c), _) => {
                self.insert(c);
                InputAction::Continue
            }

            _ => InputAction::Continue,
        }
    }

    fn insert(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn prev_char_boundary(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }
        let mut pos = self.cursor - 1;
        while pos > 0 && !self.buffer.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    fn next_char_boundary(&self) -> usize {
        let mut pos = self.cursor + 1;
        while pos <= self.buffer.len() && !self.buffer.is_char_boundary(pos) {
            pos += 1;
        }
        pos.min(self.buffer.len())
    }

    fn delete_word_back(&mut self) {
        // Skip spaces, then delete word
        while self.cursor > 0 && self.buffer.as_bytes()[self.cursor - 1] == b' ' {
            let b = self.prev_char_boundary();
            self.buffer.drain(b..self.cursor);
            self.cursor = b;
        }
        while self.cursor > 0 && self.buffer.as_bytes()[self.cursor - 1] != b' ' {
            let b = self.prev_char_boundary();
            self.buffer.drain(b..self.cursor);
            self.cursor = b;
        }
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_pos = match self.history_pos {
            None => self.history.len() - 1,
            Some(p) if p > 0 => p - 1,
            Some(p) => p,
        };
        self.history_pos = Some(new_pos);
        self.buffer = self.history[new_pos].clone();
        self.cursor = self.buffer.len();
    }

    fn history_next(&mut self) {
        match self.history_pos {
            None => {}
            Some(p) if p + 1 < self.history.len() => {
                let new_pos = p + 1;
                self.history_pos = Some(new_pos);
                self.buffer = self.history[new_pos].clone();
                self.cursor = self.buffer.len();
            }
            Some(_) => {
                self.history_pos = None;
                self.buffer.clear();
                self.cursor = 0;
            }
        }
    }
}
