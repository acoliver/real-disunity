//! Draw lists and clears: the records a scripted frame is made of.
//!
//! A list draws the items it names into the attachments it names, with the view values it carries
//! and the globals in force at its place in the frame, and keeps what the attachments hold. A clear
//! sets the parts of the attachments it names. Each is one raster pass.

use worldrunner_render_post::AttachmentLoad;
use worldrunner_render_post::contracts::{ColorAttachment, DepthAttachment, RasterId, RasterPass};

use crate::ids::TextureId;
use crate::packet::TargetSignature;

use super::ListDraws;
use super::error::OffscreenError;
use super::frame::{Attachments, Clear, DrawList, OffscreenFrame};
use super::plan::Services;
use super::steps::{
    CameraDraw, Globals, Look, Step, planned_draw, push_use, raster_id, require_render_texture,
};

/// What a pass does with its attachments before it draws: the color and the depth to clear to,
/// or `None` to keep what they hold.
pub(super) struct Loads {
    pub(super) color: Option<[f64; 4]>,
    pub(super) depth: Option<f32>,
}

/// The side of a mip of a texture of `size`.
fn size_at(size: [u32; 2], mip: u32) -> [u32; 2] {
    size.map(|side| (side >> mip).max(1))
}

/// The attachments a pass can use. A depth buffer under a color texture has one mip, and one of
/// another size than the color cannot go with it, so a pass whose depth does not fit is drawn
/// without it.
fn usable(svc: &Services<'_>, target: Attachments) -> Result<Attachments, OffscreenError> {
    for attachment in [target.color, target.depth].into_iter().flatten() {
        require_render_texture(svc, attachment.texture)?;
        if attachment.mip >= svc.textures.get(attachment.texture)?.mips {
            return Err(OffscreenError::NoSuchSubresource {
                texture: attachment.texture,
                mip: attachment.mip,
                element: 0,
            });
        }
    }
    let Some(depth) = target.depth else {
        return Ok(target);
    };
    let info = svc.textures.get(depth.texture)?;
    let mut fits = info.color.is_none() || depth.mip == 0;
    if let (true, Some(color)) = (fits, target.color) {
        let held = svc.textures.get(color.texture)?;
        fits =
            held.color.is_none() || size_at(info.size, depth.mip) == size_at(held.size, color.mip);
    }
    Ok(Attachments {
        depth: fits.then_some(depth),
        ..target
    })
}

/// The formats of a pass over `target`.
fn signature(svc: &Services<'_>, target: Attachments) -> Result<TargetSignature, OffscreenError> {
    let color = match target.color {
        Some(attachment) => {
            let info = svc.textures.get(attachment.texture)?;
            info.color.is_some().then_some(info.format)
        }
        None => None,
    };
    let depth = match target.depth {
        Some(attachment) => svc.textures.get(attachment.texture)?.depth_format,
        None => None,
    };
    Ok(match (color, depth) {
        (Some(color), depth) => TargetSignature::new(&[color], depth, 1)?,
        (None, Some(depth)) => TargetSignature::depth_only(depth, 1)?,
        (None, None) => {
            return Err(OffscreenError::Unknown(
                "a color or depth buffer to draw into".to_owned(),
            ));
        }
    })
}

/// The texture a list's counts are kept under: its color, or its depth when it has no color.
fn primary(target: Attachments) -> Option<TextureId> {
    target
        .color
        .or(target.depth)
        .map(|attachment| attachment.texture)
}

/// The raster pass over `target`: it clears the parts `loads` names and loads the rest, and reads
/// every texture its draws sample.
pub(super) fn attachments_pass(
    svc: &mut Services<'_>,
    id: RasterId,
    target: Attachments,
    loads: &Loads,
    draws: &[CameraDraw],
) -> Result<RasterPass, OffscreenError> {
    let mut uses = Vec::new();
    for draw in draws {
        for usage in &draw.plan.texture_uses {
            push_use(&mut uses, usage.clone());
        }
    }
    let color = color_attachment(svc, target, loads.color)?;
    let depth = depth_attachment(svc, target, loads.depth)?;
    Ok(RasterPass {
        id,
        colors: color.into_iter().map(Some).collect(),
        depth,
        uses,
        bindings: Vec::new(),
    })
}

fn color_attachment(
    svc: &mut Services<'_>,
    target: Attachments,
    clear: Option<[f64; 4]>,
) -> Result<Option<ColorAttachment>, OffscreenError> {
    let Some(attachment) = target.color else {
        return Ok(None);
    };
    let (imported, _) = svc.import(attachment.texture)?;
    let Some(key) = imported.color else {
        return Ok(None);
    };
    let (before, after) = svc.tracker.write(key, attachment.mip, 0);
    let view = svc
        .tracker
        .cell_view(key, (attachment.mip, 0), wgpu::TextureAspect::All, after)?;
    Ok(Some(ColorAttachment {
        view,
        before: clear.is_none().then_some(before),
        load: clear.map_or(AttachmentLoad::Load, |c| {
            AttachmentLoad::ClearColor(wgpu::Color {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            })
        }),
        store: wgpu::StoreOp::Store,
        resolve: None,
    }))
}

fn depth_attachment(
    svc: &mut Services<'_>,
    target: Attachments,
    clear: Option<f32>,
) -> Result<Option<DepthAttachment>, OffscreenError> {
    let Some(attachment) = target.depth else {
        return Ok(None);
    };
    let (imported, _) = svc.import(attachment.texture)?;
    let Some(key) = imported.depth else {
        return Ok(None);
    };
    // A depth buffer under a color texture is the one mip `usable` kept it for.
    let mip = if imported.color.is_some() {
        0
    } else {
        attachment.mip
    };
    let (before, after) = svc.tracker.write(key, mip, 0);
    let view = svc
        .tracker
        .cell_view(key, (mip, 0), wgpu::TextureAspect::All, after)?;
    Ok(Some(DepthAttachment {
        view,
        before: clear.is_none().then_some(before),
        load: clear.map_or(AttachmentLoad::Load, AttachmentLoad::ClearDepth),
        store: wgpu::StoreOp::Store,
        read_only: false,
    }))
}

/// A clear of the parts of the attachments it names, or nothing when none of them has the part.
pub(super) fn clear_step(
    svc: &mut Services<'_>,
    index: usize,
    target: Attachments,
    clear: &Clear,
) -> Result<Option<Step>, OffscreenError> {
    let target = usable(svc, target)?;
    let has_color = match target.color {
        Some(attachment) => svc.textures.get(attachment.texture)?.color.is_some(),
        None => false,
    };
    let has_depth = match target.depth {
        Some(attachment) => svc.textures.get(attachment.texture)?.depth_format.is_some(),
        None => false,
    };
    let loads = Loads {
        color: clear.color.filter(|_| has_color),
        depth: clear.depth.filter(|_| has_depth),
    };
    if loads.color.is_none() && loads.depth.is_none() {
        return Ok(None);
    }
    let pass = attachments_pass(
        svc,
        raster_id("offscreen clear", index),
        target,
        &loads,
        &[],
    )?;
    Ok(Some(Step::Camera {
        id: pass.id,
        pass,
        draws: Vec::new(),
    }))
}

/// A list: the draws the items plan to, in order, into the attachments, loading what they hold.
pub(super) fn list_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    globals: Globals<'_>,
    list: &DrawList,
) -> Result<Step, OffscreenError> {
    let target = usable(svc, list.target)?;
    let signature = signature(svc, target)?;
    let (unmatched, missing_texture, missing_channel) = (
        svc.counts.unmatched,
        svc.counts.missing_texture,
        svc.counts.missing_channel,
    );
    let mut draws = Vec::new();
    for item in &list.draws {
        let renderer = frame
            .renderers
            .get(&item.renderer)
            .ok_or_else(|| OffscreenError::Unknown(format!("renderer {:?}", item.renderer)))?;
        let Some(material) = item
            .material
            .or_else(|| renderer.materials.get(item.slot).copied())
        else {
            continue;
        };
        let look = Look {
            selector: &item.pass,
            values: &list.values,
            front_face: list.front_face,
            globals,
            signature,
        };
        if let Some(draw) = planned_draw(svc, frame, &look, (material, item.renderer, item.slot))? {
            draws.push(draw);
        }
    }
    svc.counts.current = super::CameraCauses::default();
    if let Some(texture) = primary(target) {
        svc.counts.lists.push(ListDraws {
            target: texture,
            drawn: draws.len(),
            unmatched: svc.counts.unmatched - unmatched,
            missing_texture: svc.counts.missing_texture - missing_texture,
            missing_channel: svc.counts.missing_channel - missing_channel,
        });
    }
    let loads = Loads {
        color: None,
        depth: None,
    };
    let pass = attachments_pass(
        svc,
        raster_id("offscreen list", index),
        target,
        &loads,
        &draws,
    )?;
    Ok(Step::Camera {
        id: pass.id,
        pass,
        draws,
    })
}
