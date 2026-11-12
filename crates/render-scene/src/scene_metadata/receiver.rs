//! Lossless primitive receiver displacements, produced once at pixel centres.
mod admission;
mod cards;
#[cfg(test)]
pub(crate) use cards::reference as late_card_source;
mod backing;
pub(crate) use backing::ReceiverBacking;
mod bindings;
mod reject;
mod reservation;
pub(crate) use bindings::{SetTwo, ShadowInputs, object_layout, shadow_layout};
pub use reservation::ReceiverGradientReservation;
mod shader;
mod targets;
#[cfg(test)]
mod tests;

pub(crate) use targets::ReceiverPayload;

/// The number of independently displaced receivers retained by a geometry pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReceiverLayout {
    Front,
    Opposing,
}
impl ReceiverLayout {
    pub(crate) const fn sides(self) -> u32 {
        match self {
            Self::Front => 1,
            Self::Opposing => 2,
        }
    }
    pub(crate) const fn bytes(self) -> u32 {
        self.sides() * 24
    }
    pub(crate) fn formats(self) -> Vec<wgpu::TextureFormat> {
        (0..self.sides())
            .flat_map(|_| {
                [
                    wgpu::TextureFormat::Rgba32Float,
                    wgpu::TextureFormat::Rg32Float,
                ]
            })
            .collect()
    }
    pub(crate) fn validate(
        self,
        limits: &wgpu::Limits,
        metadata_colors: u32,
    ) -> Result<(), &'static str> {
        if metadata_colors + self.sides() * 2 > limits.max_color_attachments {
            return Err("receiver displacement colour attachment count");
        }
        if metadata_colors * 8 + self.bytes() > limits.max_color_attachment_bytes_per_sample {
            return Err("receiver displacement colour attachment bytes");
        }
        Ok(())
    }
    pub(crate) fn allocations(
        self,
        size: [u32; 2],
    ) -> worldrunner_render_gpu::configuration::AllocationSet {
        let mut set = worldrunner_render_gpu::configuration::AllocationSet::default();
        for format in [
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureFormat::Rg32Float,
        ] {
            set.texture(
                "receiver displacements",
                [size[0], size[1], self.sides()],
                format,
                [1, 1],
                1,
            );
        }
        set
    }
}

pub(crate) fn producer_sources(
    output: super::MetadataOutput,
    layout: ReceiverLayout,
    bias: f32,
) -> (String, String) {
    shader::producer(output, layout, bias)
}
pub(crate) fn consumer_source(source: &str, layout: ReceiverLayout, terrain: bool) -> String {
    shader::consumer(source, layout, terrain)
}

/// The object consumer source with the relief card early reject; see [`reject`].
pub(crate) fn card_reject_source(source: &str, layout: ReceiverLayout) -> String {
    reject::card_reject_source(source, layout)
}

#[cfg(test)]
pub(crate) fn function_source<'a>(source: &'a str, name: &str) -> &'a str {
    shader::function(source, name)
}
#[cfg(test)]
pub(crate) fn probe_source(source: &str) -> String {
    // Preserve shading and its control flow while exposing displacements in existing MRTs.
    let source = source.replace(
        "let receiver_dy = dpdy(position) + dpdy(normal) * shadow.bias.y;",
        "let receiver_dy = dpdy(position) + dpdy(normal) * shadow.bias.y; scene_probe_dx = receiver_dx; scene_probe_dy = receiver_dy;",
    ).replace(
        "vec4<f32>(published_environment, 1.0), vec4<f32>(published_factor, 1.0)",
        "vec4<f32>(scene_probe_dx, scene_probe_dy.x), vec4<f32>(scene_probe_dy.yz, 0.0, 0.0)",
    );
    let source = if source.contains("struct ReflectionOutput") {
        source
    } else {
        source.replace(
            "let colour = lighting.color;",
            "let colour = lighting.color + scene_probe_dx * 0.25 + scene_probe_dy * 0.5;",
        )
    };
    format!(
        "{source}
var<private> scene_probe_dx: vec3<f32>; var<private> scene_probe_dy: vec3<f32>;"
    )
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ReceiverReference {
    LateCardRejection,
    /// The runtime receivers with reused depth, without the relief card early reject.
    WithoutCardReject,
    /// The runtime receivers with reused depth, rejecting every relief card quad.
    ForcedCardReject,
    StoredShared,
    StoredIndependent,
    ProbeIndependent,
    StoredProbeIndependent,
    ProbeShared,
    ProbeReuse,
    OffsetProbeReuse,
    StoredOffsetProbeReuse,
    StoredProbeShared,
    StoredProbeReuse,
    BlackShared,
}
#[cfg(test)]
impl ReceiverReference {
    pub(crate) const fn depth(self) -> crate::renderer::depth::DepthMode {
        match self {
            Self::StoredIndependent | Self::ProbeIndependent | Self::StoredProbeIndependent => {
                crate::renderer::depth::DepthMode::Independent
            }
            Self::StoredShared
            | Self::ProbeShared
            | Self::StoredProbeShared
            | Self::BlackShared => crate::renderer::depth::DepthMode::Shared,
            Self::LateCardRejection
            | Self::WithoutCardReject
            | Self::ForcedCardReject
            | Self::ProbeReuse
            | Self::StoredProbeReuse
            | Self::OffsetProbeReuse
            | Self::StoredOffsetProbeReuse => crate::renderer::depth::DepthMode::Reuse,
        }
    }
    pub(crate) const fn local(self) -> bool {
        matches!(
            self,
            Self::ProbeShared
                | Self::ProbeReuse
                | Self::BlackShared
                | Self::ProbeIndependent
                | Self::OffsetProbeReuse
        )
    }
    pub(crate) const fn probe(self) -> bool {
        matches!(
            self,
            Self::ProbeShared
                | Self::ProbeReuse
                | Self::OffsetProbeReuse
                | Self::StoredOffsetProbeReuse
                | Self::StoredProbeShared
                | Self::StoredProbeReuse
                | Self::ProbeIndependent
                | Self::StoredProbeIndependent
                | Self::BlackShared
        )
    }
}

impl crate::compositor::FrameRenderer {
    pub(crate) fn prepare_receiver_gradients(
        &mut self,
        device: &wgpu::Device,
        gpu: &crate::draw::SceneGpu,
        bias: f32,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        let paired = self.targets().config().samples == 1 && gpu.has_shadow_receivers();
        if let Some(screen) = &mut self.ssao {
            if !paired {
                screen.receiver = None;
                return Ok(());
            }
            let layout = if bias != 0.0 && gpu.has_transmission() {
                ReceiverLayout::Opposing
            } else {
                ReceiverLayout::Front
            };
            if !self.receiver_gradients.admits(layout) {
                return Err(worldrunner_render_post::FrameError::UnsupportedEffect(
                    "receiver displacements exceed configured reservation",
                ));
            }
            let output = ReceiverPayload::output_for(screen);
            let colors = admission::metadata_colors(output);
            admission::validate(&device.limits(), layout, colors, gpu.terrain_surface())?;
            let card_reject =
                admission::card_reject(&device.limits(), device.adapter_info().backend, colors);
            let reserved = self
                .receiver_gradients
                .layout()
                .expect("admitted receiver layout");
            screen
                .receiver_backing
                .get_or_insert_with(|| ReceiverBacking::new(device, screen.size, reserved));
            // `card_reject` follows from the device and `output`, so it needs no check of its own.
            if screen.receiver.as_ref().is_none_or(|receiver| {
                receiver.layout != layout
                    || receiver.bias.to_bits() != bias.to_bits()
                    || receiver.output != output
            }) {
                screen.receiver = Some(ReceiverPayload::new(
                    device,
                    screen,
                    layout,
                    bias,
                    card_reject,
                ));
            }
        }
        Ok(())
    }
    /// The pipelines this frame's colour pass draws with: the receiver sibling of `pipelines`,
    /// with relief cards rejecting hidden fragments when the receivers admitted it and the
    /// pass reuses the prepass depth, the only mode that can attach it read-only.
    pub(crate) fn receiver_pipelines(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
        depth: crate::renderer::depth::DepthMode,
    ) -> crate::renderer::ScenePipelines {
        self.ssao
            .as_ref()
            .and_then(|screen| screen.receiver.as_ref())
            .map_or_else(
                || pipelines.clone(),
                |receiver| {
                    #[cfg(test)]
                    if let Some(reference) = self.receiver_reference
                        && (reference.local()
                            || reference.probe()
                            || matches!(reference, ReceiverReference::LateCardRejection))
                    {
                        return pipelines.for_receiver_reference(reference, receiver.layout);
                    }
                    let reject = admission::selects_card_reject(receiver.card_reject, depth);
                    #[cfg(test)]
                    match self.receiver_reference {
                        Some(ReceiverReference::WithoutCardReject) => {
                            return pipelines.for_receiver_gradients(receiver.layout, false);
                        }
                        Some(ReceiverReference::ForcedCardReject) if reject => {
                            return pipelines.for_forced_card_reject(receiver.layout);
                        }
                        _ => {}
                    }
                    pipelines.for_receiver_gradients(receiver.layout, reject)
                },
            )
    }

    /// The frame's set-2 groups. Without receivers both are `fallback`. `card_depth` is the
    /// Opaque depth attachment, given when the selected pipelines reject relief cards.
    pub(crate) fn shadow_groups(
        &self,
        device: &wgpu::Device,
        (fallback, map): (&wgpu::BindGroup, ShadowInputs<'_>),
        card_depth: Option<&worldrunner_render_post::AttachmentView>,
    ) -> SetTwo {
        let Some(receiver) = self
            .ssao
            .as_ref()
            .and_then(|screen| screen.receiver.as_ref())
        else {
            debug_assert!(card_depth.is_none(), "the card reject needs receivers");
            return SetTwo {
                object: fallback.clone(),
                terrain: fallback.clone(),
            };
        };
        let view = card_depth.map(|depth| {
            depth.texture().create_view(&wgpu::TextureViewDescriptor {
                label: Some("card reject depth"),
                aspect: wgpu::TextureAspect::DepthOnly,
                ..Default::default()
            })
        });
        bindings::set_two(device, receiver, &map, view.as_ref())
    }
}

pub(crate) fn source_formats(source: &str) -> Vec<wgpu::TextureFormat> {
    if source.contains("const SCENE_RECEIVER_SIDES: u32 = 2u;") {
        ReceiverLayout::Opposing.formats()
    } else if source.contains("const SCENE_RECEIVER_SIDES: u32 = 1u;") {
        ReceiverLayout::Front.formats()
    } else {
        Vec::new()
    }
}
