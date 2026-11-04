use super::{RetainedEventHost, RetainedHost, RetainedInput};
use worldrunner_world_simulation::ui::{
    Clipboard, ClipboardCommand, ClipboardError, MemoryClipboard, ShortcutPlatform,
};

pub(super) enum HostClipboard {
    Memory(MemoryClipboard),
    Native(Result<arboard::Clipboard, ClipboardError>),
}
impl Default for HostClipboard {
    fn default() -> Self {
        Self::Memory(MemoryClipboard::default())
    }
}
impl Clipboard for HostClipboard {
    fn read_text(&mut self) -> Result<String, ClipboardError> {
        match self {
            Self::Memory(memory) => memory.read_text(),
            Self::Native(clipboard) => clipboard
                .as_mut()
                .map_err(|e| *e)?
                .get_text()
                .map_err(|_| ClipboardError::ReadFailed),
        }
    }
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        match self {
            Self::Memory(memory) => memory.write_text(text),
            Self::Native(clipboard) => clipboard
                .as_mut()
                .map_err(|e| *e)?
                .set_text(text)
                .map_err(|_| ClipboardError::WriteFailed),
        }
    }
}
impl RetainedHost {
    /// Windowless hosts own an in-memory clipboard. Native startup replaces it once;
    /// an unavailable OS clipboard returns a typed error and never falls back to memory.
    pub fn clipboard(&mut self) -> &mut dyn Clipboard {
        &mut self.clipboard
    }
    pub(in crate::retained) fn native_clipboard(&mut self) {
        self.clipboard = HostClipboard::Native(
            arboard::Clipboard::new().map_err(|_| ClipboardError::Unavailable),
        );
    }
    pub fn clipboard_command(&self, input: &RetainedInput) -> Option<ClipboardCommand> {
        let RetainedInput::Key {
            key,
            pressed: true,
            repeat: false,
            ..
        } = input
        else {
            return None;
        };
        ShortcutPlatform::current().clipboard(
            key,
            self.key_held("control"),
            self.key_held("super"),
            self.key_held("alt"),
        )
    }
    pub(in crate::retained) fn command_modifier_held(&self) -> bool {
        self.key_held("control") || self.key_held("super")
    }
}
impl RetainedEventHost<'_> {
    pub fn clipboard(&mut self) -> &mut dyn Clipboard {
        self.host.clipboard()
    }
    pub fn clipboard_command(&self, input: &RetainedInput) -> Option<ClipboardCommand> {
        self.host.clipboard_command(input)
    }
}
