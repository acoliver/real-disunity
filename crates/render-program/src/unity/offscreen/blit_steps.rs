//! The steps of a blit and of a texture copy: a blit with a material waits for the constants
//! buffer, a plain blit and a copy are ready as they are planned.

use worldrunner_render_post::AttachmentLoad;
use worldrunner_render_post::contracts::{
    ColorAttachment, RasterId, TextureCopy, TypedOperation, TypedWork,
};

use crate::ids::{MaterialId, MeshId, TextureId};
use crate::layers::{Layers, PropertyLayer, TextureInput};
use crate::unity::shader::PassSelector;

use super::error::OffscreenError;
use super::frame::{OffscreenFrame, Record};
use super::plan::{DrawPlan, DrawSpec, Services, VertexPlan};
use super::resample::resample_step;
use super::steps::{Globals, Step, keywords, raster_id, require_render_texture};
use super::textures::TextureInfo;

/// A blit with a material, whose bindings wait for the constants buffer.
pub(super) struct BlitStep {
    pub(super) id: RasterId,
    pub(super) colors: Vec<Option<ColorAttachment>>,
    pub(super) plan: DrawPlan,
    pub(super) vertices: VertexPlan,
    pub(super) count: u32,
}

pub(super) fn blit_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    globals: Globals<'_>,
    (source, target): (TextureId, TextureId),
    (material, pass): (Option<MaterialId>, usize),
) -> Result<Step, OffscreenError> {
    require_render_texture(svc, target)?;
    let Some(material_id) = material else {
        return plain_blit(svc, index, source, target);
    };
    let material = frame
        .materials
        .get(&material_id)
        .ok_or(OffscreenError::UnknownMaterial(material_id))?;
    let shader = material
        .shader
        .ok_or(OffscreenError::NoShader(material_id))?;
    let mesh = frame.blit_mesh.ok_or(OffscreenError::NoBlitMesh)?;
    let overlay = source_layer(svc, frame, source)?;
    let layers = [
        &overlay,
        &material.properties,
        &material.fallback,
        globals.layer,
        &material.below_globals,
    ];
    let spec = DrawSpec {
        shader,
        keywords: keywords(material, globals),
        selector: PassSelector::Index(pass),
        layers: Layers::new(&layers),
        dots: None,
    };
    let plan = svc.plan_program(&spec)?;
    let vertices = svc.plan_vertices(mesh, &plan.entry)?;
    svc.counts.note_defaulted(&vertices.defaulted);
    svc.counts.note_stand_ins(&plan.stand_ins);
    svc.keep_unset(&plan);
    let count = blit_vertex_count(svc, mesh)?;
    let (imported, _) = svc.import(target)?;
    let color = imported
        .color
        .ok_or(OffscreenError::NoColorAttachment(target))?;
    let (before, after) = svc.tracker.write(color, 0, 0);
    let view = svc
        .tracker
        .cell_view(color, (0, 0), wgpu::TextureAspect::All, after)?;
    let colors = vec![Some(ColorAttachment {
        view,
        before: Some(before),
        load: AttachmentLoad::Load,
        store: wgpu::StoreOp::Store,
        resolve: None,
    })];
    Ok(Step::Blit(BlitStep {
        id: raster_id("offscreen blit", index),
        colors,
        plan,
        vertices,
        count,
    }))
}

/// The layer a blit adds above the material: the frame's blit values and the source under the main
/// texture name. Its texel size comes from the fill every draw gets (see the `texel_size` module).
fn source_layer(
    svc: &Services<'_>,
    frame: &OffscreenFrame,
    source: TextureId,
) -> Result<PropertyLayer, OffscreenError> {
    svc.textures.get(source)?;
    let mut layer = frame.blit_values.clone();
    layer.textures.insert(
        frame.main_texture.clone(),
        TextureInput {
            texture: source,
            sampler: None,
        },
    );
    Ok(layer)
}

fn blit_vertex_count(svc: &Services<'_>, mesh: MeshId) -> Result<u32, OffscreenError> {
    let info = svc.meshes.get(mesh)?;
    if info.index.is_some() || info.vertex_count % 3 != 0 {
        return Err(OffscreenError::BadMesh {
            mesh,
            reason: "a blit mesh is triangles with no index buffer".to_owned(),
        });
    }
    Ok(info.vertex_count)
}

/// A blit with no material copies the texture when the sizes and formats agree, and otherwise
/// draws the source over the target.
fn plain_blit(
    svc: &mut Services<'_>,
    index: usize,
    source: TextureId,
    target: TextureId,
) -> Result<Step, OffscreenError> {
    let (from, to) = (svc.textures.get(source)?, svc.textures.get(target)?);
    if from.size != to.size || from.format != to.format {
        return resample_step(svc, index, (source, target));
    }
    copy_cells(
        svc,
        raster_id("offscreen blit", index),
        (source, 0, 0),
        (target, 0, 0),
    )
    .map(Step::Ready)
}

pub(super) fn copy_step(
    svc: &mut Services<'_>,
    index: usize,
    record: &Record,
) -> Result<TypedOperation, OffscreenError> {
    let Record::CopyTexture {
        source,
        source_element,
        source_mip,
        target,
        target_element,
        target_mip,
    } = *record
    else {
        unreachable!("only copies are planned here");
    };
    require_render_texture(svc, target)?;
    copy_cells(
        svc,
        raster_id("offscreen copy", index),
        (source, source_mip, source_element),
        (target, target_mip, target_element),
    )
}

/// Copies one mip of one element to another, after checking that both exist and agree.
fn copy_cells(
    svc: &mut Services<'_>,
    id: RasterId,
    (source, source_mip, source_element): (TextureId, u32, u32),
    (target, target_mip, target_element): (TextureId, u32, u32),
) -> Result<TypedOperation, OffscreenError> {
    let (from, to) = (svc.textures.get(source)?, svc.textures.get(target)?);
    for (texture, info, mip, element) in [
        (source, from, source_mip, source_element),
        (target, to, target_mip, target_element),
    ] {
        if mip >= info.mips || element >= info.layers {
            return Err(OffscreenError::NoSuchSubresource {
                texture,
                mip,
                element,
            });
        }
    }
    check_copy((source, from, source_mip), (target, to, target_mip))?;
    let (source_imported, _) = svc.import(source)?;
    let (target_imported, _) = svc.import(target)?;
    let read = svc
        .tracker
        .cell(source_imported.primary, source_mip, source_element);
    let (_, written) = svc
        .tracker
        .write(target_imported.primary, target_mip, target_element);
    let aspect = wgpu::TextureAspect::All;
    let source_view = svc.tracker.cell_view(
        source_imported.primary,
        (source_mip, source_element),
        aspect,
        read,
    )?;
    let target_view = svc.tracker.cell_view(
        target_imported.primary,
        (target_mip, target_element),
        aspect,
        written,
    )?;
    Ok(TypedOperation::new(
        id,
        TypedWork::CopyTexture {
            source: TextureCopy::whole(source_view),
            destination: TextureCopy::whole(target_view),
        },
    ))
}

/// Checks that the two mips have the same size and the formats agree up to sRGB.
fn check_copy(
    (source, from, source_mip): (TextureId, &TextureInfo, u32),
    (target, to, target_mip): (TextureId, &TextureInfo, u32),
) -> Result<(), OffscreenError> {
    let mismatch = |reason: String| OffscreenError::CopyMismatch {
        from: source,
        to: target,
        reason,
    };
    let at = |size: [u32; 2], mip: u32| size.map(|side| (side >> mip).max(1));
    let sizes = (at(from.size, source_mip), at(to.size, target_mip));
    if sizes.0 != sizes.1 {
        return Err(mismatch(format!(
            "the sizes are {:?} and {:?}",
            sizes.0, sizes.1
        )));
    }
    if from.format.remove_srgb_suffix() != to.format.remove_srgb_suffix() {
        return Err(mismatch(format!(
            "the formats are {:?} and {:?}",
            from.format, to.format
        )));
    }
    Ok(())
}
