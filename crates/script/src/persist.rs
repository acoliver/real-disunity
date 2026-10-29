//! Saving and restoring a game's Lua data.
//!
//! A game saves the values at some global paths (its roots) and restores them into a fresh
//! state that has run the same scripts. The roots and every scheduler thread are saved by
//! `lua-resume` as one graph (see [`crate::resume`]): tables, closures and upvalue cells that
//! several roots or threads share stay shared, cycles survive, and integers stay distinct from
//! floats. Functions and the tables that hold a game's code, such as class tables, are not
//! copied: they are permanents, saved by name and looked up by that name when restoring, so a
//! restored save runs the current code. Natives are permanents named by their path. The game
//! names further permanents by global path; every function and table reachable from one
//! through string keys is a permanent named by its path.
//!
//! A scheduler thread that has run is saved where it stopped: its frames, locals and closures,
//! so it continues in the middle of its function after a restore. A thread that has not run yet
//! is saved as the function it was spawned with, which may be a closure, and its arguments.
//! Finished coroutines are saved as finished, and a suspended coroutine that a thread or the
//! data holds is saved where it stopped.
//!
//! Snapshots written before all state was one graph still restore (see [`crate::legacy`]).

use std::collections::{BTreeMap, HashMap, VecDeque};

use lua_resume::capture::Snapshot;
use mlua::{Function, Lua, MultiValue, Thread, Value};

use crate::error::ScriptError;
use crate::legacy::{argument_root, current, function_root};
use crate::original::{GameScripts, lookup, segments};
use crate::threads::{Restored, ThreadRecord, Wake};

/// One saved Lua value in the format that predates the graph. Snapshots are read in this format
/// and never written in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub enum Saved {
    Nil,
    Boolean(bool),
    Integer(i64),
    /// IEEE-754 bits, so every float, including NaN and infinities, round-trips exactly.
    Float(u64),
    /// Lua strings are bytes.
    String(Vec<u8>),
    /// An index into [`LuaSnapshot::tables`].
    Table(usize),
    /// A native, by its registered path.
    Native(String),
    /// A permanent, by its global path.
    Permanent(String),
    /// An index into [`LuaSnapshot::threads`].
    Thread(usize),
}

/// When a saved thread runs next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum SavedWake {
    NextAdvance,
    At(u64),
    UntilWoken,
    /// A coroutine that had finished, in snapshots that predate the graph; it restores as
    /// finished.
    Finished,
}

/// A scheduler thread and when it runs next. Its frames are in [`LuaSnapshot::graph`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SavedThread {
    pub wake: SavedWake,
    /// For a thread that had not run, how many arguments it starts with. Its function and
    /// arguments are roots of the graph. A thread that had run has `None`; its coroutines are
    /// the graph's captured threads, in the order of the threads that have run.
    #[serde(default)]
    pub start: Option<usize>,
    /// The function of a thread that had not run, in the format that predates the graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<Saved>,
    /// Its arguments, in the same format.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Saved>,
}

/// One saved table in the format that predates the graph.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SavedTable {
    pub entries: Vec<(Saved, Saved)>,
    pub metatable: Option<Saved>,
}

/// Saved roots, scheduler threads and everything they reach.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct LuaSnapshot {
    /// The roots, the start of every thread that had not run, and all values they reach.
    #[serde(default)]
    pub graph: Snapshot,
    /// Scheduler threads in the order they would run.
    #[serde(default)]
    pub threads: Vec<SavedThread>,
    /// The scheduler clock, in milliseconds.
    #[serde(default)]
    pub clock: u64,
    /// Roots in the format that predates the graph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<(String, Saved)>,
    /// Tables in the format that predates the graph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<SavedTable>,
}

impl GameScripts {
    /// Saves the values at `roots`, treating everything under `permanents` as code.
    pub fn save(&self, roots: &[&str], permanents: &[&str]) -> Result<LuaSnapshot, ScriptError> {
        let names = permanent_names(self.lua(), self.natives(), permanents)?;
        let (records, clock) = {
            let scheduler = self.scheduler().lock().expect("scheduler");
            if scheduler.is_running() {
                return Err(refused("cannot save from inside a scheduler thread".into()));
            }
            (scheduler.records(), scheduler.now())
        };
        let unstarted: Vec<(usize, Thread)> = records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.start.is_some())
            .map(|(index, record)| (index, record.thread.clone()))
            .collect();
        let known = self.permanents_for(&names, &unstarted);
        let mut values = Vec::new();
        for &root in roots {
            let value = lookup(self.lua(), &segments(root)?)?;
            if known.name_of(&value).is_some() && !matches!(value, Value::Thread(_)) {
                return Err(refused(format!("root {root} is a permanent, not data")));
            }
            values.push((root.to_owned(), value));
        }
        let (threads, running) = thread_starts(&records, &mut values);
        let named: Vec<(&str, Value)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        let graph = self.capture_graph(&named, &running, &known)?;
        Ok(LuaSnapshot {
            graph,
            threads,
            clock,
            ..LuaSnapshot::default()
        })
    }

    /// Restores a snapshot's roots into the shared namespace. Permanents are looked up by name
    /// among the natives and under `permanents` in the current state.
    pub fn restore(
        &mut self,
        snapshot: &LuaSnapshot,
        permanents: &[&str],
    ) -> Result<(), ScriptError> {
        let snapshot = current(snapshot)?;
        let lua = self.lua().clone();
        let names = permanent_names(&lua, self.natives(), permanents)?;
        let live = snapshot
            .threads
            .iter()
            .any(|thread| thread.wake != SavedWake::Finished);
        if live && !self.scheduler().lock().expect("scheduler").is_idle() {
            return Err(refused(
                "cannot restore threads into a scheduler that already has threads".into(),
            ));
        }
        let standins = standin_threads(&lua, &snapshot.threads)?;
        let listed: Vec<(usize, Thread)> = standins
            .iter()
            .enumerate()
            .filter_map(|(index, thread)| thread.clone().map(|thread| (index, thread)))
            .collect();
        let known = self.permanents_for(&names, &listed);
        let restored = self.restore_graph(&snapshot.graph, &known)?;
        let mut scheduled = Vec::new();
        let mut running = restored.threads.iter();
        for (index, saved) in snapshot.threads.iter().enumerate() {
            let Some(wake) = wake_of(saved.wake) else {
                continue;
            };
            let (thread, start) = match saved.start {
                Some(count) => {
                    let standin = standins[index].clone().expect("a stand-in");
                    let start = start_of(&restored.roots, index, count)?;
                    standin.reset(start.0.clone())?;
                    (standin, Some(start))
                }
                None => match running.next() {
                    Some(thread) => (thread.clone(), None),
                    None => return Err(refused(format!("thread {index} is not in the snapshot"))),
                },
            };
            scheduled.push(Restored {
                thread,
                start,
                wake,
            });
        }
        for (root, value) in &restored.roots {
            if !is_start_root(root) {
                set_root(&lua, root, value.clone())?;
            }
        }
        if live {
            let mut scheduler = self.scheduler().lock().expect("scheduler");
            scheduler.restore(snapshot.clock, scheduled);
        }
        Ok(())
    }
}

/// The saved threads, and the roots that hold the function and arguments of those that had not
/// run, added to `values`. Also the coroutines of the threads that had run, in order.
fn thread_starts(
    records: &[ThreadRecord],
    values: &mut Vec<(String, Value)>,
) -> (Vec<SavedThread>, Vec<Thread>) {
    let mut threads = Vec::new();
    let mut running = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let start = record.start.as_ref().map(|(function, args)| {
            values.push((function_root(index), Value::Function(function.clone())));
            for (n, arg) in args.iter().enumerate() {
                values.push((argument_root(index, n + 1), arg.clone()));
            }
            args.len()
        });
        if start.is_none() {
            running.push(record.thread.clone());
        }
        threads.push(SavedThread {
            wake: match record.wake {
                Wake::NextAdvance => SavedWake::NextAdvance,
                Wake::At(at) => SavedWake::At(at),
                Wake::UntilWoken => SavedWake::UntilWoken,
            },
            start,
            function: None,
            args: Vec::new(),
        });
    }
    (threads, running)
}

fn wake_of(wake: SavedWake) -> Option<Wake> {
    match wake {
        SavedWake::NextAdvance => Some(Wake::NextAdvance),
        SavedWake::At(at) => Some(Wake::At(at)),
        SavedWake::UntilWoken => Some(Wake::UntilWoken),
        SavedWake::Finished => None,
    }
}

/// A coroutine for each saved thread that has no frames in the graph: one that had not run gets
/// a thread to be given its function once the graph is restored, and a finished one is run to
/// completion. Threads that had run are `None`.
fn standin_threads(lua: &Lua, saved: &[SavedThread]) -> Result<Vec<Option<Thread>>, ScriptError> {
    let nothing = lua.create_function(|_, ()| Ok(()))?;
    let mut threads = Vec::new();
    for thread in saved {
        threads.push(if thread.wake == SavedWake::Finished {
            let done = lua.create_thread(nothing.clone())?;
            done.resume::<()>(())?;
            Some(done)
        } else if thread.start.is_some() {
            Some(lua.create_thread(nothing.clone())?)
        } else {
            None
        });
    }
    Ok(threads)
}

fn is_start_root(name: &str) -> bool {
    name.starts_with("scheduler thread ")
}

/// The function and arguments thread `index` starts with, from the restored roots.
fn start_of(
    roots: &[(String, Value)],
    index: usize,
    count: usize,
) -> Result<(Function, MultiValue), ScriptError> {
    let find = |name: String| roots.iter().find(|(root, _)| *root == name).map(|r| &r.1);
    let Some(Value::Function(function)) = find(function_root(index)) else {
        return Err(refused(format!(
            "thread {index}'s function is not a function in this state"
        )));
    };
    let args = (1..=count)
        .map(|n| find(argument_root(index, n)).cloned().unwrap_or(Value::Nil))
        .collect();
    Ok((function.clone(), args))
}

fn set_root(lua: &Lua, root: &str, value: Value) -> Result<(), ScriptError> {
    let path = segments(root)?;
    let (leaf, parent) = path.split_last().expect("paths have a segment");
    let Value::Table(parent) = lookup(lua, parent)? else {
        return Err(refused(format!(
            "cannot restore {root}: its parent is not a table"
        )));
    };
    parent.raw_set(leaf.as_str(), value)?;
    Ok(())
}

/// Every permanent with its name: natives by path, then every function and table reachable
/// through string keys from each permanent root, named by its shortest path.
fn permanent_names(
    lua: &Lua,
    natives: &BTreeMap<String, mlua::Function>,
    roots: &[&str],
) -> Result<Vec<(String, Value)>, ScriptError> {
    let mut names = HashMap::new();
    for (path, native) in natives {
        let value = Value::Function(native.clone());
        names.insert(identity(&value), (format!("native {path}"), value));
    }
    let mut queue = VecDeque::new();
    for &root in roots {
        let value = lookup(lua, &segments(root)?)?;
        if !matches!(value, Value::Table(_) | Value::Function(_)) {
            return Err(refused(format!(
                "permanent root {root} is a {}, not a table or function",
                value.type_name()
            )));
        }
        queue.push_back((root.to_owned(), value));
    }
    while let Some((path, value)) = queue.pop_front() {
        if names.contains_key(&identity(&value)) {
            continue;
        }
        if let Value::Table(table) = &value {
            let mut children: Vec<(String, Value)> = table
                .pairs::<Value, Value>()
                .filter_map(Result::ok)
                .filter_map(|(key, child)| match (key, &child) {
                    (Value::String(key), Value::Table(_) | Value::Function(_)) => {
                        Some((format!("{path}.{}", key.to_string_lossy()), child))
                    }
                    _ => None,
                })
                .collect();
            children.sort_by(|a, b| a.0.cmp(&b.0));
            queue.extend(children);
        }
        names.insert(identity(&value), (format!("permanent {path}"), value));
    }
    let mut named: Vec<_> = names.into_values().collect();
    named.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(named)
}

fn identity(value: &Value) -> usize {
    value.to_pointer() as usize
}

pub(crate) fn refused(message: String) -> ScriptError {
    ScriptError::Sandbox { message }
}
