//! Saving and restoring a game's Lua data.
//!
//! A game saves the values at some global paths (its roots) and restores them into a fresh
//! state that has run the same scripts. Tables shared by reference stay shared, cycles survive,
//! and integers stay distinct from floats. Functions and the tables that hold a game's code, such
//! as class tables, are not copied: they are permanents, saved by name and looked up by that name
//! when restoring, so a restored save runs the current code. Natives are permanents named by their
//! path. The game names further permanents by global path; every function and table reachable
//! from one through string keys is a permanent named by its path.
//!
//! Scheduler threads are saved as data: the permanent function each was spawned with, its
//! arguments and when it wakes. A restored thread starts its function again from the beginning
//! at that time, so state a thread must keep across a save belongs in tables, where the game's
//! own data already lives. Finished coroutines are saved as finished.
//!
//! A function that is not a permanent, such as a closure created at run time, cannot be saved
//! yet, nor can a suspended coroutine the scheduler does not own; saving one fails and names
//! where it was found.
//!
//! Snapshots are deterministic for a given state: string, number and boolean keys are written in
//! sorted order. Keys that are tables keep Lua's iteration order.

use std::collections::{BTreeMap, HashMap, VecDeque};

use mlua::{Function, Lua, MultiValue, Table, Thread, ThreadStatus, Value};

use crate::error::ScriptError;
use crate::original::{GameScripts, lookup, segments};
use crate::threads::{ThreadRecord, Wake};

/// One saved Lua value.
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
    /// A coroutine that had finished; it restores as finished.
    Finished,
}

/// A thread saved as the function it was spawned with, its arguments and its wake time.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SavedThread {
    /// A native or permanent; `Nil` for a finished coroutine.
    pub function: Saved,
    pub args: Vec<Saved>,
    pub wake: SavedWake,
}

/// One saved table: its entries and metatable.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SavedTable {
    pub entries: Vec<(Saved, Saved)>,
    pub metatable: Option<Saved>,
}

/// Saved roots and every table they reach.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct LuaSnapshot {
    pub roots: Vec<(String, Saved)>,
    pub tables: Vec<SavedTable>,
    /// Scheduler threads in the order they would run, then finished coroutines.
    #[serde(default)]
    pub threads: Vec<SavedThread>,
    /// The scheduler clock, in milliseconds.
    #[serde(default)]
    pub clock: u64,
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
        let mut saver = Saver {
            names,
            ids: HashMap::new(),
            tables: Vec::new(),
            pending: VecDeque::new(),
            threads: Vec::new(),
            thread_ids: HashMap::new(),
        };
        saver.scheduler_threads(&records)?;
        let mut saved = Vec::new();
        for &root in roots {
            let value = lookup(self.lua(), &segments(root)?)?;
            let slot = saver.slot(&value, root)?;
            if matches!(slot, Saved::Native(_) | Saved::Permanent(_)) {
                return Err(refused(format!("root {root} is a permanent, not data")));
            }
            saved.push((root.to_owned(), slot));
        }
        while let Some((id, table, location)) = saver.pending.pop_front() {
            saver.tables[id] = saver.fill(&table, &location)?;
        }
        Ok(LuaSnapshot {
            roots: saved,
            tables: saver.tables,
            threads: saver.threads,
            clock,
        })
    }

    /// Restores a snapshot's roots into the shared namespace. Permanents are looked up by name
    /// among the natives and under `permanents` in the current state.
    pub fn restore(
        &mut self,
        snapshot: &LuaSnapshot,
        permanents: &[&str],
    ) -> Result<(), ScriptError> {
        let lua = self.lua().clone();
        let by_name: HashMap<Saved, Value> = permanent_names(&lua, self.natives(), permanents)?
            .into_values()
            .collect();
        let live = snapshot
            .threads
            .iter()
            .any(|thread| thread.wake != SavedWake::Finished);
        if live && !self.scheduler().lock().expect("scheduler").is_idle() {
            return Err(refused(
                "cannot restore threads into a scheduler that already has threads".into(),
            ));
        }
        let tables = (0..snapshot.tables.len())
            .map(|_| lua.create_table())
            .collect::<mlua::Result<Vec<_>>>()?;
        let threads = new_threads(&lua, &by_name, &snapshot.threads)?;
        let restorer = Restorer {
            by_name,
            tables: &tables,
            threads: &threads,
        };
        for (table, saved) in tables.iter().zip(&snapshot.tables) {
            for (key, value) in &saved.entries {
                table.raw_set(restorer.value(&lua, key)?, restorer.value(&lua, value)?)?;
            }
        }
        for (table, saved) in tables.iter().zip(&snapshot.tables) {
            if let Some(metatable) = &saved.metatable {
                match restorer.value(&lua, metatable)? {
                    Value::Table(metatable) => table.set_metatable(Some(metatable)),
                    _ => return Err(refused("a metatable must be a table".into())),
                }
            }
        }
        for (root, slot) in &snapshot.roots {
            let path = segments(root)?;
            let (leaf, parent) = path.split_last().expect("paths have a segment");
            let Value::Table(parent) = lookup(&lua, parent)? else {
                return Err(refused(format!(
                    "cannot restore {root}: its parent is not a table"
                )));
            };
            parent.raw_set(leaf.as_str(), restorer.value(&lua, slot)?)?;
        }
        let scheduled = scheduled_threads(&lua, &restorer, &threads, &snapshot.threads)?;
        if live {
            let mut scheduler = self.scheduler().lock().expect("scheduler");
            scheduler.restore(snapshot.clock, scheduled);
        }
        Ok(())
    }
}

/// The live restored threads with their function, arguments and wake time, for the scheduler.
fn scheduled_threads(
    lua: &Lua,
    restorer: &Restorer,
    threads: &[(Thread, Option<Function>)],
    saved: &[SavedThread],
) -> Result<Vec<(Thread, Function, MultiValue, Wake)>, ScriptError> {
    let mut scheduled = Vec::new();
    for ((thread, function), saved) in threads.iter().zip(saved) {
        let wake = match saved.wake {
            SavedWake::NextAdvance => Wake::NextAdvance,
            SavedWake::At(at) => Wake::At(at),
            SavedWake::UntilWoken => Wake::UntilWoken,
            SavedWake::Finished => continue,
        };
        let args = saved
            .args
            .iter()
            .map(|arg| restorer.value(lua, arg))
            .collect::<Result<MultiValue, _>>()?;
        let function = function.clone().expect("live threads have functions");
        scheduled.push((thread.clone(), function, args, wake));
    }
    Ok(scheduled)
}

/// A fresh coroutine for each saved thread: live ones start their permanent function again,
/// finished ones are run to completion.
fn new_threads(
    lua: &Lua,
    by_name: &HashMap<Saved, Value>,
    saved: &[SavedThread],
) -> Result<Vec<(Thread, Option<Function>)>, ScriptError> {
    let mut threads = Vec::new();
    for (index, thread) in saved.iter().enumerate() {
        if thread.wake == SavedWake::Finished {
            let done = lua.create_thread(lua.create_function(|_, ()| Ok(()))?)?;
            done.resume::<()>(())?;
            threads.push((done, None));
            continue;
        }
        let Some(Value::Function(function)) = by_name.get(&thread.function).cloned() else {
            return Err(refused(format!(
                "thread {index}'s function {:?} is not a permanent in this state",
                thread.function
            )));
        };
        threads.push((lua.create_thread(function.clone())?, Some(function)));
    }
    Ok(threads)
}

/// Every permanent by identity, with its name: natives by path, then every function and table
/// reachable through string keys from each permanent root, named by its shortest path.
fn permanent_names(
    lua: &Lua,
    natives: &BTreeMap<String, mlua::Function>,
    roots: &[&str],
) -> Result<HashMap<usize, (Saved, Value)>, ScriptError> {
    let mut names = HashMap::new();
    for (path, native) in natives {
        let value = Value::Function(native.clone());
        names.insert(identity(&value), (Saved::Native(path.clone()), value));
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
        names.insert(identity(&value), (Saved::Permanent(path), value));
    }
    Ok(names)
}

fn identity(value: &Value) -> usize {
    value.to_pointer() as usize
}

struct Saver {
    names: HashMap<usize, (Saved, Value)>,
    ids: HashMap<usize, usize>,
    tables: Vec<SavedTable>,
    pending: VecDeque<(usize, Table, String)>,
    threads: Vec<SavedThread>,
    thread_ids: HashMap<usize, usize>,
}

impl Saver {
    /// Numbers the scheduler's threads first, so data that holds them refers to these entries,
    /// then saves each one's function and arguments.
    fn scheduler_threads(&mut self, records: &[ThreadRecord]) -> Result<(), ScriptError> {
        for (index, record) in records.iter().enumerate() {
            self.thread_ids
                .insert(record.thread.to_pointer() as usize, index);
        }
        for (index, record) in records.iter().enumerate() {
            let function = Value::Function(record.function.clone());
            let function = self.slot(
                &function,
                &format!("the function scheduler thread {index} was spawned with"),
            )?;
            let args = record
                .args
                .iter()
                .enumerate()
                .map(|(n, arg)| {
                    self.slot(
                        arg,
                        &format!("argument {} of scheduler thread {index}", n + 1),
                    )
                })
                .collect::<Result<_, _>>()?;
            let wake = match record.wake {
                Wake::NextAdvance => SavedWake::NextAdvance,
                Wake::At(at) => SavedWake::At(at),
                Wake::UntilWoken => SavedWake::UntilWoken,
            };
            self.threads.push(SavedThread {
                function,
                args,
                wake,
            });
        }
        Ok(())
    }

    /// A thread value: a scheduler thread by index, or a finished coroutine.
    fn thread(&mut self, thread: &Thread, location: &str) -> Result<Saved, ScriptError> {
        let key = thread.to_pointer() as usize;
        if let Some(&index) = self.thread_ids.get(&key) {
            return Ok(Saved::Thread(index));
        }
        if !matches!(
            thread.status(),
            ThreadStatus::Finished | ThreadStatus::Error
        ) {
            return Err(refused(format!(
                "{location} is a suspended coroutine the scheduler does not own; only scheduler threads can be saved"
            )));
        }
        let index = self.threads.len();
        self.thread_ids.insert(key, index);
        self.threads.push(SavedThread {
            function: Saved::Nil,
            args: Vec::new(),
            wake: SavedWake::Finished,
        });
        Ok(Saved::Thread(index))
    }

    fn slot(&mut self, value: &Value, location: &str) -> Result<Saved, ScriptError> {
        Ok(match value {
            Value::Nil => Saved::Nil,
            Value::Boolean(b) => Saved::Boolean(*b),
            Value::Integer(i) => Saved::Integer(*i),
            Value::Number(n) => Saved::Float(n.to_bits()),
            Value::String(s) => Saved::String(s.as_bytes().to_vec()),
            Value::Table(_) | Value::Function(_) => {
                if let Some((name, _)) = self.names.get(&identity(value)) {
                    return Ok(name.clone());
                }
                let Value::Table(table) = value else {
                    return Err(refused(format!(
                        "{location} is a function that is not a permanent; saving closures is not supported yet"
                    )));
                };
                self.table(table, location)
            }
            Value::Thread(thread) => self.thread(thread, location)?,
            other => {
                return Err(refused(format!(
                    "{location} is a {}, which cannot be saved",
                    other.type_name()
                )));
            }
        })
    }

    fn table(&mut self, table: &Table, location: &str) -> Saved {
        let key = table.to_pointer() as usize;
        if let Some(&id) = self.ids.get(&key) {
            return Saved::Table(id);
        }
        let id = self.tables.len();
        self.ids.insert(key, id);
        self.tables.push(SavedTable {
            entries: Vec::new(),
            metatable: None,
        });
        self.pending
            .push_back((id, table.clone(), location.to_owned()));
        Saved::Table(id)
    }

    fn fill(&mut self, table: &Table, location: &str) -> Result<SavedTable, ScriptError> {
        let pairs: Vec<(Value, Value)> =
            table.pairs::<Value, Value>().collect::<mlua::Result<_>>()?;
        let (mut plain, keyed): (Vec<_>, Vec<_>) = pairs
            .into_iter()
            .partition(|(key, _)| !matches!(key, Value::Table(_) | Value::Function(_)));
        plain.sort_by_key(|(key, _)| key_order(key));
        let mut entries = Vec::new();
        for (key, value) in plain.into_iter().chain(keyed) {
            let at = format!("{location}[{}]", describe(&key));
            let key = self.slot(&key, &format!("a key in {location}"))?;
            entries.push((key, self.slot(&value, &at)?));
        }
        let metatable = match table.metatable() {
            Some(metatable) => Some(self.slot(
                &Value::Table(metatable),
                &format!("the metatable of {location}"),
            )?),
            None => None,
        };
        Ok(SavedTable { entries, metatable })
    }
}

/// Sorts booleans, then numbers by value, then strings by bytes.
fn key_order(key: &Value) -> (u8, i128, u64, Vec<u8>) {
    let total = |n: f64| {
        let bits = n.to_bits();
        if bits >> 63 == 1 {
            !bits
        } else {
            bits | (1 << 63)
        }
    };
    match key {
        Value::Boolean(b) => (0, i128::from(*b), 0, Vec::new()),
        Value::Integer(i) => (1, i128::from(*i), 0, Vec::new()),
        Value::Number(n) => (2, 0, total(*n), Vec::new()),
        Value::String(s) => (3, 0, 0, s.as_bytes().to_vec()),
        _ => (4, 0, 0, Vec::new()),
    }
}

fn describe(key: &Value) -> String {
    match key {
        Value::String(s) => format!("{:?}", s.to_string_lossy()),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.type_name().to_owned(),
    }
}

struct Restorer<'a> {
    by_name: HashMap<Saved, Value>,
    tables: &'a [Table],
    threads: &'a [(Thread, Option<Function>)],
}

impl Restorer<'_> {
    fn value(&self, lua: &Lua, slot: &Saved) -> Result<Value, ScriptError> {
        Ok(match slot {
            Saved::Nil => Value::Nil,
            Saved::Boolean(b) => Value::Boolean(*b),
            Saved::Integer(i) => Value::Integer(*i),
            Saved::Float(bits) => Value::Number(f64::from_bits(*bits)),
            Saved::String(bytes) => Value::String(lua.create_string(bytes)?),
            Saved::Table(id) => Value::Table(
                self.tables
                    .get(*id)
                    .ok_or_else(|| refused(format!("table {id} is not in the snapshot")))?
                    .clone(),
            ),
            Saved::Thread(index) => Value::Thread(
                self.threads
                    .get(*index)
                    .ok_or_else(|| refused(format!("thread {index} is not in the snapshot")))?
                    .0
                    .clone(),
            ),
            Saved::Native(name) | Saved::Permanent(name) => {
                self.by_name.get(slot).cloned().ok_or_else(|| {
                    refused(format!("permanent {name} does not exist in this state"))
                })?
            }
        })
    }
}

fn refused(message: String) -> ScriptError {
    ScriptError::Sandbox { message }
}
