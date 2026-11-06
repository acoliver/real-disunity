//! Bounded single-line editing. Indices count characters; the active text path supports ASCII.
use super::{FontBook, TextRun, UiError, UiKey};
use crate::TextInput;
use std::ops::Range;
mod drawing;
mod metrics;
pub(super) mod pointer;
pub use drawing::TextFieldStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextReject {
    Clipboard(super::ClipboardError),
    MaxLength,
    Font(UiError),
    InvalidPreedit,
}
#[derive(Clone, Debug, PartialEq)]
pub struct UiTextField {
    run: TextRun,
    metrics: metrics::FieldMetrics,
    max: usize,
    cursor: usize,
    anchor: usize,
    preedit: String,
    preedit_cursor: Option<(usize, usize)>,
}
impl UiTextField {
    pub fn new(mut run: TextRun, max: usize, fonts: &FontBook) -> Result<Self, TextReject> {
        if max > fonts.limits().max_glyphs || run.text.chars().count() > max {
            return Err(TextReject::MaxLength);
        }
        run.rich_text = false;
        run.horizontal = super::HorizontalAlign::Left;
        run.vertical = super::VerticalAlign::Middle;
        run.horizontal_overflow = super::HorizontalOverflow::Overflow;
        run.vertical_overflow = super::VerticalOverflow::Overflow;
        fonts.validate_single_line(&run).map_err(TextReject::Font)?;
        let cursor = run.text.len();
        Ok(Self {
            metrics: metrics::FieldMetrics::new(fonts, &run).map_err(TextReject::Font)?,
            run,
            max,
            cursor,
            anchor: cursor,
            preedit: String::new(),
            preedit_cursor: None,
        })
    }
    pub fn value(&self) -> &str {
        &self.run.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    pub fn preedit(&self) -> &str {
        &self.preedit
    }
    pub fn text_run(&self) -> &TextRun {
        &self.run
    }
    pub fn cancel_preedit(&mut self) {
        self.preedit.clear();
        self.preedit_cursor = None;
    }
    pub(super) fn input(
        &mut self,
        fonts: &FontBook,
        event: &TextInput,
    ) -> Result<bool, TextReject> {
        match event {
            TextInput::Commit(text) => {
                self.cancel_preedit();
                let run = self.replacement(text, fonts)?;
                let changed = self.run.text != run.text;
                self.cursor = self.selection().start + text.len();
                self.anchor = self.cursor;
                self.run = run;
                Ok(changed)
            }
            TextInput::Preedit { text, cursor } => {
                self.cancel_preedit();
                if cursor
                    .is_some_and(|(a, b)| !text.is_char_boundary(a) || !text.is_char_boundary(b))
                {
                    return Err(TextReject::InvalidPreedit);
                }
                self.replacement(text, fonts)?;
                self.preedit.clone_from(text);
                self.preedit_cursor = *cursor;
                Ok(false)
            }
            TextInput::Cancel | TextInput::Disabled => {
                self.cancel_preedit();
                Ok(false)
            }
            TextInput::Enabled => Ok(false),
        }
    }
    fn replacement(&self, text: &str, fonts: &FontBook) -> Result<TextRun, TextReject> {
        let range = self.selection();
        let count = self.value().chars().count() - range.len() + text.chars().count();
        if count > self.max {
            return Err(TextReject::MaxLength);
        }
        let mut run = self.run.clone();
        run.text.replace_range(range, text);
        fonts.validate_single_line(&run).map_err(TextReject::Font)?;
        Ok(run)
    }
    pub(super) fn edit(&mut self, key: UiKey, shift: bool, word: bool) -> bool {
        self.cancel_preedit();
        let before = self.run.text.clone();
        match key {
            UiKey::SelectAll => {
                self.anchor = 0;
                self.cursor = self.value().len();
            }
            UiKey::Backspace | UiKey::Delete => self.delete(key, word),
            _ => self.move_cursor(key, shift, word),
        }
        self.run.text != before
    }
    fn delete(&mut self, key: UiKey, word: bool) {
        if self.selection().is_empty() {
            self.cursor = self.destination(
                if key == UiKey::Backspace {
                    UiKey::Left
                } else {
                    UiKey::Right
                },
                word,
            );
        }
        let range = self.selection();
        self.cursor = range.start;
        self.anchor = self.cursor;
        self.run.text.replace_range(range, "");
    }
    fn move_cursor(&mut self, key: UiKey, shift: bool, word: bool) {
        let range = self.selection();
        self.cursor = match key {
            UiKey::Left if !shift && !range.is_empty() => range.start,
            UiKey::Right if !shift && !range.is_empty() => range.end,
            _ => self.destination(key, word),
        };
        if !shift {
            self.anchor = self.cursor;
        }
    }
    fn destination(&self, key: UiKey, word: bool) -> usize {
        match key {
            UiKey::Home => 0,
            UiKey::End => self.value().len(),
            UiKey::Left if word => self.word_left(),
            UiKey::Right if word => self.word_right(),
            UiKey::Left => self.cursor.saturating_sub(1),
            UiKey::Right => (self.cursor + 1).min(self.value().len()),
            _ => self.cursor,
        }
    }
    fn word_left(&self) -> usize {
        let bytes = self.value().as_bytes();
        let mut i = self.cursor;
        while i > 0 && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        while i > 0 && !bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        i
    }
    fn word_right(&self) -> usize {
        let bytes = self.value().as_bytes();
        let mut i = self.cursor;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    }
    fn displayed(&self) -> (TextRun, usize) {
        let mut run = self.run.clone();
        if self.preedit.is_empty() {
            return (run, self.cursor);
        }
        run.text.replace_range(self.selection(), &self.preedit);
        let cursor =
            self.selection().start + self.preedit_cursor.map_or(self.preedit.len(), |(a, _)| a);
        (run, cursor)
    }
}
