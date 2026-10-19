//! Caller-configured, fail-closed production capability handlers.
//!
//! Every handler in this module serves exactly one caller-authored route: the
//! caller is the sole authority for the exact capability URI that selects the
//! handler, the exact payload-contract URI its contributions carry, the
//! handler descriptor identity and revision, the element policy bindings must
//! satisfy, and every operational and decode bound. There is no compiled
//! route, dataset dimension, filename, default, or fallback registry, and no
//! handler inspects content to select a decoder: the canonical artifact
//! formats are validated only after complete bounded streaming.

mod artifacts;
mod bytes;
mod grid;
mod package;
mod route;
mod session;

pub use artifacts::{
    MaterialPayload, StaticMeshPayload, TexturePayload, material_handler, static_mesh_handler,
    texture_handler,
};
pub use bytes::{ExactBytes, exact_bytes_handler};
pub use grid::{GridShape, U8Grid, U16LeGrid, u8_grid_handler, u16_le_grid_handler};
pub use package::{PackagePayload, package_handler};
pub use route::{CapabilityConfigError, ElementPolicy, ExactCapabilityRoute};
