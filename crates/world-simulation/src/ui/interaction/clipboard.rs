use super::{UiAction, UiResponse, UiRouter};
use crate::ui::{Clipboard, ClipboardCommand, FontBook, TextReject, UiLayout};
use crate::{TextInput, World, WorldError};

impl UiRouter {
    /// Copy/cut publish before mutating the field. Paste uses ordinary atomic validation,
    /// including rejection of CR/LF under the single-line policy.
    pub fn clipboard(
        &mut self,
        world: &World,
        layout: &UiLayout,
        fonts: &FontBook,
        command: ClipboardCommand,
        clipboard: &mut dyn Clipboard,
    ) -> Result<UiResponse, WorldError> {
        // Revalidate focus and modal state through the same route as committed input.
        let mut response = self.text(world, layout, fonts, &TextInput::Enabled)?;
        let Some(control) = self.text_target(world, layout)? else {
            return Ok(response);
        };
        let field = self
            .text_fields
            .get_mut(&control)
            .expect("registered field");
        let result = match command {
            ClipboardCommand::Paste => clipboard
                .read_text()
                .map_err(TextReject::Clipboard)
                .and_then(|text| field.input(fonts, &TextInput::Commit(text))),
            ClipboardCommand::Copy | ClipboardCommand::Cut => {
                if field.selection().is_empty() {
                    return Ok(response);
                }
                clipboard
                    .write_text(&field.value()[field.selection()])
                    .map_err(TextReject::Clipboard)
                    .map(|()| {
                        command == ClipboardCommand::Cut
                            && field.edit(super::UiKey::Delete, false, false)
                    })
            }
        };
        match result {
            Ok(true) => response.actions.push(UiAction::TextChanged { control }),
            Ok(false) => {}
            Err(reason) => response
                .actions
                .push(UiAction::TextRejected { control, reason }),
        }
        Ok(response)
    }
}
