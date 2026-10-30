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

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use lua_resume::compiled::Compiled;
use lua_resume::import::{ImportChunk, ImportSources};
use lua_resume::restore::Sources;
use lua_resume::rewrite::normalize;
use lua_resume::strip::strip_debug_calls;
use lua_resume::syntax::ast::Block;
use lua_resume::syntax::{OperatorHooks, parse, print, print_with};
use mlua::{ChunkMode, Function, Lua, MultiValue, Value};
use sha2::{Digest, Sha256};

use crate::config::{NameCollisions, RefusedChunks};
use crate::error::ScriptError;

type Digest256 = [u8; 32];

/// The rewritten chunks by name, and how chunks are prepared.
#[derive(Default)]
pub(crate) struct Loader {
    /// The normalized trees a restore generates continuations from.
    sources: Sources,
    /// The same chunks as an import of another build's save reads them.
    chunks: BTreeMap<String, ImportChunk>,
    /// The text each name was first loaded with, to see a name used for another text.
    digests: BTreeMap<String, Digest256>,
    /// Chunks that loaded as plain text, by name, with the reason.
    plain: BTreeMap<String, String>,
    hooks: OperatorHooks,
    strip: bool,
    refused: RefusedChunks,
    collisions: NameCollisions,
}

pub(crate) type SharedLoader = Arc<Mutex<Loader>>;

impl Loader {
    /// The trees a restore regenerates code from, printed with the operator hooks the chunks
    /// were loaded with.
    pub(crate) fn sources(&self) -> Sources {
        self.sources.clone().with_hooks(self.hooks.clone())
    }

    /// The rewritten chunks as an import reads them, by the name saved prototypes carry:
    /// `@path` for the chunk loaded as `=path`.
    pub(crate) fn import_sources(&self) -> ImportSources {
        let mut sources = ImportSources::new();
        for (name, chunk) in &self.chunks {
            let saved = name.strip_prefix('=').map_or_else(
                || name.clone().into_bytes(),
                |path| format!("@{path}").into_bytes(),
            );
            sources.insert(&saved, chunk.clone());
        }
        sources
    }

    /// The chunks that loaded as plain text, with the reason each could not be resumable.
    pub(crate) fn plain(&self) -> BTreeMap<String, String> {
        self.plain.clone()
    }

    /// `text` printed with the operator hooks, for code that is run once and never saved. With
    /// no hooks it is returned as it is.
    pub(crate) fn hooked(&self, text: &str) -> Result<String, ScriptError> {
        if self.hooks == OperatorHooks::none() {
            return Ok(text.to_owned());
        }
        let tree = parse(text.as_bytes()).map_err(|error| refused("<eval>", &error.to_string()))?;
        Ok(print_with(&tree, &self.hooks))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.digests.is_empty()
    }

    pub(crate) fn set_strip(&mut self, strip: bool) {
        self.strip = strip;
    }

    pub(crate) fn set_hooks(&mut self, hooks: OperatorHooks) {
        self.hooks = hooks;
    }

    pub(crate) fn set_refused(&mut self, policy: RefusedChunks) {
        self.refused = policy;
    }

    pub(crate) fn set_collisions(&mut self, policy: NameCollisions) {
        self.collisions = policy;
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
        // Source stock Lua accepts and `lua-resume` cannot parse would run without the
        // operator hooks, so it is never loaded plain.
        let mut tree = parse(text).map_err(|error| refused(name, &error.to_string()))?;
        let stripped = self.strip && strip_debug_calls(&mut tree) > 0;
        let digest: Digest256 = Sha256::digest(text).into();
        if self.digests.get(name).is_some_and(|known| *known != digest) {
            return self.collided(name, &tree);
        }
        let dump = if stripped {
            compile(print(&tree).as_bytes())?.dump(true)
        } else {
            original.dump(true)
        };
        match rewrite(name, &tree, &dump) {
            Ok((normalized, chunk)) => Ok(self.keep(name, digest, normalized, chunk)),
            Err(reason) => self.not_resumable(name, digest, &tree, &reason),
        }
    }

    fn keep(
        &mut self,
        name: &str,
        digest: Digest256,
        normalized: Block,
        chunk: ImportChunk,
    ) -> String {
        let printed = print_with(&normalized, &self.hooks);
        self.digests.insert(name.to_owned(), digest);
        self.sources.insert(name, normalized);
        self.chunks.insert(name.to_owned(), chunk);
        printed
    }

    fn not_resumable(
        &mut self,
        name: &str,
        digest: Digest256,
        tree: &Block,
        reason: &str,
    ) -> Result<String, ScriptError> {
        if self.refused == RefusedChunks::Fail {
            return Err(refused(name, reason));
        }
        self.digests.entry(name.to_owned()).or_insert(digest);
        Ok(self.load_plain(name, tree, reason))
    }

    /// A name used for a second text. The first chunk's tree stays in place either way.
    fn collided(&mut self, name: &str, tree: &Block) -> Result<String, ScriptError> {
        let reason = "another chunk was loaded under this name";
        match self.collisions {
            NameCollisions::Reject => Err(ScriptError::Sandbox {
                message: format!("{name} cannot be loaded: {reason}"),
            }),
            NameCollisions::LoadPlain => Ok(self.load_plain(name, tree, reason)),
        }
    }

    fn load_plain(&mut self, name: &str, tree: &Block, reason: &str) -> String {
        self.plain.insert(name.to_owned(), reason.to_owned());
        print_with(tree, &self.hooks)
    }
}

/// The normalized tree of a chunk and its import record, or why the chunk cannot be resumed.
/// `dump` is the compilation of the original text, whose analysis guides the rewrite.
fn rewrite(name: &str, tree: &Block, dump: &[u8]) -> Result<(Block, ImportChunk), String> {
    let compiled = Compiled::from_dump(dump).map_err(|error| error.to_string())?;
    let normalized = normalize(tree, &compiled).map_err(|error| error.to_string())?;
    let chunk =
        ImportChunk::new(name, tree, normalized.clone()).map_err(|error| error.to_string())?;
    Ok((normalized, chunk))
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
