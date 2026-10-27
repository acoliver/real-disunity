//! Sandbox rules shared by the strict engine host and the original-script profile.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Function, HookTriggers, Lua, VmState};

use crate::error::ScriptError;

/// Counts instructions in steps of 1024 into `counter`, raising a Lua error once a call has
/// spent more than `budget`. Callers reset the counter before each entry into Lua.
pub(crate) fn install_budget(lua: &Lua, budget: u32, counter: &Arc<AtomicU64>) {
    let counter = Arc::clone(counter);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(1024),
        move |_, _| {
            let spent = counter.fetch_add(1024, Ordering::Relaxed) + 1024;
            if spent > u64::from(budget) {
                return Err(mlua::Error::RuntimeError(format!(
                    "instruction budget of {budget} exceeded"
                )));
            }
            Ok(VmState::Continue)
        },
    );
}

/// Removes the base library's file loaders and makes `load` compile text only, keeping its
/// other arguments as given: an explicit `env` of `nil` differs from an absent one.
pub(crate) fn restrict_base(lua: &Lua) -> Result<(), ScriptError> {
    let globals = lua.globals();
    for name in ["dofile", "loadfile"] {
        globals.raw_set(name, mlua::Value::Nil)?;
    }
    let load: Function = globals.raw_get("load")?;
    let text_only = lua.create_function(move |lua, args: mlua::MultiValue| {
        let mut args: Vec<mlua::Value> = args.into_iter().take(4).collect();
        if args.len() < 3 {
            args.resize(3, mlua::Value::Nil);
        }
        args[2] = mlua::Value::String(lua.create_string("t")?);
        load.call::<mlua::MultiValue>(mlua::MultiValue::from_iter(args))
    })?;
    globals.raw_set("load", text_only)?;
    Ok(())
}

pub(crate) fn table_to_json(table: &mlua::Table) -> Result<serde_json::Value, mlua::Error> {
    // Arrays stay arrays; keyed tables become objects; recursion is bounded
    // by Lua's own table depth, which the budget hook caps in instructions.
    let mut map = serde_json::Map::new();
    let mut array = Vec::new();
    for pair in table.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair?;
        match key {
            mlua::Value::Integer(i) => array.push((i, lua_value_to_json(&value))),
            other => {
                let key = match other {
                    mlua::Value::String(s) => s.to_string_lossy().to_owned(),
                    ref v => format!("{v:?}"),
                };
                map.insert(key, lua_value_to_json(&value));
            }
        }
    }
    if map.is_empty() && !array.is_empty() {
        array.sort_by_key(|(i, _)| *i);
        return Ok(serde_json::Value::Array(
            array.into_iter().map(|(_, v)| v).collect(),
        ));
    }
    Ok(serde_json::Value::Object(map))
}

pub(crate) fn lua_value_to_json(value: &mlua::Value) -> serde_json::Value {
    match value {
        mlua::Value::Nil => serde_json::Value::Null,
        mlua::Value::Boolean(b) => serde_json::Value::Bool(*b),
        mlua::Value::Integer(i) => serde_json::json!(i),
        mlua::Value::Number(n) => serde_json::json!(n),
        mlua::Value::String(s) => serde_json::Value::String(s.to_string_lossy().to_owned()),
        mlua::Value::Table(t) => table_to_json(t).unwrap_or(serde_json::Value::Null),
        other => serde_json::Value::String(format!("{other:?}")),
    }
}
