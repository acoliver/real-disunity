//! Uniform state the terrain splat shader reads once per fragment.
//!
//! The mesh path picks one material per quad and bakes it into the draw. The
//! splat path cannot: a fragment discovers which surfaces it must blend by
//! reading the type grid, so every surface has to be addressable from the
//! shader at the same time. This module is the table that makes that possible.
//!
//! # Surfaces are deduplicated here and only here
//!
//! [`SceneTerrain`] deliberately refuses to collapse two surfaces that share a
//! material, because appearance is the only thing telling them apart and
//! collapsing them destroys it. A texture array layer is the opposite case:
//! two surfaces sharing a material sample identical texels, and giving them
//! separate layers would upload the same image twice for no gain.
//!
//! Both statements hold at once because they are about different things. The
//! surface list keys on what is drawn; the layer list keys on what is sampled.
//! This module holds the mapping between them, so a surface keeps its identity
//! while its texture data is shared.
//!
//! # Rotation is resolved on the CPU
//!
//! Each surface stores the cosine and sine of its rotation rather than the
//! angle. The rotation is per-surface uniform data, so recomputing the trig per
//! fragment would repeat identical work millions of times per frame. It also
//! puts the arcminute-to-radian conversion in Rust, where a test can reach it,
//! rather than in shader code where none can.
//!
//! # Byte packing rather than a cast
//!
//! The block is written out field by field, as [`crate::frame`] does, instead
//! of reinterpreting a struct. This crate depends on nothing that can name a
//! file, and that short dependency list is what makes "pathless" a checked
//! property rather than a claim; a plain-old-data cast would buy a little
//! convenience at the cost of widening it.

use thiserror::Error;
use worldrunner_render_plan::{MaterialHandle, SceneTerrain, SceneTerrainSurface};

use crate::geometry::millimeter_to_meter;
use crate::terrain::TERRAIN_METERS_PER_TILE;

/// Surfaces one terrain uniform can address.
///
/// A surface index is stored in the type grid as one byte, so it cannot name
/// more than this many surfaces however the scene was composed.
pub const MAX_TERRAIN_SURFACES: usize = 256;

/// Size in bytes of one encoded surface: two four-component vectors.
pub const TERRAIN_SURFACE_BYTES: usize = 32;

/// Size in bytes of the grid header preceding the surface table.
pub const TERRAIN_HEADER_BYTES: usize = 32;

/// Size in bytes of the whole terrain uniform block.
pub const TERRAIN_UNIFORM_BYTES: usize =
    TERRAIN_HEADER_BYTES + TERRAIN_SURFACE_BYTES * MAX_TERRAIN_SURFACES;

/// Arcminutes in one full turn, the unit authored rotations arrive in.
const ARCMINUTES_PER_TURN: f32 = 360.0 * 60.0;

/// Tint value that leaves a sampled colour unchanged, in authored percent.
const NEUTRAL_TINT_PERCENT: f32 = 100.0;

/// Refusals encountered while encoding terrain uniform state.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TerrainUniformError {
    /// The scene carried more surfaces than a surface index can address.
    #[error("terrain declares {count} surfaces but only {MAX_TERRAIN_SURFACES} are addressable")]
    TooManySurfaces {
        /// Surfaces the scene declared.
        count: usize,
    },
}

/// Grid geometry and surface table for one terrain, ready to upload.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainUniformState {
    bytes: [u8; TERRAIN_UNIFORM_BYTES],
    layer_materials: Vec<MaterialHandle>,
    surface_count: usize,
}

impl TerrainUniformState {
    /// Encodes one terrain's grid geometry and surface table.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainUniformError::TooManySurfaces`] when the terrain
    /// declares more surfaces than a one-byte surface index can address.
    pub fn encode(terrain: &SceneTerrain) -> Result<Self, TerrainUniformError> {
        validate_source(terrain)?;
        let surfaces = terrain.surfaces();

        let layer_materials = distinct_materials(surfaces);
        let spacing_millimeters = terrain.sample_spacing_millimeters();
        let mut bytes = [0u8; TERRAIN_UNIFORM_BYTES];
        let mut offset = 0usize;

        write_f32(
            &mut bytes,
            &mut offset,
            millimeter_to_meter(terrain.origin_east_millimeters()),
        );
        write_f32(
            &mut bytes,
            &mut offset,
            millimeter_to_meter(terrain.origin_north_millimeters()),
        );
        write_f32(
            &mut bytes,
            &mut offset,
            millimeter_to_meter(i64::from(spacing_millimeters)),
        );
        write_f32(&mut bytes, &mut offset, 1.0 / TERRAIN_METERS_PER_TILE);
        write_u32(&mut bytes, &mut offset, terrain.columns());
        write_u32(&mut bytes, &mut offset, terrain.rows());
        write_u32(
            &mut bytes,
            &mut offset,
            u32::try_from(surfaces.len()).expect("surface count is bounded by the table size"),
        );
        write_u32(
            &mut bytes,
            &mut offset,
            u32::from(terrain.layered_source().is_some()),
        );
        debug_assert_eq!(
            offset, TERRAIN_HEADER_BYTES,
            "the header is written in full"
        );

        for index in 0..MAX_TERRAIN_SURFACES {
            match surfaces.get(index) {
                Some(surface) => {
                    let layer = layer_materials
                        .iter()
                        .position(|material| *material == surface.material())
                        .expect("every surface material was collected into the layer list");
                    write_surface(&mut bytes, &mut offset, surface, layer, spacing_millimeters);
                }
                // Slots past the surface count are never addressed, because the
                // type grid only holds indices composition emitted. They are
                // written as an untinted, unrotated, layer-nil surface so the
                // padding is deterministic rather than whatever memory held.
                None => write_unused_surface(&mut bytes, &mut offset),
            }
        }
        debug_assert_eq!(
            offset, TERRAIN_UNIFORM_BYTES,
            "the block is written in full"
        );

        Ok(Self {
            bytes,
            layer_materials,
            surface_count: surfaces.len(),
        })
    }

    /// Returns the encoded block as the bytes a buffer write expects.
    #[must_use]
    pub const fn bytes(&self) -> &[u8; TERRAIN_UNIFORM_BYTES] {
        &self.bytes
    }

    /// Returns the number of surfaces the type grid may address.
    #[must_use]
    pub const fn surface_count(&self) -> usize {
        self.surface_count
    }

    /// Returns the materials backing each palette array layer, in layer order.
    ///
    /// The texture arrays must be built from exactly this sequence: a surface's
    /// encoded layer is a position in it, so any other order silently draws the
    /// terrain with another material's texels.
    #[must_use]
    pub fn layer_materials(&self) -> &[MaterialHandle] {
        &self.layer_materials
    }
}

/// Collects the distinct materials the surfaces name, in first-use order.
///
/// The scan is linear because the layer list is bounded by the surface table
/// and is built once per terrain; a set would cost more to allocate than it
/// saves, and would have to be ordered afterwards regardless.
fn distinct_materials(surfaces: &[SceneTerrainSurface]) -> Vec<MaterialHandle> {
    let mut materials: Vec<MaterialHandle> = Vec::new();
    for surface in surfaces {
        if !materials.contains(&surface.material()) {
            materials.push(surface.material());
        }
    }
    materials
}

/// Writes one surface against the palette layer its material occupies.
fn write_surface(
    bytes: &mut [u8; TERRAIN_UNIFORM_BYTES],
    offset: &mut usize,
    surface: &SceneTerrainSurface,
    layer: usize,
    spacing_millimeters: u32,
) {
    let appearance = surface.appearance();
    let tint = appearance.tint_percent;
    write_f32(bytes, offset, f32::from(tint[0]) / NEUTRAL_TINT_PERCENT);
    write_f32(bytes, offset, f32::from(tint[1]) / NEUTRAL_TINT_PERCENT);
    write_f32(bytes, offset, f32::from(tint[2]) / NEUTRAL_TINT_PERCENT);
    write_f32(bytes, offset, blur_texels(&appearance, spacing_millimeters));

    // The angle is reduced modulo a turn before the trig so that the conversion
    // stays exact for the arcminute values authoring actually uses.
    let turns =
        f32_from_u32(appearance.rotation_arcminutes % arcminutes_per_turn()) / ARCMINUTES_PER_TURN;
    let radians = turns * std::f32::consts::TAU;
    write_f32(bytes, offset, radians.cos());
    write_f32(bytes, offset, radians.sin());
    write_f32(bytes, offset, f32_from_usize(layer));
    write_f32(bytes, offset, 0.0);
}

/// Writes the surface an unaddressed slot describes.
fn write_unused_surface(bytes: &mut [u8; TERRAIN_UNIFORM_BYTES], offset: &mut usize) {
    for value in [1.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        write_f32(bytes, offset, value);
    }
}

/// Converts an authored blur radius into grid texels.
///
/// The radius is authored as a world distance so that it means the same thing
/// whatever resolution the grid is sampled at; the shader walks the grid in
/// texels, so the division happens once here rather than per fragment.
fn blur_texels(
    appearance: &worldrunner_render_plan::SceneTerrainAppearance,
    spacing_millimeters: u32,
) -> f32 {
    // Scene validation rejects a zero spacing, so this cannot divide by zero.
    debug_assert_ne!(
        spacing_millimeters, 0,
        "scene validation rejects zero spacing"
    );
    f32_from_u32(appearance.blur_radius_millimeters) / f32_from_u32(spacing_millimeters)
}

/// Arcminutes in one full turn, as the integer the authored unit uses.
const fn arcminutes_per_turn() -> u32 {
    360 * 60
}

/// Widens one authored magnitude to the precision the shader reads it at.
#[expect(
    clippy::cast_precision_loss,
    reason = "authored blur radii, spacings and arcminutes are bounded far below the magnitude where f32 stops counting integers exactly"
)]
fn f32_from_u32(value: u32) -> f32 {
    value as f32
}

/// Widens one palette layer index.
///
/// The index cannot exceed [`MAX_TERRAIN_SURFACES`], so the narrowing never
/// discards a real layer; the saturating branch is unreachable and exists only
/// so this conversion has no panic in it.
fn f32_from_usize(value: usize) -> f32 {
    u16::try_from(value).map_or(f32::from(u16::MAX), f32::from)
}

/// Writes one float at the running offset.
fn write_f32(bytes: &mut [u8; TERRAIN_UNIFORM_BYTES], offset: &mut usize, value: f32) {
    bytes[*offset..*offset + 4].copy_from_slice(&value.to_le_bytes());
    *offset += 4;
}

/// Writes one unsigned integer at the running offset.
fn write_u32(bytes: &mut [u8; TERRAIN_UNIFORM_BYTES], offset: &mut usize, value: u32) {
    bytes[*offset..*offset + 4].copy_from_slice(&value.to_le_bytes());
    *offset += 4;
}

#[cfg(test)]
mod tests;

fn validate_source(terrain: &SceneTerrain) -> Result<(), TerrainUniformError> {
    let surfaces = terrain.surfaces();
    if surfaces.len() > MAX_TERRAIN_SURFACES {
        return Err(TerrainUniformError::TooManySurfaces {
            count: surfaces.len(),
        });
    }

    Ok(())
}
