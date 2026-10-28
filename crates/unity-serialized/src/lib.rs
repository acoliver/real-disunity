//! Reader for Unity's binary serialized files.
//!
//! A Unity player stores its scenes and assets in serialized files (`globalgamemanagers`,
//! `level0`, `sharedassets0.assets`, `resources.assets` and so on). Each starts with a header
//! and a metadata block that lists the file's types, its objects with their byte ranges, the
//! script classes it uses and the other files its references point into. This crate reads
//! that header and metadata for format versions 19 to 22 (Unity 2019.1 through Unity 6).
//!
//! The reader works on bytes the caller supplies and performs no I/O. Object data is located
//! by [`ObjectInfo::offset`] and [`ObjectInfo::size`], and [`object::decode`] reads it with a
//! class layout from [`classes`].

pub mod camera;
mod class;
pub mod classes;
mod cursor;
mod error;
mod file;
pub mod font;
pub mod graphics;
mod header;
pub mod light;
pub mod lz4;
pub mod mesh;
pub mod object;
pub mod shader;
pub mod shader_program;
pub mod shader_state;
pub mod sprite;
pub mod text_asset;
mod type_tree;
pub mod ui;
pub mod write;

pub use class::ClassId;
pub use error::SerializedError;
pub use file::{External, ObjectInfo, ScriptClass, ScriptRef, SerializedFile, SerializedType};
pub use header::{Header, MAX_VERSION, MIN_VERSION};
pub use type_tree::{NodeString, TypeTreeNode};
