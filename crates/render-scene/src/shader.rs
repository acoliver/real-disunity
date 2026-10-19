//! The environment shader and the names the pipelines bind it by.
//!
//! The source is embedded rather than loaded, which is what keeps this crate
//! pathless: a shader read from disk at run time would reintroduce exactly the
//! file resolution the render scene exists to avoid, and would let a build ship
//! without the shader it needs.

// shader/lighting.wgsl carries the frame uniform and the reflectance model and
// is prepended to both sources below. It is composed at compile time rather than
// duplicated into each because terrain and the props standing on it must resolve
// the same surface to the same colour under the same light; a divergence there
// would not read as a shading fault, it would read as terrain being the wrong
// material. concat! keeps this a compile-time constant, so nothing here can
// reach the filesystem.

/// WGSL source for governed environment geometry.
pub const ENVIRONMENT_WGSL: &str = concat!(
    include_str!("shader/lighting.wgsl"),
    include_str!("shader/unshadowed.wgsl"),
    include_str!("shader/environment.wgsl")
);

/// WGSL source for the per-fragment terrain splat blend.
pub const TERRAIN_WGSL: &str = concat!(
    include_str!("shader/lighting.wgsl"),
    include_str!("shader/unshadowed.wgsl"),
    include_str!("shader/terrain.wgsl")
);

/// Vertex entry point for the terrain pipeline.
pub const TERRAIN_VERTEX_ENTRY_POINT: &str = "vs_terrain";

/// Fragment entry point for the terrain pipeline.
pub const TERRAIN_FRAGMENT_ENTRY_POINT: &str = "fs_terrain";

/// Vertex entry point shared by every pipeline variant.
pub const VERTEX_ENTRY_POINT: &str = "vs_main";

/// Fragment entry point for materials that cover every fragment they touch.
pub const OPAQUE_ENTRY_POINT: &str = "fs_opaque";

/// Fragment entry point for materials that cut fragments out by alpha.
pub const MASKED_ENTRY_POINT: &str = "fs_masked";

/// Bit marking a material that supplies a normal map.
pub const FLAG_NORMAL: u32 = 1;

/// Bit marking a material that supplies a packed roughness/metalness map.
pub const FLAG_ROUGH_METAL: u32 = 2;

/// Bit marking a material that supplies an ambient occlusion map.
pub const FLAG_AMBIENT_OCCLUSION: u32 = 4;

/// Bit marking a material that supplies a self-illumination mask.
pub const FLAG_SELF_ILLUMINATION: u32 = 8;

/// Returns the fragment entry point a pipeline variant must bind.
#[must_use]
pub const fn fragment_entry_point(masked: bool) -> &'static str {
    if masked {
        MASKED_ENTRY_POINT
    } else {
        OPAQUE_ENTRY_POINT
    }
}

#[cfg(test)]
mod tests;
