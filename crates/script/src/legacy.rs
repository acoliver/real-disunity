//! Reading snapshots saved before all state was one graph.
//!
//! Those snapshots hold their own tables, and their threads are permanent functions with
//! arguments, finished coroutines or, in development builds, a graph per thread. They are
//! converted to the current form when restored.

use std::borrow::Cow;

use lua_resume::capture::{Snapshot, TableData, Value};

use crate::error::ScriptError;
use crate::persist::{LuaSnapshot, Saved, SavedThread, SavedWake};

/// The name of the root that holds the function scheduler thread `index` starts with.
pub(crate) fn function_root(index: usize) -> String {
    format!("scheduler thread {index} function")
}

/// The name of the root that holds argument `n` (from 1) of scheduler thread `index`.
pub(crate) fn argument_root(index: usize, n: usize) -> String {
    format!("scheduler thread {index} argument {n}")
}

/// The snapshot in its current form.
pub(crate) fn current(snapshot: &LuaSnapshot) -> Result<Cow<'_, LuaSnapshot>, ScriptError> {
    let older = !snapshot.roots.is_empty()
        || !snapshot.tables.is_empty()
        || snapshot
            .threads
            .iter()
            .any(|thread| thread.function.is_some());
    if !older {
        return Ok(Cow::Borrowed(snapshot));
    }
    let mut graph = Snapshot::default();
    for table in &snapshot.tables {
        graph.tables.push(TableData {
            entries: table
                .entries
                .iter()
                .map(|(key, value)| (value_of(key), value_of(value)))
                .collect(),
            metatable: match &table.metatable {
                Some(Saved::Table(id)) => Some(*id),
                Some(_) => {
                    return Err(crate::persist::refused(
                        "a metatable must be a table".into(),
                    ));
                }
                None => None,
            },
        });
    }
    for (name, slot) in &snapshot.roots {
        graph.roots.push((name.clone(), value_of(slot)));
    }
    let mut threads = Vec::new();
    for (index, thread) in snapshot.threads.iter().enumerate() {
        threads.push(upgrade_thread(&mut graph, index, thread)?);
    }
    Ok(Cow::Owned(LuaSnapshot {
        graph,
        threads,
        clock: snapshot.clock,
        ..LuaSnapshot::default()
    }))
}

fn upgrade_thread(
    graph: &mut Snapshot,
    index: usize,
    thread: &SavedThread,
) -> Result<SavedThread, ScriptError> {
    let mut upgraded = SavedThread {
        wake: thread.wake,
        start: None,
        function: None,
        args: Vec::new(),
    };
    if thread.wake == SavedWake::Finished {
        return Ok(upgraded);
    }
    let function = match &thread.function {
        Some(Saved::Nil) | None => {
            return Err(crate::persist::refused(format!(
                "thread {index} was saved by a development build and has no function to start"
            )));
        }
        Some(function) => function,
    };
    graph.roots.push((function_root(index), value_of(function)));
    for (n, arg) in thread.args.iter().enumerate() {
        graph
            .roots
            .push((argument_root(index, n + 1), value_of(arg)));
    }
    upgraded.start = Some(thread.args.len());
    Ok(upgraded)
}

fn value_of(saved: &Saved) -> Value {
    match saved {
        Saved::Nil => Value::Nil,
        Saved::Boolean(b) => Value::Boolean(*b),
        Saved::Integer(i) => Value::Integer(*i),
        Saved::Float(bits) => Value::Float(*bits),
        Saved::String(bytes) => Value::String(bytes.clone()),
        Saved::Table(id) => Value::Table(*id),
        Saved::Native(path) => Value::Permanent(format!("native {path}")),
        Saved::Permanent(path) => Value::Permanent(format!("permanent {path}")),
        Saved::Thread(index) => Value::Permanent(format!("scheduler thread {index}")),
    }
}
