//! Built-in `worldrunner.world` and `worldrunner.assets` modules.
//!
//! They read the world and resolver from the host's app data at call time, so
//! installing them once stays correct across world swaps.

use std::sync::Arc;

use mlua::{Lua, UserData, UserDataMethods};

use crate::api::ScriptApi;
use crate::error::ScriptError;
use crate::world::{EntityHandle, WorldView};

/// Resolves logical asset ids for `worldrunner.assets.resolve`. Hosts install
/// an implementation that knows their package.
pub trait AssetResolver: Send + Sync {
    /// Returns the resolved asset identity, or `None` when unknown.
    fn resolve(&self, logical_id: &str) -> Option<u64>;
}

/// `worldrunner.world`: read-only terrain and entity queries.
pub struct WorldApi;

impl ScriptApi for WorldApi {
    fn name(&self) -> &'static str {
        "world"
    }

    fn api_version(&self) -> (u32, u32, u32) {
        (0, 1, 0)
    }

    fn install(&self, lua: &Lua) -> Result<(), ScriptError> {
        let world_module = lua.create_table()?;
        world_module.raw_set("api_version_fn", lua.create_function(|_, ()| Ok("0.1.0"))?)?;
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
}

/// `worldrunner.assets`: logical-id resolution against the host package.
pub struct AssetsApi;

impl ScriptApi for AssetsApi {
    fn name(&self) -> &'static str {
        "assets"
    }

    fn api_version(&self) -> (u32, u32, u32) {
        (0, 1, 0)
    }

    fn install(&self, lua: &Lua) -> Result<(), ScriptError> {
        let assets = lua.create_table()?;
        assets.raw_set("api_version_fn", lua.create_function(|_, ()| Ok("0.1.0"))?)?;
        assets.raw_set(
            "resolve",
            lua.create_function(|lua, logical_id: String| {
                let resolver = lua
                    .app_data_ref::<Arc<dyn AssetResolver>>()
                    .ok_or_else(|| {
                        mlua::Error::RuntimeError("no asset resolver is installed".into())
                    })?;
                Ok(resolver.resolve(&logical_id))
            })?,
        )?;
        let root: mlua::Table = lua.globals().raw_get("worldrunner")?;
        root.raw_set("assets", assets)?;
        Ok(())
    }
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
