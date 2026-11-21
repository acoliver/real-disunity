//! The built-in `worldrunner.world` module.
//!
//! It reads the world from the host's app data at call time, so installing it once stays
//! correct across world swaps.

use std::sync::Arc;

use mlua::{Lua, UserData, UserDataMethods};

use crate::error::ScriptError;
use crate::world::{EntityHandle, WorldView};

/// Installs `worldrunner.world`: read-only terrain and entity queries.
pub(crate) fn install_world(lua: &Lua) -> Result<(), ScriptError> {
    let world_module = lua.create_table()?;
    world_module.raw_set(
        "terrain_height",
        lua.create_function(|lua, (east, north): (f64, f64)| {
            let world = live_world(lua)?;
            Ok(world.terrain_height(east, north))
        })?,
    )?;
    world_module.raw_set(
        "entities_by_family",
        lua.create_function(|lua, family: String| {
            let world = live_world(lua)?;
            let handles: Vec<HandleUserdata> = world
                .entities_by_family(&family)
                .into_iter()
                .map(HandleUserdata)
                .collect();
            let table = lua.create_table()?;
            for (index, handle) in handles.into_iter().enumerate() {
                table.raw_set(index + 1, handle)?;
            }
            Ok(table)
        })?,
    )?;
    world_module.raw_set(
        "generation",
        lua.create_function(|lua, ()| Ok(live_world(lua)?.generation()))?,
    )?;

    let root: mlua::Table = lua.globals().raw_get("worldrunner")?;
    root.raw_set("world", world_module)?;
    Ok(())
}

fn live_world(lua: &Lua) -> Result<mlua::AppDataRef<'_, Arc<dyn WorldView>>, mlua::Error> {
    lua.app_data_ref::<Arc<dyn WorldView>>()
        .ok_or_else(|| mlua::Error::RuntimeError("no world is loaded".into()))
}

struct HandleUserdata(EntityHandle);

impl UserData for HandleUserdata {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, ()| Ok(this.0.id));
        methods.add_method("generation", |_, this, ()| Ok(this.0.generation));
        methods.add_method("family", |lua, this, ()| {
            let world = lua
                .app_data_ref::<Arc<dyn WorldView>>()
                .ok_or_else(|| mlua::Error::RuntimeError("no world is loaded".into()))?;
            match world.entity_family(this.0) {
                Some(family) => Ok(Some(family)),
                None => Err(mlua::Error::RuntimeError(format!(
                    "handle invalid: entity {} of generation {}",
                    this.0.id, this.0.generation
                ))),
            }
        });
    }
}
