use super::UiTextField;

#[derive(Clone, Copy)]
pub(in crate::ui) enum PointerEdit {
    Place,
    Extend,
    Word,
    Drag,
}
impl UiTextField {
    pub(in crate::ui) fn pointer(&mut self, x: f32, width: f32, edit: PointerEdit) {
        self.cancel_preedit();
        let positions = self.metrics.positions(self.value());
        let mut offset = self.metrics.reveal(&positions, self.cursor, width);
        if matches!(edit, PointerEdit::Drag) {
            let outside = x - x.clamp(0., width);
            let max = (positions[positions.len() - 1] + 1. - width).max(0.);
            offset = (offset + outside).clamp(0., max);
            self.metrics.offset.set(offset);
        }
        let target = x + offset;
        self.cursor = positions
            .iter()
            .enumerate()
            .min_by(|a, b| (*a.1 - target).abs().total_cmp(&(*b.1 - target).abs()))
            .expect("line contains the initial boundary")
            .0;
        match edit {
            PointerEdit::Place => self.anchor = self.cursor,
            PointerEdit::Word => self.select_word(),
            PointerEdit::Extend | PointerEdit::Drag => {}
        }
        self.metrics.reveal(&positions, self.cursor, width);
    }
    fn select_word(&mut self) {
        if self.value().is_empty() {
            return;
        }
        let bytes = self.value().as_bytes();
        let index = self.cursor.min(bytes.len() - 1);
        let whitespace = bytes[index].is_ascii_whitespace();
        let mut start = index;
        let mut end = index + 1;
        while start > 0 && bytes[start - 1].is_ascii_whitespace() == whitespace {
            start -= 1;
        }
        while end < bytes.len() && bytes[end].is_ascii_whitespace() == whitespace {
            end += 1;
        }
        self.anchor = start;
        self.cursor = end;
    }
}
