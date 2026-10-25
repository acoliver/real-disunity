use glam::{Quat, Vec3};
use worldrunner_world_simulation::ObjectEdit;

use crate::{
    DisunityCommand, command_kind,
    store::{Session, Store},
};

pub(crate) fn submit_commands(
    store: &mut Store,
    world: u64,
    commands: &[DisunityCommand],
) -> Result<(), crate::errors::AbiError> {
    store
        .session_mut(world)?
        .pending_commands
        .extend_from_slice(commands);
    Ok(())
}

fn decode(
    session: &Session,
    command: DisunityCommand,
) -> Result<ObjectEdit, crate::errors::AbiError> {
    let object = session.object(command.object)?;
    match command.kind {
        command_kind::SET_POSITION => Ok(ObjectEdit::Translation(
            object,
            session
                .native
                .local_position_from_world(object, Vec3::new(command.x, command.y, command.z))
                .map_err(crate::errors::AbiError::from)?,
        )),
        command_kind::SET_ROTATION_Y => {
            if !command.x.is_finite() {
                return Err("rotation must be finite".into());
            }
            Ok(ObjectEdit::Rotation(
                object,
                Quat::from_rotation_y(command.x),
            ))
        }
        command_kind::SET_MATERIAL_SLOT => {
            if !command.x.is_finite()
                || command.x < 0.0
                || command.x.fract() != 0.0
                || f64::from(command.x) > f64::from(u32::MAX)
            {
                return Err("material slot must be a non-negative integer in u32 range".into());
            }
            Ok(ObjectEdit::MaterialSlot(object, command.x as u32))
        }
        _ => Err("unknown command kind".into()),
    }
}

pub(crate) fn flush(session: &mut Session) -> Result<(), crate::errors::AbiError> {
    let edits: Vec<_> = session
        .pending_commands
        .iter()
        .map(|&command| decode(session, command))
        .collect::<Result<_, _>>()?;
    session
        .native
        .apply_edits(&edits)
        .map_err(crate::errors::AbiError::from)?;
    session.pending_commands.clear();
    Ok(())
}

pub(crate) fn apply_immediate(
    store: &mut Store,
    world: u64,
    command: DisunityCommand,
) -> Result<(), crate::errors::AbiError> {
    let session = store.session_mut(world)?;
    let edit = decode(session, command)?;
    session
        .native
        .apply_edits(&[edit])
        .map_err(crate::errors::AbiError::from)
}
