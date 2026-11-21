//! Opaque-path admission: the renderer's admission of the selected path, and the CPU-only
//! census query for the opaque material experiment contract.
mod early;
pub use early::validate_receiver_reservation;
pub(crate) use early::{admit_early_producer, validate_hdr_admission};

use super::RendererConfiguration;
use std::collections::BTreeSet;

use thiserror::Error;
use worldrunner_render_plan::{SceneAlphaMode, SceneOpaquePath, SceneOpaqueStorage};
use worldrunner_render_post::{FrameConfig, FrameError, PostError};

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
    /// Distinct lighting records besides the neutral record 0 in the scene's material table:
    /// every material, referenced or not, as the renderer's record table holds them.
    pub record_count: u32,
    /// Lighting records besides record 0 that the layout's record index addresses: 255 for
    /// Compact's eight-bit G1.a index, 65,535 for Wide's 16-bit index.
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
    /// The scene's lighting records exceed what the layout's record index addresses.
    #[error("opaque path needs {records} material records, limit is {limit}")]
    RecordOverflow { records: u32, limit: u32 },
    /// The early Render producer cannot reserve an opposing receiver representation.
    #[error("opaque G-buffer Render does not support Opposing receiver reservations")]
    UnsupportedOpposingReceiver,
    /// Enabled device attachment bytes cannot hold the ordered producer outputs.
    #[error(
        "opaque G-buffer needs {required} color attachment bytes per sample, available {available}"
    )]
    ColorAttachmentBytes { required: u32, available: u32 },
    /// Enabled device attachment slots cannot hold the producer outputs.
    #[error("opaque G-buffer needs {required} color attachments, available {available}")]
    ColorAttachments { required: u32, available: u32 },
}

/// Reports the formats and capacities required by both layouts without allocation or encoding.
fn admission_report(
    prepared: &PreparedScene,
    limits: &wgpu::Limits,
    sample_count: u32,
) -> Result<OpaqueAdmissionReport, OpaqueAdmissionError> {
    validate_samples(sample_count)?;
    let families = census(prepared);
    let record_count = lighting_records(prepared).distinct();
    validate_record_count(record_count, RecordIndex::Wide)?;
    validate_record_count(record_count, RecordIndex::Compact)?;
    let wide = layout(
        SceneOpaqueStorage::Wide,
        [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba16Uint,
        ],
        RecordIndex::Wide,
        limits,
        record_count,
    );
    let compact = layout(
        SceneOpaqueStorage::Compact,
        crate::gbuffer::GBUFFER_FORMATS,
        RecordIndex::Compact,
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

/// Admits the selected opaque path for `config` and allocates what it needs: nothing for
/// Forward, the four G-buffer targets for the G-buffer paths, and the diagnostic lighting
/// targets for the observation. The G-buffer is single-sample, so a multisampled frame is
/// refused with the admission's sample error.
pub(crate) fn allocate_path(
    device: &wgpu::Device,
    path: SceneOpaquePath,
    config: FrameConfig,
    reflections: bool,
) -> Result<Option<crate::gbuffer::Gbuffer>, PostError> {
    admit_early_producer(
        path,
        config.samples,
        super::ReceiverGradientReservation::None,
        crate::gbuffer_writes::EarlyOutputs {
            velocity: false,
            reflections,
        },
        &device.limits(),
    )?;
    admit_path(path, config.samples).map(|usage| {
        usage.map(|usage| {
            crate::gbuffer::Gbuffer::new(device, config.size, (config.format, reflections), usage)
        })
    })
}

impl super::FrameRenderer {
    pub(crate) fn validate_opaque_depth(
        &self,
        depth: &worldrunner_render_post::AttachmentView,
    ) -> Result<(), FrameError> {
        if self
            .gbuffer
            .as_ref()
            .is_some_and(crate::gbuffer::Gbuffer::renders)
            && (depth.texture() != self.targets().depth().texture()
                || depth.resource().mip != 0
                || depth.resource().layer != 0)
        {
            return Err(FrameError::FeaturePreparation {
                feature: "opaque G-buffer",
                reason: "rendering from the G-buffer needs the frame depth target".to_owned(),
            });
        }
        Ok(())
    }

    /// Builds the scene's G-buffer pipelines when the frame uses the G-buffer and has the
    /// prepass metadata whose depth and screen camera they use. A screen that already has them,
    /// and a frame without a G-buffer or metadata, are left as they are.
    pub(crate) fn install_gbuffer_producer(&mut self, device: &wgpu::Device) {
        let Some(gbuffer) = &self.gbuffer else {
            return;
        };
        let outputs = crate::gbuffer_writes::EarlyOutputs {
            velocity: false,
            reflections: self.ssr.is_some(),
        };
        if let Some(screen) = self
            .ssao
            .as_mut()
            .filter(|screen| screen.geometry.gbuffer.is_none())
        {
            screen.geometry.gbuffer = Some(if gbuffer.renders() {
                crate::gbuffer_writes::early_pipelines(device, &screen.uniform, outputs)
            } else {
                crate::gbuffer_writes::pipelines(device, &screen.uniform, false)
            });
        }
    }

    /// When the frame renders from the G-buffer, rebuilds the producer pipelines if they do not
    /// match `velocity`, whether the frame's colour draws write the object-motion residual: the
    /// producers then draw the surfaces Opaque no longer draws, so they write it in its place.
    /// Observation keeps producers without velocity.
    pub(crate) fn match_producer_velocity(&mut self, device: &wgpu::Device, velocity: bool) {
        if !self
            .gbuffer
            .as_ref()
            .is_some_and(crate::gbuffer::Gbuffer::renders)
        {
            return;
        }
        let outputs = crate::gbuffer_writes::EarlyOutputs {
            velocity,
            reflections: self.ssr.is_some(),
        };
        if let Some(screen) = self.ssao.as_mut().filter(|screen| {
            screen
                .geometry
                .gbuffer
                .as_ref()
                .is_none_or(|(pipelines, _)| pipelines.early_outputs() != Some(outputs))
        }) {
            screen.geometry.gbuffer = Some(crate::gbuffer_writes::early_pipelines(
                device,
                &screen.uniform,
                outputs,
            ));
        }
    }

    /// Settles whether the frame's colour draws write the object-motion residual, from the
    /// pipelines they draw with, and makes the renderer and the scene agree with it: the G-buffer
    /// producer is rebuilt to match and the scene keeps previous-frame transforms while velocity
    /// is drawn. Returns the decision.
    ///
    /// The stock prefix plans the producer's attachments and the stock scene draws with it, so
    /// both take this decision, the prefix before it plans. Selecting a depth mode keeps whether
    /// pipelines write velocity, so the base `pipelines` and the ones a frame selects agree.
    pub(crate) fn adopt_scene_velocity(
        &mut self,
        device: &wgpu::Device,
        gpu: &mut crate::draw::SceneGpu,
        pipelines: &crate::renderer::ScenePipelines,
    ) -> bool {
        let writes_velocity = self.draws_velocity(pipelines);
        self.match_producer_velocity(device, writes_velocity);
        gpu.track_previous_transforms(writes_velocity);
        writes_velocity
    }
}

/// How `path` uses the G-buffer, if at all, refusing paths this renderer cannot draw.
pub(crate) fn admit_path(
    path: SceneOpaquePath,
    samples: u32,
) -> Result<Option<crate::gbuffer::GbufferUse>, PostError> {
    let usage = match path {
        SceneOpaquePath::Forward => return Ok(None),
        SceneOpaquePath::GBUFFER_OBSERVATION => crate::gbuffer::GbufferUse::Observe,
        SceneOpaquePath::GBUFFER_RENDER => crate::gbuffer::GbufferUse::Render,
        SceneOpaquePath::Experiment(_) => {
            return Err(PostError::UnsupportedProfile("opaque path experiment"));
        }
    };
    validate_samples(samples).map_err(PostError::from)?;
    Ok(Some(usage))
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

/// The lighting-record index of a layout: Wide's 16-bit lane in its fourth target, Compact's
/// eight-bit G1.a. Index 0 is the neutral record in both.
#[derive(Clone, Copy)]
enum RecordIndex {
    Wide,
    Compact,
}

impl RecordIndex {
    const fn capacity(self) -> u32 {
        match self {
            Self::Wide => u16::MAX as u32,
            Self::Compact => crate::gbuffer::records::LIGHTING_RECORD_LIMIT,
        }
    }
}

/// The lighting records of every material in the prepared scene, as the renderer's record
/// table builds them at upload.
fn lighting_records(prepared: &PreparedScene) -> crate::gbuffer::records::LightingRecordTable {
    let mut table = crate::gbuffer::records::LightingRecordTable::default();
    for material in prepared.scene().resources().materials() {
        table.assign(&mut crate::pipeline::MaterialUniform::from_material(material).to_bytes());
    }
    table
}

fn validate_record_count(
    record_count: u32,
    index: RecordIndex,
) -> Result<(), OpaqueAdmissionError> {
    let record_limit = index.capacity();
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
    index: RecordIndex,
    limits: &wgpu::Limits,
    record_count: u32,
) -> OpaqueLayoutAdmission {
    let color_attachment_byte_cost = early::color_attachment_byte_cost(&formats);
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
        record_limit: index.capacity(),
        admitted: color_attachment_byte_cost <= byte_limit && 4 <= attachment_limit,
    }
}

fn census(prepared: &PreparedScene) -> BTreeSet<OpaqueMaterialFamily> {
    let scene = prepared.scene();
    let mut families = BTreeSet::new();
    for object in scene.geometry().objects() {
        for lod in scene.geometry().meshes()[object.mesh().raw() as usize].lods() {
            if let Some(slots) = lod.material_slots() {
                for slot in slots {
                    let material = slot.material().raw();
                    families.insert(material_family(
                        &scene.resources().materials()[material as usize],
                    ));
                }
            } else {
                let material = object.material().raw();
                families.insert(material_family(
                    &scene.resources().materials()[material as usize],
                ));
            }
        }
    }
    if scene
        .terrain()
        .is_some_and(|terrain| !terrain.surfaces().is_empty())
    {
        families.insert(OpaqueMaterialFamily::Terrain);
    }
    families
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
