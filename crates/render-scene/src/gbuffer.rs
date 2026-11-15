//! The compact deferred G-buffer selected by `SceneOpaquePath::GBUFFER_OBSERVATION` and
//! `SceneOpaquePath::GBUFFER_RENDER`.
//!
//! The renderer owns four single-sample targets at the scene extent and clears them in
//! the [`PassSlot::OpaqueGbuffer`] slot, which runs before Opaque, every frame. Producers
//! draw into that slot through the ordinary feature boundary; the shared encoding lives in
//! [`GBUFFER_WGSL`]. The [`lighting`] pass lights what they wrote, after the G-buffer and
//! before Opaque, with the lighting records of [`records`].
//!
//! In observation the lighting pass writes diagnostic targets and nothing reads them except
//! captures, so the visible frame and every other pass are unchanged. When the frame renders
//! from the G-buffer ([`GbufferUse::Render`]), the lighting pass takes over Opaque's colour
//! targets and their clears, so it writes the scene colour and the reflection products SSR
//! reads, and Opaque loads them and draws only the surfaces without a producer. With
//! velocity, the producers write the object-motion residual and the G-buffer slot clears the
//! velocity target in Opaque's place.
mod early;
pub(crate) use early::producer_pass;
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeatureDeclaration, FrameError,
    PassDeclaration, PassSlot, ResourceRef,
};

/// The shared WGSL encode and decode module. It declares no bindings; producers and the
/// lighting pass concatenate it into their own sources.
pub const GBUFFER_WGSL: &str = include_str!("shader/gbuffer.wgsl");

/// One G-buffer target, in colour-attachment location order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GbufferTarget {
    /// Location 0, `Rgb10a2Unorm`: spheremap view-space shading normal in rg.
    G0,
    /// Location 1, `Rgba8UnormSrgb`: base colour.
    G1,
    /// Location 2, `Rgba8Unorm`: roughness, metal, square-root emission, material AO.
    G2,
    /// Location 3, `R16Uint`: octahedral geometric normal and the two engine flags.
    G3,
}

impl GbufferTarget {
    /// Every target in location order.
    pub const ALL: [Self; 4] = [Self::G0, Self::G1, Self::G2, Self::G3];

    /// The target's texture format.
    #[must_use]
    pub const fn format(self) -> wgpu::TextureFormat {
        match self {
            Self::G0 => wgpu::TextureFormat::Rgb10a2Unorm,
            Self::G1 => wgpu::TextureFormat::Rgba8UnormSrgb,
            Self::G2 => wgpu::TextureFormat::Rgba8Unorm,
            Self::G3 => wgpu::TextureFormat::R16Uint,
        }
    }

    /// The resource name the slot declares the target under.
    #[must_use]
    pub const fn resource_name(self) -> &'static str {
        match self {
            Self::G0 => "gbuffer g0",
            Self::G1 => "gbuffer g1",
            Self::G2 => "gbuffer g2",
            Self::G3 => "gbuffer g3",
        }
    }
}

/// The four target formats in location order.
pub const GBUFFER_FORMATS: [wgpu::TextureFormat; 4] = [
    GbufferTarget::G0.format(),
    GbufferTarget::G1.format(),
    GbufferTarget::G2.format(),
    GbufferTarget::G3.format(),
];

/// What a frame does with the G-buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GbufferUse {
    /// `SceneOpaquePath::GBUFFER_OBSERVATION`: store and light into diagnostic targets; the
    /// visible frame stays forward.
    Observe,
    /// `SceneOpaquePath::GBUFFER_RENDER`: the lighting pass writes the scene colour.
    Render,
}

/// The renderer's four targets and the lighting pass's targets, reallocated with the scene
/// extent.
pub(crate) struct Gbuffer {
    textures: [wgpu::Texture; 4],
    lighting: lighting::LightingTargets,
    usage: GbufferUse,
}

impl Gbuffer {
    /// The targets at `size`, with lighting outputs in the scene colour `format` and, with
    /// `reflections`, the two reflection products. Only observation allocates diagnostic
    /// lighting targets; rendering writes Opaque's own.
    pub(crate) fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        (format, reflections): (wgpu::TextureFormat, bool),
        usage: GbufferUse,
    ) -> Self {
        Self {
            textures: GbufferTarget::ALL.map(|target| {
                worldrunner_render_post::color_texture(
                    device,
                    target.resource_name(),
                    size,
                    target.format(),
                )
            }),
            lighting: lighting::LightingTargets::new(
                device,
                size,
                (format, reflections),
                usage == GbufferUse::Observe,
            ),
            usage,
        }
    }

    /// The same targets at another extent.
    pub(crate) fn resized(&self, device: &wgpu::Device, size: [u32; 2]) -> Self {
        Self::new(
            device,
            size,
            (self.lighting.format(), self.lighting.reflections()),
            self.usage,
        )
    }

    /// Whether the frame renders from the G-buffer rather than observing it.
    pub(crate) fn renders(&self) -> bool {
        self.usage == GbufferUse::Render
    }

    pub(crate) fn size(&self) -> [u32; 2] {
        [self.textures[0].width(), self.textures[0].height()]
    }

    pub(crate) fn texture(&self, target: GbufferTarget) -> &wgpu::Texture {
        &self.textures[target as usize]
    }

    /// Every texture the G-buffer slot and the lighting pass write besides Opaque's own: the
    /// four targets and, in observation, the diagnostic lighting targets. A capture segment
    /// must own these writes.
    pub(crate) fn written_textures(&self) -> impl Iterator<Item = &wgpu::Texture> {
        self.textures.iter().chain(self.lighting.textures())
    }

    /// One of the lighting pass's targets; the reflection products only with reflections.
    pub(crate) fn lighting_texture(
        &self,
        target: lighting::LightingTarget,
    ) -> Option<&wgpu::Texture> {
        self.lighting.texture(target)
    }

    /// The lighting pass's colour format and whether it writes the reflection products.
    pub(crate) fn lighting_outputs(&self) -> (wgpu::TextureFormat, bool) {
        (self.lighting.format(), self.lighting.reflections())
    }

    /// Adds the slot's pass, which clears all four targets to zero, and their views. A
    /// producer that declares a depth format in the slot gets the scene depth through
    /// [`attach_producer_depth`].
    pub(crate) fn schedule<'a>(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
        features: impl IntoIterator<Item = &'a FeatureDeclaration>,
    ) -> Result<(), FrameError> {
        if self.renders() {
            let velocity = early::move_early_velocity(passes, views)?;
            self.fuse_prepass(passes, views, velocity.as_ref())?;
            return lighting::schedule_render(passes);
        }
        let mut pass = PassDeclaration::new(PassSlot::OpaqueGbuffer);
        for target in GbufferTarget::ALL {
            let view = AttachmentView::new(
                ResourceRef::new(target.resource_name()),
                self.texture(target),
            )?;
            let attachment = AttachmentDeclaration {
                resource: view.resource(),
                format: view.format(),
                size: view.size(),
                samples: 1,
                load: AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
                read_only: false,
            };
            if pass.color.is_none() {
                pass.color = Some(attachment);
            } else {
                pass.additional_colors.push(attachment);
            }
            views.push(view);
        }
        passes.push(pass);
        self.lighting.schedule(passes, views)?;
        attach_producer_depth(passes, features)
    }
}

/// Declares the scene's G-buffer producer pass when the prepass metadata carries G-buffer
/// pipelines: the four targets in location order, and the finished scene depth to test
/// against, which [`attach_producer_depth`] gives the slot read-only.
pub(crate) fn declare_producer(
    declaration: &mut FeatureDeclaration,
    screen: &crate::scene_metadata::SceneMetadata,
) {
    let Some((pipelines, _)) = &screen.geometry.gbuffer else {
        return;
    };
    let pass = producer_pass(pipelines, screen.depth.format());
    if let Some(existing) = declaration
        .passes
        .iter_mut()
        .find(|existing| existing.slot == pass.slot)
    {
        *existing = pass;
    } else {
        declaration.passes.push(pass);
    }
}

/// Gives the slot the scene depth when a producer declares a depth format there.
///
/// In observation the slot does not own depth. When the scene depth is complete before
/// Opaque (the depth prepass ran, so Opaque loads it), the slot attaches that same depth
/// read-only: producers test against it (`Equal`, like forward reuse) and write none, so
/// depth ownership and the visible frame are unchanged. Without a completed depth the
/// request is refused with [`FrameError::AttachmentContract`]. Making this slot clear and
/// write the scene depth is the switch that admits rendering from the G-buffer.
pub(crate) fn attach_producer_depth<'a>(
    passes: &mut [PassDeclaration],
    features: impl IntoIterator<Item = &'a FeatureDeclaration>,
) -> Result<(), FrameError> {
    let wants_depth = features
        .into_iter()
        .flat_map(|feature| &feature.passes)
        .any(|pass| pass.slot == PassSlot::OpaqueGbuffer && pass.depth_format.is_some());
    if !wants_depth {
        return Ok(());
    }
    let depth = passes
        .iter()
        .find(|pass| pass.slot == PassSlot::Opaque)
        .and_then(|pass| pass.depth.as_ref())
        .filter(|depth| depth.load == AttachmentLoad::Load)
        .cloned()
        .ok_or(FrameError::AttachmentContract(PassSlot::OpaqueGbuffer))?;
    passes
        .iter_mut()
        .find(|pass| pass.slot == PassSlot::OpaqueGbuffer)
        .ok_or(FrameError::MissingSlot(PassSlot::OpaqueGbuffer))?
        .depth = Some(AttachmentDeclaration {
        read_only: true,
        ..depth
    });
    Ok(())
}

/// Declares the four targets and, in observation, the lighting targets for the allocation
/// budget.
pub(crate) fn allocations(
    frame: worldrunner_render_post::FrameConfig,
    reflections: bool,
    usage: GbufferUse,
    set: &mut AllocationSet,
) {
    let size = [frame.size[0], frame.size[1], 1];
    for target in GbufferTarget::ALL {
        set.texture(target.resource_name(), size, target.format(), [1, 1], 1);
    }
    if usage == GbufferUse::Render {
        return;
    }
    let outputs: &[lighting::LightingTarget] = if reflections {
        &[
            lighting::LightingTarget::Colour,
            lighting::LightingTarget::Environment,
            lighting::LightingTarget::Factor,
        ]
    } else {
        &[lighting::LightingTarget::Colour]
    };
    for target in outputs {
        set.texture(target.resource_name(), size, frame.format, [1, 1], 1);
    }
}

pub mod lighting;
pub mod records;

#[cfg(test)]
pub(crate) mod reference;

#[cfg(test)]
pub(crate) mod tests;
