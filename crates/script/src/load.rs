//! Makes every text chunk of a game's scripts resumable.
//!
//! A suspended thread can only be saved and continued mid-function when the code it runs is in
//! the form `lua-resume` understands. Each chunk therefore goes through one path before Lua
//! loads it: stock Lua compiles the original text (so syntax errors read as Lua's own), the
//! analysis of that compilation guides the rewrite, and the rewritten text is what runs, under
//! the original chunk name. The rewritten tree is kept by chunk name, because restoring a
//! thread generates its continuations from it.
//!
//! A chunk the rewrite refuses fails to load with an error naming the chunk and the reason.

use std::sync::{Arc, Mutex};

use lua_resume::compiled::Compiled;
use lua_resume::restore::Sources;
use lua_resume::rewrite::normalize;
use lua_resume::strip::strip_debug_calls;
use lua_resume::syntax::{parse, print};
use mlua::{ChunkMode, Function, Lua, MultiValue, Value};

use crate::error::ScriptError;

/// The rewritten chunks by name, and how chunks are prepared.
#[derive(Default)]
pub(crate) struct Loader {
    sources: Sources,
    strip: bool,
}

pub(crate) type SharedLoader = Arc<Mutex<Loader>>;

impl Loader {
    pub(crate) fn sources(&self) -> Sources {
        self.sources.clone()
    }

    pub(crate) fn set_strip(&mut self, strip: bool) {
        self.strip = strip;
    }

    /// The text to load for `text` under the chunk name `name`, and the chunk's tree is kept
    /// for restoring.
    pub(crate) fn prepare(
        &mut self,
        lua: &Lua,
        text: &[u8],
        name: &str,
    ) -> Result<String, ScriptError> {
        let compile = |text: &[u8]| {
            lua.load(text)
                .set_name(name)
                .set_mode(ChunkMode::Text)
                .into_function()
        };
        let original = compile(text)?;
        let mut tree = parse(text).map_err(|error| refused(name, &error.to_string()))?;
        let dump = if self.strip && strip_debug_calls(&mut tree) > 0 {
            compile(print(&tree).as_bytes())?.dump(true)
        } else {
            original.dump(true)
        };
        let compiled =
            Compiled::from_dump(&dump).map_err(|error| refused(name, &error.to_string()))?;
        let normalized =
            normalize(&tree, &compiled).map_err(|error| refused(name, &error.to_string()))?;
        let printed = print(&normalized);
        self.sources.insert(name, normalized);
        Ok(printed)
    }
}

fn refused(chunk: &str, reason: &str) -> ScriptError {
    ScriptError::Sandbox {
        message: format!("{chunk} cannot be made resumable: {reason}"),
    }
}

/// Replaces `load` in the shared namespace with one that prepares text chunks the same way.
/// Arguments and results follow Lua's `load`, which takes text only here: a binary chunk, or
/// any chunk that does not compile, gives `nil` and a message.
pub(crate) fn install(lua: &Lua, loader: &SharedLoader) -> mlua::Result<()> {
    let base: Function = lua.globals().raw_get("load")?;
    let loader = Arc::clone(loader);
    let load = lua.create_function(move |lua, args: MultiValue| {
        let args: Vec<Value> = args.into_iter().take(4).collect();
        let Some(chunk) = args.first() else {
            return base.call::<MultiValue>(args_of(args));
        };
        let (text, default_name) = match chunk_text(chunk)? {
            Some(found) => found,
            None => return base.call::<MultiValue>(args_of(args)),
        };
        let name = match args.get(1) {
            Some(Value::String(name)) => name.to_string_lossy().to_string(),
            _ => default_name,
        };
        let prepared = loader.lock().expect("loader").prepare(lua, &text, &name);
        let mut forwarded = args;
        if forwarded.len() < 3 {
            forwarded.resize(3, Value::Nil);
        }
        forwarded[1] = Value::String(lua.create_string(&name)?);
        forwarded[0] = match prepared {
            Ok(printed) => Value::String(lua.create_string(printed)?),
            Err(ScriptError::Sandbox { message }) => {
                return Ok(MultiValue::from_iter([
                    Value::Nil,
                    Value::String(lua.create_string(message)?),
                ]));
            }
            // Base `load` reports a syntax error in Lua's own words.
            Err(_) => Value::String(lua.create_string(&text)?),
        };
        forwarded[2] = Value::String(lua.create_string("t")?);
        base.call::<MultiValue>(args_of(forwarded))
    })?;
    lua.globals().raw_set("load", load)
}

fn args_of(args: Vec<Value>) -> MultiValue {
    MultiValue::from_iter(args)
}

/// The text of a `load` chunk argument, with Lua's default chunk name for it: a string is its
/// own name and a reader function's pieces are joined under `=(load)`. Anything else, and a
/// binary chunk, is left to the base `load` to refuse.
fn chunk_text(chunk: &Value) -> mlua::Result<Option<(Vec<u8>, String)>> {
    match chunk {
        Value::String(text) if !text.as_bytes().starts_with(b"\x1bLua") => Ok(Some((
            text.as_bytes().to_vec(),
            text.to_string_lossy().to_string(),
        ))),
        Value::Function(reader) => {
            let mut text = Vec::new();
            loop {
                match reader.call::<Value>(())? {
                    Value::String(piece) if !piece.as_bytes().is_empty() => {
                        text.extend_from_slice(&piece.as_bytes());
                    }
                    _ => break,
                }
            }
            Ok(Some((text, "=(load)".to_owned())))
        }
        _ => Ok(None),
    }
}
