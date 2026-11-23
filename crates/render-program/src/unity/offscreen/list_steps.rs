//! Draw lists and clears: the records a scripted frame is made of.
//!
//! A list draws the items it names into the attachments it names, with the view values it carries
//! and the globals in force at its place in the frame, and keeps what the attachments hold. A clear
//! sets the parts of the attachments it names. Each is one raster pass.

use worldrunner_render_post::AttachmentLoad;
use worldrunner_render_post::contracts::{
    AllocationKey, ColorAttachment, DepthAttachment, RasterId, RasterPass, TextureRange,
};

use crate::ids::{MaterialId, RendererId, TextureId};
use crate::packet::{MAX_COLOR_ATTACHMENTS, TargetSignature};

use super::ListDraws;
use super::depth_range;
use super::dots;
use super::error::OffscreenError;
use super::frame::{Attachment, Attachments, Clear, DrawList, Instances, OffscreenFrame};
use super::plan::Services;
use super::raster::{self, PassRaster};
use super::steps::{
    CameraDraw, Globals, Look, Step, planned_draw, push_use, raster_id, require_render_texture,
};
use super::tracker::Imported;

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

/// The attachments a pass can use. Several colors must each have color storage and one size at
/// their mip, and none may repeat; a depth buffer under a color texture has one mip, and one of
/// another size than the color cannot go with it, so a pass whose depth does not fit is drawn
/// without it.
pub(super) fn usable(
    svc: &Services<'_>,
    target: Attachments,
) -> Result<Attachments, OffscreenError> {
    if target.colors.len() > MAX_COLOR_ATTACHMENTS {
        return Err(OffscreenError::ColorTargets {
            count: target.colors.len(),
        });
    }
    for attachment in target.colors.iter().chain(&target.depth) {
        require_render_texture(svc, attachment.texture)?;
        if attachment.mip >= svc.textures.get(attachment.texture)?.mips {
            return Err(OffscreenError::NoSuchSubresource {
                texture: attachment.texture,
                mip: attachment.mip,
                element: 0,
            });
        }
    }
    check_colors(svc, &target.colors)?;
    let Some(depth) = target.depth else {
        return Ok(target);
    };
    let info = svc.textures.get(depth.texture)?;
    let mut fits = info.color.is_none() || depth.mip == 0;
    if let (true, Some(color)) = (fits, target.colors.first()) {
        let held = svc.textures.get(color.texture)?;
        fits =
            held.color.is_none() || size_at(info.size, depth.mip) == size_at(held.size, color.mip);
    }
    Ok(Attachments {
        depth: fits.then_some(depth),
        ..target
    })
}

/// Checks that the colors of a pass can be drawn into together: with two or more, each has color
/// storage and the size the first has at its mip, and none is a mip of a texture another is.
fn check_colors(svc: &Services<'_>, colors: &[Attachment]) -> Result<(), OffscreenError> {
    for (index, attachment) in colors.iter().enumerate() {
        if colors[..index].contains(attachment) {
            return Err(OffscreenError::ColorTargetRepeated {
                texture: attachment.texture,
                mip: attachment.mip,
            });
        }
    }
    let [first, _, ..] = colors else {
        return Ok(());
    };
    let first_size = size_at(svc.textures.get(first.texture)?.size, first.mip);
    for attachment in colors {
        let info = svc.textures.get(attachment.texture)?;
        if info.color.is_none() {
            return Err(OffscreenError::ColorTargetWithoutColor(attachment.texture));
        }
        let size = size_at(info.size, attachment.mip);
        if size != first_size {
            return Err(OffscreenError::ColorTargetsDiffer {
                first: first.texture,
                first_size,
                texture: attachment.texture,
                size,
            });
        }
    }
    Ok(())
}

/// The formats of a pass over `target`.
pub(super) fn signature(
    svc: &Services<'_>,
    target: &Attachments,
) -> Result<TargetSignature, OffscreenError> {
    let mut colors = Vec::new();
    for attachment in &target.colors {
        let info = svc.textures.get(attachment.texture)?;
        if info.color.is_some() {
            colors.push(info.format);
        }
    }
    let depth = match target.depth {
        Some(attachment) => svc.textures.get(attachment.texture)?.depth_format,
        None => None,
    };
    Ok(match (colors.as_slice(), depth) {
        ([], Some(depth)) => TargetSignature::depth_attachment(depth, 1)?,
        ([], None) => {
            return Err(OffscreenError::Unknown(
                "a color or depth buffer to draw into".to_owned(),
            ));
        }
        (colors, depth) => TargetSignature::attachments(colors, depth, 1)?,
    })
}

/// A part of a texture a pass writes: the texture, the storage it is in and the levels and
/// layers.
pub(super) struct Written {
    texture: TextureId,
    key: AllocationKey,
    range: TextureRange,
}

/// The mip a depth attachment writes: a depth buffer under a color texture is the one mip
/// `usable` kept it for.
fn depth_mip(imported: Imported, attachment: Attachment) -> u32 {
    if imported.color.is_some() {
        0
    } else {
        attachment.mip
    }
}

/// What a pass over `target` writes, as `attachments_pass` writes it.
pub(super) fn written(
    svc: &mut Services<'_>,
    target: &Attachments,
) -> Result<Vec<Written>, OffscreenError> {
    let cell = |texture, key, mip| Written {
        texture,
        key,
        range: TextureRange::single(mip, 0, wgpu::TextureAspect::All),
    };
    let mut cells = Vec::new();
    for attachment in &target.colors {
        let (imported, _) = svc.import(attachment.texture)?;
        if let Some(key) = imported.color {
            cells.push(cell(attachment.texture, key, attachment.mip));
        }
    }
    if let Some(attachment) = target.depth {
        let (imported, _) = svc.import(attachment.texture)?;
        if let Some(key) = imported.depth {
            let mip = depth_mip(imported, attachment);
            cells.push(cell(attachment.texture, key, mip));
        }
    }
    Ok(cells)
}

/// `draw`, or the refusal naming the texture when it samples levels of a texture that its pass
/// writes. The contract refuses such a pass whole, so the draw is left out where it is planned.
pub(super) fn refuse_feedback(
    draw: CameraDraw,
    written: &[Written],
) -> Result<CameraDraw, OffscreenError> {
    for usage in &draw.plan.texture_uses {
        let view = &usage.view;
        let looped = written
            .iter()
            .find(|cell| cell.key == view.allocation() && cell.range.overlaps(view.range()));
        if let Some(cell) = looped {
            return Err(OffscreenError::FeedbackLoop(cell.texture));
        }
    }
    Ok(draw)
}

/// The texture a list's counts are kept under: its color, or its depth when it has no color.
fn primary(target: &Attachments) -> Option<TextureId> {
    target
        .colors
        .first()
        .or(target.depth.as_ref())
        .map(|attachment| attachment.texture)
}

/// The raster pass over `target`: it clears the parts `loads` names and loads the rest, and reads
/// every texture its draws sample.
pub(super) fn attachments_pass(
    svc: &mut Services<'_>,
    id: RasterId,
    target: &Attachments,
    loads: &Loads,
    draws: &[CameraDraw],
) -> Result<RasterPass, OffscreenError> {
    let mut uses = Vec::new();
    for draw in draws {
        for usage in &draw.plan.texture_uses {
            push_use(&mut uses, usage.clone());
        }
    }
    // Each color is written on its own, so it takes the next version of its own storage and a
    // later pass reads it as that write left it.
    let mut colors = Vec::new();
    for attachment in &target.colors {
        colors.extend(color_attachment(svc, *attachment, loads.color)?);
    }
    let depth = depth_attachment(svc, target, loads.depth)?;
    Ok(RasterPass {
        id,
        colors: colors.into_iter().map(Some).collect(),
        depth,
        uses,
        bindings: Vec::new(),
    })
}

fn color_attachment(
    svc: &mut Services<'_>,
    attachment: Attachment,
    clear: Option<[f64; 4]>,
) -> Result<Option<ColorAttachment>, OffscreenError> {
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
    target: &Attachments,
    clear: Option<f32>,
) -> Result<Option<DepthAttachment>, OffscreenError> {
    let Some(attachment) = target.depth else {
        return Ok(None);
    };
    let (imported, _) = svc.import(attachment.texture)?;
    let Some(key) = imported.depth else {
        return Ok(None);
    };
    let mip = depth_mip(imported, attachment);
    let (before, after) = svc.tracker.write(key, mip, 0);
    let view = svc
        .tracker
        .cell_view(key, (mip, 0), wgpu::TextureAspect::All, after)?;
    Ok(Some(DepthAttachment {
        view,
        before: clear.is_none().then_some(before),
        load: clear.map_or(AttachmentLoad::Load, |depth| {
            AttachmentLoad::ClearDepth(depth_range::depth_clear(depth))
        }),
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
    let mut has_color = false;
    for attachment in &target.colors {
        has_color |= svc.textures.get(attachment.texture)?.color.is_some();
    }
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
        &target,
        &loads,
        &[],
    )?;
    Ok(Some(Step::Camera {
        id: pass.id,
        pass,
        draws: Vec::new(),
    }))
}

/// The size in texels of the attachments of `target`: of the first color at its mip, or of the
/// depth when it has no color.
pub(super) fn target_size(
    svc: &Services<'_>,
    target: &Attachments,
) -> Result<[u32; 2], OffscreenError> {
    let attachment = target
        .colors
        .first()
        .or(target.depth.as_ref())
        .ok_or_else(|| {
            OffscreenError::Unknown("a color or depth buffer to draw into".to_owned())
        })?;
    Ok(size_at(
        svc.textures.get(attachment.texture)?.size,
        attachment.mip,
    ))
}

/// The draws the items of `list` plan to, in order, each with the raster state `raster` and the
/// formats of `signature`.
fn list_draws(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    globals: Globals<'_>,
    list: &DrawList,
    (signature, raster): (TargetSignature, PassRaster),
    cells: &[Written],
) -> Result<Vec<CameraDraw>, OffscreenError> {
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
            own: Some(&item.per_draw),
            front_face: list.front_face,
            globals,
            signature,
            written: cells,
            dots: None,
        };
        let ids = (material, item.renderer, item.slot);
        let mut planned = item_draws(svc, frame, &look, ids, &item.instances)?;
        for draw in &mut planned {
            draw.raster = raster;
        }
        draws.extend(planned);
    }
    Ok(draws)
}

/// The draws one list item plans to: one for a count of instances, and none for a count of 0.
fn item_draws(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    look: &Look<'_>,
    ids: (MaterialId, RendererId, usize),
    instances: &Instances,
) -> Result<Vec<CameraDraw>, OffscreenError> {
    match instances {
        Instances::Count(0) => Ok(Vec::new()),
        Instances::Count(count) => {
            let planned = planned_draw(svc, frame, look, ids)?;
            Ok(planned
                .into_iter()
                .map(|mut draw| {
                    draw.geometry.instances = 0..*count;
                    draw
                })
                .collect())
        }
        Instances::Dots(batch) => dots::draws(svc, frame, look, ids, batch),
    }
}

/// A list: the draws the items plan to, in order, into the attachments, loading what they hold. A
/// scissor with no texel in it leaves nothing to draw.
pub(super) fn list_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    globals: Globals<'_>,
    list: &DrawList,
) -> Result<Step, OffscreenError> {
    let target = usable(svc, list.target.clone())?;
    let signature = signature(svc, &target)?;
    let size = target_size(svc, &target)?;
    let cells = written(svc, &target)?;
    let (unmatched, missing_texture, missing_channel, feedback_loop) = (
        svc.counts.unmatched,
        svc.counts.missing_texture,
        svc.counts.missing_channel,
        svc.counts.feedback_loop,
    );
    let draws = match raster::resolve(&list.raster, size) {
        Some(raster) => list_draws(svc, frame, globals, list, (signature, raster), &cells)?,
        None => Vec::new(),
    };
    svc.counts.current = super::CameraCauses::default();
    if let Some(texture) = primary(&target) {
        svc.counts.lists.push(ListDraws {
            target: texture,
            drawn: draws.len(),
            unmatched: svc.counts.unmatched - unmatched,
            missing_texture: svc.counts.missing_texture - missing_texture,
            missing_channel: svc.counts.missing_channel - missing_channel,
            feedback_loop: svc.counts.feedback_loop - feedback_loop,
        });
    }
    let loads = Loads {
        color: None,
        depth: None,
    };
    let pass = attachments_pass(
        svc,
        raster_id("offscreen list", index),
        &target,
        &loads,
        &draws,
    )?;
    Ok(Step::Camera {
        id: pass.id,
        pass,
        draws,
    })
}
