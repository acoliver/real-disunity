//! What decides the stock schedule, and which allocations its attachments come from.
//!
//! The schedule is a function of the settings the renderer was built from, of the extents and
//! formats of what it allocated, and of what the caller authored. [`Shape`] holds those so two
//! frames can be compared for the schedule they need. [`Identity`] holds the allocations
//! themselves, which decide only which textures the schedule's attachments are bound to.
use super::{Authored, Environment};
use crate::water::snapshots::Snapshots;
use worldrunner_render_plan::{SceneSsao, SceneSsr, SceneWaterBody};
use worldrunner_render_post::{AttachmentView, FrameConfig, PassSlot, ResourceRef};

/// The textures the schedule's attachments can come from, by the role each plays, in a fixed
/// order. A role is a label for diagnostics; the position in the list is what pairs a texture
/// with the one it replaces.
pub(super) type Owned<'a> = Vec<(&'static str, &'a wgpu::Texture)>;

/// Everything the renderer allocates up front that a pass of the stock schedule can attach.
///
/// Water receivers are selected by the frame slot, so the same logical resources bind to
/// different textures on different slots.
pub(super) fn owned<'a>(environment: &'a Environment<'_>) -> Owned<'a> {
    let mut owned: Owned<'a> = Vec::new();
    owned.extend(
        environment
            .velocity
            .as_ref()
            .map(|texture| ("velocity", texture)),
    );
    if let Some(screen) = environment.ssao {
        owned.extend(
            screen
                .passes
                .textures()
                .into_iter()
                .map(|texture| ("screen stage", texture)),
        );
        owned.extend([
            ("screen depth", &screen.depth),
            ("screen independent depth", &screen.independent_depth),
            ("screen normals", &screen.normals),
        ]);
        if let Some(receiver) = &screen.receiver {
            owned.extend(
                receiver
                    .textures
                    .iter()
                    .map(|texture| ("receiver payload", texture)),
            );
        }
    }
    if let Some(ssr) = environment.ssr {
        owned.extend(
            ssr.textures()
                .into_iter()
                .map(|texture| ("reflection", texture)),
        );
    }
    if let Some(water) = environment.water {
        owned.extend(
            water.receivers[environment.slot]
                .iter()
                .map(|texture| ("water receiver", texture)),
        );
    }
    owned
}

/// The snapshot bank of the frame slot, which the renderer allocates when a frame first needs
/// it. The bank is sized by the frame configuration, so it decides which textures the schedule
/// binds to and nothing about the schedule.
pub(super) fn snapshot_bank<'a>(
    environment: &Environment<'_>,
    snapshots: Option<&'a [Snapshots; 3]>,
) -> Vec<&'a wgpu::Texture> {
    snapshots.map_or_else(Vec::new, |snapshots| {
        let snapshot = &snapshots[environment.slot];
        let mut bank = vec![&snapshot.color, &snapshot.depth, &snapshot.final_depth];
        bank.extend(snapshot.device_depth.as_ref().map(|depth| &depth.texture));
        bank.extend(
            snapshot
                .final_device_depth
                .as_ref()
                .map(|depth| &depth.texture),
        );
        bank
    })
}

/// A texture's role, format and extent: what a schedule can depend on in an allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Descriptor {
    role: &'static str,
    format: wgpu::TextureFormat,
    extent: [u32; 3],
    mips: u32,
    samples: u32,
}

impl Descriptor {
    fn new(role: &'static str, texture: &wgpu::Texture) -> Self {
        Self {
            role,
            format: texture.format(),
            extent: [
                texture.width(),
                texture.height(),
                texture.depth_or_array_layers(),
            ],
            mips: texture.mip_level_count(),
            samples: texture.sample_count(),
        }
    }
}

/// The screen-space metadata's contribution to the schedule beyond its textures.
#[derive(Clone, Debug, PartialEq)]
struct ScreenShape {
    settings: SceneSsao,
    size: [u32; 2],
    depth_format: wgpu::TextureFormat,
    merged: bool,
    receiver: Option<Vec<wgpu::TextureFormat>>,
    computes_occlusion: bool,
    stages: Vec<(PassSlot, Vec<ResourceRef>)>,
}

impl ScreenShape {
    fn new(screen: &crate::scene_metadata::SceneMetadata) -> Self {
        Self {
            settings: screen.settings,
            size: screen.size,
            depth_format: screen.depth.format(),
            merged: screen.geometry.merged.is_some(),
            receiver: screen
                .receiver
                .as_ref()
                .map(|receiver| receiver.layout.formats()),
            computes_occlusion: screen.passes.computes_occlusion(),
            stages: screen.passes.schedule_shape(),
        }
    }
}

/// What the stock schedule is a function of, short of what the caller authored.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Shape {
    config: FrameConfig,
    ssao_requested: Option<SceneSsao>,
    ssr_requested: Option<SceneSsr>,
    water: Option<std::sync::Arc<[SceneWaterBody]>>,
    screen: Option<ScreenShape>,
    overlay_depth: bool,
    allocations: Vec<Descriptor>,
}

impl Shape {
    pub(super) fn new(environment: &Environment<'_>, owned: &Owned<'_>) -> Self {
        Self {
            config: environment.config,
            ssao_requested: environment.requested.0,
            ssr_requested: environment.requested.1,
            water: environment.water.map(|water| water.settings.clone()),
            screen: environment.ssao.map(ScreenShape::new),
            overlay_depth: environment.overlay_depth,
            allocations: owned
                .iter()
                .map(|(role, texture)| Descriptor::new(role, texture))
                .collect(),
        }
    }
}

/// A caller view's selected texture and permission to load its existing contents.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ViewIdentity {
    resource: ResourceRef,
    texture: wgpu::Texture,
    format: wgpu::TextureFormat,
    retained: bool,
}

impl ViewIdentity {
    pub(super) fn new(view: &AttachmentView) -> Self {
        Self {
            resource: view.resource(),
            texture: view.texture().clone(),
            format: view.format(),
            retained: view.retained(),
        }
    }
}

/// The allocations a binding table was resolved against: the frame slot, the caller's views and
/// every texture the renderer owns that an attachment can bind to.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Identity {
    slot: usize,
    supplied: Vec<ViewIdentity>,
    owned: Vec<wgpu::Texture>,
}

impl Identity {
    pub(super) fn new(
        environment: &Environment<'_>,
        authored: &Authored<'_>,
        owned: (&Owned<'_>, &[&wgpu::Texture]),
    ) -> Self {
        Self {
            slot: environment.slot,
            supplied: authored.views.iter().map(ViewIdentity::new).collect(),
            owned: owned
                .0
                .iter()
                .map(|(_, texture)| *texture)
                .chain(owned.1.iter().copied())
                .cloned()
                .collect(),
        }
    }
}

/// The views of a binding table as the allocations they select, for comparing two tables.
#[cfg(any(test, debug_assertions))]
pub(super) fn identities(views: &[AttachmentView]) -> Vec<ViewIdentity> {
    views.iter().map(ViewIdentity::new).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_shape_distinguishes_computed_occlusion_from_depth_only_metadata() {
        let base = ScreenShape {
            settings: crate::ssr::neutral_ao(),
            size: [64, 64],
            depth_format: wgpu::TextureFormat::Depth32Float,
            merged: false,
            receiver: None,
            computes_occlusion: true,
            stages: Vec::new(),
        };
        let mut depth_only = base.clone();
        depth_only.computes_occlusion = false;

        assert_eq!(base.stages, depth_only.stages);
        assert_ne!(base, depth_only);
    }
}
