//! Engine-owned storage behind the Disunity C ABI.
//!
//! The ABI never lets a raw pointer persist across a call: handles are
//! generation-checked indices into this store, and command buffers are copied
//! in before any state changes. A world unload drops its slot, which is what
//! makes the load/unload soak test meaningful: the table must drain.

pub mod abi;

mod body;
mod clock;
mod components;
mod destruction;
mod encoding;
mod geometry;
mod input;
mod integrity;
mod ledger;
mod physics;
mod protocol;
mod ratio;
mod readiness;
pub mod response;
mod snapshots;
mod strict;
mod tiles;
mod transform;
mod transport;
mod ui;
mod world;

/// Executes a typed protocol operation against the canonical native world.
pub fn request_value(
    world: u64,
    request: serde_json::Value,
) -> Result<serde_json::Value, &'static str> {
    let bytes = serde_json::to_vec(&request).map_err(|_| "invalid request")?;
    if bytes.len() > response::MAX_REQUEST_BYTES {
        return Err("request budget exceeded");
    }
    let response = integrity::execute(world, request, &bytes).map_err(|code| match code {
        result_code::BUDGET => "request retention budget exceeded",
        _ => "world handle does not address a live world",
    })?;
    serde_json::from_slice(&response).map_err(|_| "invalid response")
}

pub use abi::{DISUNITY_ABI_VERSION, DisunityResult, result_code};

use std::sync::Mutex;

/// Maximum commands accepted by one `disunity_command_buffer_submit`. A larger
/// request is a `BUDGET` result, not an allocation.
pub const MAX_COMMANDS_PER_SUBMIT: u32 = 1 << 20;

/// Command kinds understood by [`DisunityCommand::kind`].
pub mod command_kind {
    pub const SET_POSITION: u32 = 0;
    pub const SET_ROTATION_Y: u32 = 1;
    pub const SET_MATERIAL_SLOT: u32 = 2;
}

/// A blittable frame intent: `object` is the target handle, `kind` selects the
/// operation, and `x`/`y`/`z` carry the operands. `SET_POSITION` uses all
/// three; `SET_ROTATION_Y` and `SET_MATERIAL_SLOT` use `x` (slots stay exact
/// because they are small integers).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityCommand {
    pub object: u64,
    pub kind: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// One engine-side object: native-authoritative state. C# `GameObject`
/// instances are views over the handle; destroying the object invalidates the
/// view instead of keeping ghost state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityObject {
    pub position: [f32; 3],
    pub rotation_y_radians: f32,
    pub material_slot: u32,
    pub scale: [f32; 3],
    pub rotation_x_radians: f32,
    pub rotation_z_radians: f32,
}

impl Default for DisunityObject {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation_y_radians: 0.0,
            material_slot: 0,
            scale: [1.0; 3],
            rotation_x_radians: 0.0,
            rotation_z_radians: 0.0,
        }
    }
}

/// A blittable snapshot of one live object, for bulk extraction by a host
/// renderer. Batched like the command buffer: one call, no per-object FFI.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityObjectState {
    pub object: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y_radians: f32,
    pub material_slot: u32,
}

/// Writes live object states into `out` (capacity `capacity`), setting `written`
/// to the number copied. `written` exceeds capacity only to report truncation:
/// callers re-invoke with a larger buffer.
pub fn export_object_states(
    handle: u64,
    out: &mut [DisunityObjectState],
    written: &mut u32,
) -> Result<(), &'static str> {
    *written = 0;
    with_world(handle, |world| {
        for (slot, entry) in world.objects.iter().enumerate() {
            let Some(object) = entry else { continue };
            if let Some(destination) = out.get_mut(*written as usize) {
                let (position, rotation, _) = world
                    .world_trs(make_handle(slot, world.object_generations[slot]))
                    .map_err(|_| "invalid hierarchy")?;
                *destination = DisunityObjectState {
                    object: make_handle(slot, world.object_generations[slot]),
                    x: position.x,
                    y: position.y,
                    z: position.z,
                    rotation_y_radians: rotation.to_euler(glam::EulerRot::XYZ).1,
                    material_slot: object.material_slot,
                };
            }
            *written = written.checked_add(1).ok_or("object count overflow")?;
        }
        Ok(())
    })?
}

/// A world: its object table, the command log, and the last error.
#[derive(Clone, Default)]
pub struct DisunityWorld {
    pub(crate) strict: Option<strict::Authority>,
    pub(crate) snapshots: snapshots::Receipts,
    pub(crate) content_revision: u64,
    pub(crate) readiness: readiness::NativeReadinessState,
    pub(crate) input: input::Input,
    pub(crate) destruction: destruction::PendingDestruction,
    pub(crate) clock: clock::Clock,
    pub(crate) random: input::Random,
    pub(crate) physics: physics::Physics,
    pub(crate) ui: ui::Ui,
    pub(crate) metadata: std::collections::BTreeMap<u64, world::ObjectMetadata>,
    pub(crate) components: Vec<Option<components::Component>>,
    /// Objects in slot order; `None` slots are free for reuse.
    pub objects: Vec<Option<DisunityObject>>,
    /// Generation for each object slot; a handle is stale when its generation
    /// does not match.
    pub object_generations: Vec<u32>,
    /// Commands applied to the world, in order. Replay of this log from a
    /// fresh world is the determinism contract.
    pub applied_commands: Vec<DisunityCommand>,
    /// Commands submitted but not yet applied by a step.
    pub pending_commands: Vec<DisunityCommand>,
    /// Human-readable last error, fetched by `disunity_error_message`.
    pub last_error: Option<String>,
}

impl DisunityWorld {
    pub(crate) fn advance_content(&mut self) -> Result<(), &'static str> {
        self.content_revision = self
            .content_revision
            .checked_add(1)
            .ok_or("content revision overflow")?;
        Ok(())
    }

    /// Applies a submitted command to object state, recording it in the log.
    fn apply_command(&mut self, command: DisunityCommand) -> Result<(), &'static str> {
        if ![command.x, command.y, command.z]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err("command operands must be finite");
        }
        let (index, generation) = split_handle(command.object)?;
        if self.object_generations.get(index).copied() != Some(generation)
            || self.objects.get(index).and_then(Option::as_ref).is_none()
        {
            return Err("object handle generation is stale");
        }
        if command.kind == command_kind::SET_POSITION {
            return self
                .set_trs(
                    command.object,
                    Some([command.x, command.y, command.z]),
                    None,
                    None,
                    "world",
                )
                .map_err(|_| "invalid world transform");
        }
        let object = self.objects[index].as_mut().unwrap();
        match command.kind {
            command_kind::SET_ROTATION_Y => {
                object.rotation_y_radians = command.x;
            }
            command_kind::SET_MATERIAL_SLOT => {
                if command.x < 0.0 || command.x.fract() != 0.0 {
                    return Err("material slot must be a non-negative integer");
                }
                object.material_slot = command.x as u32;
            }
            _ => return Err("unknown command kind"),
        }
        Ok(())
    }
}

/// Splits a handle into (index, generation).
fn split_handle(handle: u64) -> Result<(usize, u32), &'static str> {
    let index = (handle & 0xFFFF_FFFF) as usize;
    let generation = (handle >> 32) as u32;
    Ok((index, generation))
}

/// Builds a handle from (index, generation).
fn make_handle(index: usize, generation: u32) -> u64 {
    (u64::from(generation) << 32) | u64::try_from(index).unwrap_or(0xFFFF_FFFF)
}

struct Store {
    worlds: Vec<Option<integrity::Session>>,
    world_generations: Vec<u32>,
}

impl Store {
    const fn new() -> Self {
        Self {
            worlds: Vec::new(),
            world_generations: Vec::new(),
        }
    }
}

static STORE: Mutex<Store> = Mutex::new(Store::new());

/// Locks the store; poisoning means a prior caller panicked mid-mutation, and
/// every subsequent call failing closed is the correct behavior there.
pub(crate) fn lock_store() -> std::sync::MutexGuard<'static, Store> {
    STORE.lock().unwrap()
}

/// Allocates a world and returns its handle.
pub(crate) fn create_world(store: &mut Store) -> u64 {
    let slot = store.worlds.iter().position(Option::is_none);
    match slot {
        Some(index) => {
            store.world_generations[index] = next_generation();
            store.worlds[index] = Some(integrity::Session::default());
            make_handle(index, store.world_generations[index])
        }
        None => {
            let index = store.worlds.len();
            store.worlds.push(Some(integrity::Session::default()));
            let generation = next_generation();
            store.world_generations.push(generation);
            make_handle(index, generation)
        }
    }
}

/// Removes a world; its objects go with it because they live in the world.
pub(crate) fn destroy_world(store: &mut Store, handle: u64) -> Result<(), &'static str> {
    let (index, generation) = split_handle(handle)?;
    if store.world_generations.get(index).copied() != Some(generation)
        || store.worlds.get(index).map(Option::is_none) != Some(false)
    {
        return Err("world handle does not address a live world");
    }
    store.worlds[index].as_ref().unwrap().require_owner()?;
    store.worlds[index] = None;
    Ok(())
}

/// Allocates an object in a world and returns its handle.
pub(crate) fn create_object(store: &mut Store, world: u64) -> Result<u64, &'static str> {
    let (world_index, world_generation) = split_handle(world)?;
    let world = live_world_mut(store, world_index, world_generation)?;
    world.advance_content()?;
    Ok(world.allocate_object())
}

/// Removes an object; the generation bump invalidates outstanding handles.
pub(crate) fn destroy_object(
    store: &mut Store,
    world: u64,
    object: u64,
) -> Result<(), &'static str> {
    let (world_index, world_generation) = split_handle(world)?;
    let world = live_world_mut(store, world_index, world_generation)?;
    world
        .object_index(object)
        .map_err(|_| "object handle does not address a live object")?;
    world.advance_content()?;
    world
        .remove_object(object)
        .map_err(|_| "object handle does not address a live object")?;
    readiness::observe(world);
    Ok(())
}

fn live_session_mut(
    store: &mut Store,
    handle: u64,
) -> Result<&mut integrity::Session, &'static str> {
    let (index, generation) = split_handle(handle)?;
    if store.world_generations.get(index).copied() != Some(generation) {
        return Err("world handle generation is stale");
    }
    store
        .worlds
        .get_mut(index)
        .and_then(Option::as_mut)
        .ok_or("world handle does not address a live world")
}
fn live_world_mut(
    store: &mut Store,
    index: usize,
    generation: u32,
) -> Result<&mut DisunityWorld, &'static str> {
    let session = live_session_mut(store, make_handle(index, generation))?;
    session.require_owner()?;
    strict::allow_mutation(&session.world).map_err(|_| "strict phase does not permit mutation")?;
    Ok(&mut session.world)
}

/// Runs `f` against a live world's state; used by the Rust-side bridge, never
/// exported through the ABI.
pub fn with_world<U>(handle: u64, f: impl FnOnce(&DisunityWorld) -> U) -> Result<U, &'static str> {
    let store = lock_store();
    let (index, generation) = split_handle(handle)?;
    if store.world_generations.get(index).copied() != Some(generation) {
        return Err("world handle generation is stale");
    }
    let world = store
        .worlds
        .get(index)
        .and_then(Option::as_ref)
        .ok_or("world handle does not address a live world")?;
    Ok(f(&world.world))
}

/// Records a submitted command batch as pending, bounded by
/// [`MAX_COMMANDS_PER_SUBMIT`].
pub(crate) fn submit_commands(
    store: &mut Store,
    world: u64,
    commands: &[DisunityCommand],
) -> Result<(), &'static str> {
    let (world_index, world_generation) = split_handle(world)?;
    let world = live_world_mut(store, world_index, world_generation)?;
    world.advance_content()?;
    world.pending_commands.extend_from_slice(commands);
    Ok(())
}

/// Applies pending commands during a step; a bad command records the error and
/// stops the step so the host can fetch the message.
pub(crate) fn step_world(store: &mut Store, world: u64, dt: f32, frame: u64) -> Result<(), String> {
    let (world_index, world_generation) = split_handle(world)?;
    let world = live_world_mut(store, world_index, world_generation)
        .map_err(std::string::ToString::to_string)?;
    if world.strict.is_some() {
        return Err("strict session requires authenticated native advancement".into());
    }
    world.clock.require_legacy()?;
    input::validate_dt(dt)?;
    world.advance_content()?;
    let pending = std::mem::take(&mut world.pending_commands);
    for command in pending {
        if let Err(reason) = world.apply_command(command) {
            world.last_error = Some(reason.to_string());
            return Err(reason.to_string());
        }
        world.applied_commands.push(command);
    }
    world.clock.begin_legacy(dt)?;
    world.clock.frame = frame;
    world.physics_step(dt * world.clock.time_scale)?;
    Ok(())
}

/// Stores the last error message for `disunity_error_message` to fetch.
pub(crate) fn record_error(store: &mut Store, world: Option<u64>, message: &str) {
    let Some(world) = world else { return };
    let (index, generation) = match split_handle(world) {
        Ok(split) => split,
        Err(_) => return,
    };
    if let Some(world) = store
        .worlds
        .get_mut(index)
        .and_then(Option::as_mut)
        .filter(|_| store.world_generations.get(index).copied() == Some(generation))
        && world.require_owner().is_ok()
    {
        world.world.last_error = Some(message.to_string());
    }
}

/// Number of live worlds and allocated slots; the soak test's drain check.
pub fn store_statistics() -> (usize, usize) {
    let store = lock_store();
    let live = store.worlds.iter().flatten().count();
    (live, store.worlds.len())
}

// Process-wide generations prevent handles from aliasing across world and resource owners.
fn next_generation() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    NEXT.try_update(
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
        |n| n.checked_add(1),
    )
    .expect("handle generation space exhausted")
}
