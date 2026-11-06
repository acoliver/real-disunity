//! Host-supplied text clipboard. The simulation never opens an OS connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable,
    ReadFailed,
    WriteFailed,
}

pub trait Clipboard {
    fn read_text(&mut self) -> Result<String, ClipboardError>;
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryClipboard(String);
impl Clipboard for MemoryClipboard {
    fn read_text(&mut self) -> Result<String, ClipboardError> {
        Ok(self.0.clone())
    }
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.0 = text.into();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardCommand {
    Copy,
    Cut,
    Paste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutPlatform {
    MacOs,
    Other,
}
impl ShortcutPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
    /// `command` is the macOS Command key; Alt-modified chords are not shortcuts.
    pub fn clipboard(
        self,
        key: &str,
        control: bool,
        command: bool,
        alt: bool,
    ) -> Option<ClipboardCommand> {
        let primary = match self {
            Self::MacOs => command && !control,
            Self::Other => control && !command,
        };
        if !primary || alt {
            return None;
        }
        match key {
            "c" => Some(ClipboardCommand::Copy),
            "x" => Some(ClipboardCommand::Cut),
            "v" => Some(ClipboardCommand::Paste),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_use_command_on_macos_and_control_elsewhere() {
        for (platform, control, command) in [
            (ShortcutPlatform::MacOs, false, true),
            (ShortcutPlatform::Other, true, false),
        ] {
            for (key, expected) in [
                ("c", ClipboardCommand::Copy),
                ("x", ClipboardCommand::Cut),
                ("v", ClipboardCommand::Paste),
            ] {
                assert_eq!(
                    platform.clipboard(key, control, command, false),
                    Some(expected)
                );
                assert_eq!(platform.clipboard(key, command, control, false), None);
                assert_eq!(platform.clipboard(key, control, command, true), None);
                assert_eq!(platform.clipboard(key, false, false, false), None);
            }
        }
    }
}
