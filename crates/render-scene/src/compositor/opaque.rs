//! CPU-only admission query for the opaque material experiment contract.
use super::RendererConfiguration;
use std::collections::BTreeSet;

use thiserror::Error;
use worldrunner_render_plan::{SceneAlphaMode, SceneOpaqueStorage};

use crate::draw::PreparedScene;

/// Opaque material families found in a prepared scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OpaqueMaterialFamily {
    /// Terrain surface material.
    Terrain,
    /// Fully covered mesh material.
    OpaqueMesh,
    /// Alpha-masked mesh material.
    MaskedMesh,
    /// Billboard/impostor material.
    Impostor,
}

/// Device and record requirements for one candidate layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueLayoutAdmission {
    /// Candidate storage representation.
    pub storage: SceneOpaqueStorage,
    /// Attachment formats in declaration order.
    pub formats: [wgpu::TextureFormat; 4],
    /// wgpu's colour attachment byte cost for one pixel sample.
    pub color_attachment_byte_cost: u32,
    /// Device's `max_color_attachment_bytes_per_sample` limit.
    pub byte_limit: u32,
    /// Number of color attachments required.
    pub attachment_count: u32,
    /// Device's `max_color_attachments` limit.
    pub attachment_limit: u32,
    /// Distinct material records referenced by the scene.
    pub record_count: u32,
    /// Material-record capacity from the layout's 16-bit ID attachment.
    pub record_limit: u32,
    /// Whether the device limits admit this candidate.
    pub admitted: bool,
}

/// CPU-only opaque-path census. It owns no GPU resources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueAdmissionReport {
    /// Material families referenced by opaque scene content.
    pub material_families: Vec<OpaqueMaterialFamily>,
    /// Limits for both candidate storage representations.
    pub layouts: [OpaqueLayoutAdmission; 2],
}

/// A typed reason an opaque-path request cannot be admitted.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum OpaqueAdmissionError {
    /// The initial opaque contract supports one sample only.
    #[error("opaque admission supports one sample, received {observed}")]
    UnsupportedSampleCount { observed: u32 },
    /// The referenced material table exceeds the declared record capacity.
    #[error("opaque path needs {records} material records, limit is {limit}")]
    RecordOverflow { records: u32, limit: u32 },
    /// A CPU count could not be represented by the report fields.
    #[error("opaque admission {part} exceeds the report's integer range")]
    CountOverflow { part: &'static str },
}

/// Reports the formats and capacities required by both layouts without allocation or encoding.
fn admission_report(
    prepared: &PreparedScene,
    limits: &wgpu::Limits,
    sample_count: u32,
) -> Result<OpaqueAdmissionReport, OpaqueAdmissionError> {
    validate_samples(sample_count)?;
    let (records, families) = census(prepared);
    let record_count =
        u32::try_from(records.len()).map_err(|_| OpaqueAdmissionError::CountOverflow {
            part: "material record count",
        })?;
    let wide_id = MaterialIdFormat::Rgba16Uint;
    let compact_id = MaterialIdFormat::R16Uint;
    validate_record_count(record_count, wide_id)?;
    validate_record_count(record_count, compact_id)?;
    let wide = layout(
        SceneOpaqueStorage::Wide,
        [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba16Float,
            wide_id.texture_format(),
        ],
        wide_id,
        limits,
        record_count,
    );
    let compact = layout(
        SceneOpaqueStorage::Compact,
        [
            wgpu::TextureFormat::Rgb10a2Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Rgba8Unorm,
            compact_id.texture_format(),
        ],
        compact_id,
        limits,
        record_count,
    );
    Ok(OpaqueAdmissionReport {
        material_families: families.into_iter().collect(),
        layouts: [wide, compact],
    })
}

impl RendererConfiguration {
    /// Explicitly queries opaque material admission without allocating renderer resources.
    pub fn opaque_admission_report(
        &self,
        prepared: &PreparedScene,
        limits: &wgpu::Limits,
    ) -> Result<OpaqueAdmissionReport, OpaqueAdmissionError> {
        admission_report(prepared, limits, self.frame.samples)
    }
}

fn validate_samples(sample_count: u32) -> Result<(), OpaqueAdmissionError> {
    if sample_count == 1 {
        Ok(())
    } else {
        Err(OpaqueAdmissionError::UnsupportedSampleCount {
            observed: sample_count,
        })
    }
}

#[derive(Clone, Copy)]
enum MaterialIdFormat {
    Rgba16Uint,
    R16Uint,
}

impl MaterialIdFormat {
    const fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba16Uint => wgpu::TextureFormat::Rgba16Uint,
            Self::R16Uint => wgpu::TextureFormat::R16Uint,
        }
    }

    const fn capacity(self) -> u32 {
        match self {
            Self::Rgba16Uint | Self::R16Uint => u16::MAX as u32 + 1,
        }
    }
}

fn validate_record_count(
    record_count: u32,
    id_format: MaterialIdFormat,
) -> Result<(), OpaqueAdmissionError> {
    let record_limit = id_format.capacity();
    if record_count <= record_limit {
        Ok(())
    } else {
        Err(OpaqueAdmissionError::RecordOverflow {
            records: record_count,
            limit: record_limit,
        })
    }
}

fn layout(
    storage: SceneOpaqueStorage,
    formats: [wgpu::TextureFormat; 4],
    id_format: MaterialIdFormat,
    limits: &wgpu::Limits,
    record_count: u32,
) -> OpaqueLayoutAdmission {
    let color_attachment_byte_cost = formats.iter().fold(0u32, |total, format| {
        let cost = format.target_pixel_byte_cost().unwrap();
        let alignment = format.target_component_alignment().unwrap();
        total.next_multiple_of(alignment) + cost
    });
    let byte_limit = limits.max_color_attachment_bytes_per_sample;
    let attachment_limit = limits.max_color_attachments;
    OpaqueLayoutAdmission {
        storage,
        formats,
        color_attachment_byte_cost,
        byte_limit,
        attachment_count: 4,
        attachment_limit,
        record_count,
        record_limit: id_format.capacity(),
        admitted: color_attachment_byte_cost <= byte_limit && 4 <= attachment_limit,
    }
}

fn census(prepared: &PreparedScene) -> (BTreeSet<u32>, BTreeSet<OpaqueMaterialFamily>) {
    let scene = prepared.scene();
    let mut records = BTreeSet::new();
    let mut families = BTreeSet::new();
    for object in scene.geometry().objects() {
        for lod in scene.geometry().meshes()[object.mesh().raw() as usize].lods() {
            if let Some(slots) = lod.material_slots() {
                for slot in slots {
                    let material = slot.material().raw();
                    records.insert(material);
                    families.insert(material_family(
                        &scene.resources().materials()[material as usize],
                    ));
                }
            } else {
                let material = object.material().raw();
                records.insert(material);
                families.insert(material_family(
                    &scene.resources().materials()[material as usize],
                ));
            }
        }
    }
    if let Some(terrain) = scene.terrain() {
        for surface in terrain.surfaces() {
            let material = surface.material().raw();
            records.insert(material);
            families.insert(OpaqueMaterialFamily::Terrain);
        }
    }
    (records, families)
}

fn material_family(material: &worldrunner_render_plan::SceneMaterial) -> OpaqueMaterialFamily {
    match (material.impostor(), material.alpha_mode()) {
        (Some(_), _) => OpaqueMaterialFamily::Impostor,
        (None, SceneAlphaMode::Opaque) => OpaqueMaterialFamily::OpaqueMesh,
        (None, SceneAlphaMode::Mask { .. }) => OpaqueMaterialFamily::MaskedMesh,
    }
}

#[cfg(test)]
#[path = "opaque_tests.rs"]
mod tests;
