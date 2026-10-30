//! Registering the scheduler's natives under a game's names.

use std::sync::Arc;

use mlua::{ChunkMode, Function, Lua, Table};

use super::lua::{SLEEP_CHUNK, WAIT_CHUNK, sleep_text, wait_text};
use super::natives;
use super::policy::SchedulerPolicy;
use super::state::{Scheduler, WaitSpec};
use super::{SchedulerNames, Shared};
use crate::error::ScriptError;
use crate::original::GameScripts;

impl GameScripts {
    /// Registers the scheduler's functions as natives under the game's names, with the engine's
    /// own scheduler policy.
    pub fn register_scheduler(&mut self, names: &SchedulerNames) -> Result<(), ScriptError> {
        self.register_scheduler_with(names, &SchedulerPolicy::default())
    }

    /// Registers the scheduler's functions under the game's names, with the semantics of
    /// `policy`. Natives that a policy names, including the `on_resume` hook of a wait, are
    /// registered by the game; register the hooks before calling this.
    pub fn register_scheduler_with(
        &mut self,
        names: &SchedulerNames,
        policy: &SchedulerPolicy,
    ) -> Result<(), ScriptError> {
        let lua = self.lua().clone();
        let waits = self.wait_specs(policy)?;
        if policy.clocks.is_empty() {
            return Err(refused("a scheduler needs at least one clock"));
        }
        let shared: Shared = Arc::clone(self.scheduler());
        {
            let mut scheduler = shared.lock().expect("scheduler");
            if !scheduler.is_idle() {
                return Err(refused("the scheduler already has threads"));
            }
            *scheduler = Scheduler::new(policy, waits);
            scheduler.sleep_native.clone_from(&names.sleep);
        }
        let sleep = self.yielding(&sleep_text(policy), SLEEP_CHUNK, &[])?;
        let mut natives = vec![
            (names.spawn.clone(), natives::spawn(&lua, &shared, 0, 0)?),
            (names.sleep.clone(), sleep),
            (names.wakeup.clone(), natives::wakeup(&lua, &shared)?),
            (names.kill.clone(), natives::kill(&lua, &shared)?),
            (names.current.clone(), natives::current(&lua, &shared)?),
            (names.now.clone(), natives::now(&lua, &shared)?),
        ];
        natives.extend(self.policy_natives(&lua, &shared, policy)?);
        for (name, function) in natives {
            self.register_function(&name, function)?;
        }
        Ok(())
    }

    fn policy_natives(
        &self,
        lua: &Lua,
        shared: &Shared,
        policy: &SchedulerPolicy,
    ) -> Result<Vec<(String, Function)>, ScriptError> {
        let index = |name: &str| {
            policy
                .clocks
                .iter()
                .position(|clock| clock == name)
                .ok_or_else(|| refused(&format!("no clock named {name}")))
        };
        let mut out = Vec::new();
        for spawn in &policy.spawn_on {
            let function = natives::spawn(lua, shared, index(&spawn.clock)?, 0)?;
            out.push((spawn.native.clone(), function));
        }
        for spawn in &policy.spawn_flagged {
            let function = natives::spawn(lua, shared, index(&spawn.clock)?, spawn.flags)?;
            out.push((spawn.native.clone(), function));
        }
        for set in &policy.set_clock {
            let function = natives::set_clock(lua, shared, index(&set.clock)?)?;
            out.push((set.native.clone(), function));
        }
        for read in &policy.read_clock {
            let function = natives::read_clock(lua, shared, index(&read.clock)?)?;
            out.push((read.native.clone(), function));
        }
        if let Some(name) = &policy.wake {
            out.push((name.clone(), natives::wake(lua, shared)?));
        }
        out.extend(thread_natives(lua, shared, policy)?);
        let text = wait_text(policy);
        for (kind, wait) in policy.waits.iter().enumerate() {
            let kind = i64::try_from(kind).map_err(|_| refused("too many wait kinds"))?;
            out.push((
                wait.native.clone(),
                self.yielding(&text, WAIT_CHUNK, &[mlua::Value::Integer(kind)])?,
            ));
        }
        Ok(out)
    }

    fn wait_specs(&self, policy: &SchedulerPolicy) -> Result<Vec<WaitSpec>, ScriptError> {
        policy
            .waits
            .iter()
            .map(|wait| {
                let on_resume = match &wait.on_resume {
                    None => None,
                    Some(path) => Some(self.natives().get(path).cloned().ok_or_else(|| {
                        refused(&format!("on_resume native {path} is not registered"))
                    })?),
                };
                Ok(WaitSpec {
                    native: wait.native.clone(),
                    timeout: wait.timeout_value.clone(),
                    on_resume,
                })
            })
            .collect()
    }

    /// Loads a yielding function from its chunk, given the coroutine yield and the token.
    fn yielding(
        &self,
        text: &str,
        chunk: &str,
        extra: &[mlua::Value],
    ) -> Result<Function, ScriptError> {
        let lua = self.lua();
        let coroutine: Table = lua.globals().get("coroutine")?;
        let yield_fn: Function = coroutine.get("yield")?;
        let prepared = self.prepare(text.as_bytes(), chunk)?;
        let make: Function = lua
            .load(prepared)
            .set_name(chunk)
            .set_mode(ChunkMode::Text)
            .into_function()?;
        let mut args = vec![
            mlua::Value::Function(yield_fn),
            mlua::Value::Table(self.sleep_token().clone()),
        ];
        args.extend_from_slice(extra);
        Ok(make.call(mlua::MultiValue::from_iter(args))?)
    }
}

/// The natives over a thread's flags, source and clock, and the one that interrupts an advance.
fn thread_natives(
    lua: &Lua,
    shared: &Shared,
    policy: &SchedulerPolicy,
) -> Result<Vec<(String, Function)>, ScriptError> {
    let mut out = Vec::new();
    if let Some(flags) = &policy.thread_flags {
        if let Some(name) = &flags.get {
            out.push((name.clone(), natives::get_flags(lua, shared)?));
        }
        if let Some(name) = &flags.set {
            out.push((name.clone(), natives::set_flags(lua, shared)?));
        }
        if let Some(name) = &flags.has {
            out.push((name.clone(), natives::has_flags(lua, shared)?));
        }
    }
    if let Some(name) = &policy.set_source {
        out.push((name.clone(), natives::set_source(lua, shared)?));
    }
    if let Some(name) = &policy.clock_of {
        out.push((name.clone(), natives::clock_of(lua, shared)?));
    }
    if let Some(name) = &policy.interrupt {
        out.push((name.clone(), natives::interrupt(lua, shared)?));
    }
    Ok(out)
}

fn refused(message: &str) -> ScriptError {
    ScriptError::Sandbox {
        message: message.into(),
    }
}
