//! Saving and restoring a game's Lua data.
//!
//! A game saves the values at some global paths (its roots) and restores them into a fresh
//! state that has run the same scripts. The roots and every scheduler thread are saved by
//! `lua-resume` as one graph (see [`GameScripts::save`]): tables, closures and upvalue cells that
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

use std::collections::{BTreeMap, HashMap, VecDeque};

use lua_resume::capture::Snapshot;
use mlua::{Function, Lua, MultiValue, Thread, Value};

use crate::error::ScriptError;
use crate::original::{GameScripts, lookup, segments};
use crate::threads::{Restored, ThreadRecord, Wake};

/// When a saved thread runs next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum SavedWake {
    NextAdvance,
    At(u64),
    UntilWoken,
}

/// A scheduler thread and when it runs next. Its frames are in [`LuaSnapshot::graph`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SavedThread {
    pub wake: SavedWake,
    /// For a thread that had not run, how many arguments it starts with. Its function and
    /// arguments are roots of the graph. A thread that had run has `None`; its coroutines are
    /// the graph's captured threads, in the order of the threads that have run.
    pub start: Option<usize>,
}

/// How a saved thread is scheduled besides when it wakes: its clock, the wait it is in, the
/// game's flags and the kind of native it is parked in.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ThreadSchedule {
    /// The name of the thread's clock; none is the default clock.
    pub clock: Option<String>,
    /// The native path of the wait kind the thread is in; none for a sleeper.
    pub wait: Option<String>,
    pub flags: i64,
    pub source: String,
    /// The park kind of the native the thread waits in. A restore notes it again, so the
    /// thread can be adopted by a game that persists its own thread array.
    pub park: Option<String>,
}

/// What a save keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveOptions {
    /// Keeps only the scheduler threads whose flags share a bit with the mask. `None` keeps
    /// every scheduler thread, which refuses threads on other clocks or in waits: a game that
    /// has them says which it persists.
    pub persisted: Option<i64>,
}

/// Saved roots, scheduler threads and everything they reach.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct LuaSnapshot {
    /// The roots, the start of every thread that had not run, and all values they reach.
    pub graph: Snapshot,
    /// Scheduler threads in the order they would run.
    pub threads: Vec<SavedThread>,
    /// How each of `threads` is scheduled, in the same order.
    pub schedules: Vec<ThreadSchedule>,
    /// The time of every clock by name, in milliseconds.
    pub clocks: Vec<(String, u64)>,
}

impl GameScripts {
    /// Saves the values at `roots`, treating everything under `permanents` as code, and the
    /// scheduler threads `options` selects.
    pub fn save(
        &self,
        roots: &[&str],
        permanents: &[&str],
        options: &SaveOptions,
    ) -> Result<LuaSnapshot, ScriptError> {
        let names = permanent_names(self.lua(), self.natives(), permanents)?;
        let (records, clocks) = {
            let scheduler = self.scheduler().lock().expect("scheduler");
            if scheduler.is_running() {
                return Err(refused("cannot save from inside a scheduler thread".into()));
            }
            if !scheduler.is_saveable() {
                return Err(refused(
                    "cannot save a wait that was woken and has not run".into(),
                ));
            }
            if options.persisted.is_none() && !scheduler.is_plain() {
                return Err(refused(
                    "cannot save threads on other clocks or in waits without choosing the \
                     persisted threads"
                        .into(),
                ));
            }
            let mut records = scheduler.records();
            if let Some(mask) = options.persisted {
                records.retain(|record| record.schedule.flags & mask != 0);
            }
            let times = scheduler
                .clocks
                .iter()
                .map(|clock| (clock.name.clone(), clock.now))
                .collect::<Vec<_>>();
            (records, times)
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
        let schedules = records.iter().map(|r| r.schedule.clone()).collect();
        let named: Vec<(&str, Value)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        let graph = self.capture_graph(&named, &running, &known)?;
        Ok(LuaSnapshot {
            graph,
            threads,
            schedules,
            clocks,
        })
    }

    /// Restores a snapshot's roots into the shared namespace. Permanents are looked up by name
    /// among the natives and under `permanents` in the current state.
    pub fn restore(
        &mut self,
        snapshot: &LuaSnapshot,
        permanents: &[&str],
    ) -> Result<(), ScriptError> {
        if snapshot.schedules.len() != snapshot.threads.len() {
            return Err(refused(format!(
                "the snapshot has {} threads and {} schedules",
                snapshot.threads.len(),
                snapshot.schedules.len()
            )));
        }
        let lua = self.lua().clone();
        let names = permanent_names(&lua, self.natives(), permanents)?;
        let live = !snapshot.threads.is_empty();
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
        let saved_threads = snapshot.threads.iter().zip(&snapshot.schedules);
        for (index, (saved, schedule)) in saved_threads.enumerate() {
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
                wake: wake_of(saved.wake),
                schedule: schedule.clone(),
            });
        }
        for (root, value) in &restored.roots {
            if !is_start_root(root) {
                set_root(&lua, root, value.clone())?;
            }
        }
        if live {
            let mut scheduler = self.scheduler().lock().expect("scheduler");
            scheduler
                .restore(&snapshot.clocks, scheduled)
                .map_err(refused)?;
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
        });
    }
    (threads, running)
}

fn wake_of(wake: SavedWake) -> Wake {
    match wake {
        SavedWake::NextAdvance => Wake::NextAdvance,
        SavedWake::At(at) => Wake::At(at),
        SavedWake::UntilWoken => Wake::UntilWoken,
    }
}

/// A coroutine for each saved thread that had not run, to be given its function once the graph
/// is restored. Threads that had run are `None`; their frames are in the graph.
fn standin_threads(lua: &Lua, saved: &[SavedThread]) -> Result<Vec<Option<Thread>>, ScriptError> {
    let nothing = lua.create_function(|_, ()| Ok(()))?;
    saved
        .iter()
        .map(|thread| {
            thread
                .start
                .map(|_| lua.create_thread(nothing.clone()))
                .transpose()
                .map_err(ScriptError::from)
        })
        .collect()
}

/// The name of the root that holds the function scheduler thread `index` starts with.
fn function_root(index: usize) -> String {
    format!("scheduler thread {index} function")
}

/// The name of the root that holds argument `n` (from 1) of scheduler thread `index`.
fn argument_root(index: usize, n: usize) -> String {
    format!("scheduler thread {index} argument {n}")
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
pub(crate) fn permanent_names(
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
