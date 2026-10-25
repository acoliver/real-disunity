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
) -> Result<(), String> {
    store
        .session_mut(world)?
        .pending_commands
        .extend_from_slice(commands);
    Ok(())
}

fn decode(session: &Session, command: DisunityCommand) -> Result<ObjectEdit, String> {
    let object = session.object(command.object)?;
    match command.kind {
        command_kind::SET_POSITION => Ok(ObjectEdit::Translation(
            object,
            Vec3::new(command.x, command.y, command.z),
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

pub(crate) fn step_world(
    store: &mut Store,
    world: u64,
    dt: f32,
    _frame: u64,
) -> Result<(), String> {
    let session = store.session_mut(world)?;
    if !dt.is_finite() || dt < 0.0 {
        return Err("delta must be finite and non-negative".into());
    }
    let pending = std::mem::take(&mut session.pending_commands);
    let edits: Vec<_> = pending
        .iter()
        .map(|&command| decode(session, command))
        .collect::<Result<_, _>>()?;
    session
        .native
        .apply_edits(&edits)
        .map_err(|error| error.to_string())?;
    session.applied_commands.extend(pending);
    Ok(())
}
