//! Byte-level captures of the terrain material.
//!
//! Each configuration draws the same terrain twice on one device: once through the scene and
//! once through the frozen copy of the fixed path in [`crate::renderer::legacy_terrain`], with
//! the fixed pipelines, layouts and bind groups built from the resources the scene's terrain
//! holds. Every colour attachment is read back and compared byte for byte. Terrain writes every
//! attachment of its pass, so each is also compared with an untouched render, which keeps
//! "terrain drew nothing" from passing.
//!
//! Each run prints one `TERRAIN-CAPTURE` line per configuration, surface source and attachment
//! with a hash of the bytes, so captures taken before and after a change can be compared as
//! text. Hashes differ between adapters and are only comparable on one.
//!
//! Surface sources are the four terrain draws select between: [`Source::Plain`] (palette
//! arrays and a type grid), [`Source::Pages`] (baked material pages), [`Source::Layered`]
//! (layered source composition) and [`Source::LayeredPages`] (layered pages).

use wgpu::{BindGroup, CommandEncoder, RenderPass, TextureFormat};
use worldrunner_render_plan::{RenderScene, SceneSsao, SceneSsaoReceiver};

use crate::device::GpuContext;
use crate::draw::{DrawCount, SceneGpu};
use crate::dynamic::capture_tests::{
    Attachments, CONFIGS, Config, SIZE, build, context, copy_results, digest, fill, read_results,
    supported,
};
use crate::frame::FrameUniform;
use crate::renderer::ScenePipelines;

mod legacy;
pub(super) mod scenes;
mod shadow;
pub(super) mod streaming;
mod variants;

pub(super) use legacy::Legacy;

/// Which terrain draw a capture exercises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Plain,
    Pages,
    Layered,
    LayeredPages,
}

impl Source {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Pages => "pages",
            Self::Layered => "layered",
            Self::LayeredPages => "layered-pages",
        }
    }

    pub(super) const fn layered(self) -> bool {
        matches!(self, Self::Layered | Self::LayeredPages)
    }

    pub(super) const fn pages(self) -> bool {
        matches!(self, Self::Pages | Self::LayeredPages)
    }

    fn scene(self) -> RenderScene {
        if self.layered() {
            scenes::layered()
        } else {
            scenes::plain()
        }
    }
}

/// The attachments of a pass and the sample count of its pipelines.
#[derive(Clone, Copy)]
pub(super) struct Target<'a> {
    pub formats: &'a [TextureFormat],
    pub samples: u32,
}

/// One scene uploaded for a surface source, with the frame it draws.
pub(super) struct Rig<'a> {
    pub context: &'a GpuContext,
    pub gpu: SceneGpu,
    pub frame: FrameUniform,
}

impl<'a> Rig<'a> {
    /// Uploads the terrain of `source` for `pipelines`. Page sources are enabled and prepared
    /// until the root page is resident.
    pub(super) fn new(
        context: &'a GpuContext,
        pipelines: &ScenePipelines,
        scene: &RenderScene,
        (source, frame): (Source, FrameUniform),
    ) -> Self {
        let (device, authority) = (context.device(), context.submission());
        let gpu = SceneGpu::upload(device, authority, scene, pipelines).unwrap();
        gpu.set_frame(context.queue(), frame);
        let mut rig = Self {
            context,
            gpu,
            frame,
        };
        if source.pages() {
            rig.gpu
                .enable_terrain_pages(
                    device,
                    authority,
                    pipelines,
                    scenes::page_source(source.layered()),
                )
                .unwrap();
            for _ in 0..4 {
                let mut transaction = authority.begin(device).unwrap();
                rig.prepare(&mut transaction);
                transaction.finish().submit().unwrap();
                if rig.gpu.terrain_pages_ready() {
                    break;
                }
            }
            assert!(
                rig.gpu.terrain_pages_ready(),
                "pages did not become resident"
            );
        }
        rig
    }

    /// Prepares material pages in the frame `encoder` records, when the terrain has any.
    pub(super) fn prepare(&self, encoder: &mut CommandEncoder) {
        if self.gpu.has_terrain_pages() {
            self.gpu
                .prepare_terrain_pages(self.context.device(), encoder, &self.frame, [SIZE; 2])
                .unwrap();
        }
    }

    /// Makes the scene's receiver the one `receiver` names.
    pub(super) fn receiving(mut self, receiver: SceneSsaoReceiver) -> Self {
        self.gpu.ssao = Some(SceneSsao {
            receiver,
            depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
            kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
            filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
            radius_millimeters: 2000,
            intensity_micro: 1_000_000,
            sample_count: 8,
            bias_millimeters: 10,
            horizon_angle_millidegrees: 90_000,
        });
        self
    }

    /// The pipelines the scene selects for its receiver and the frame's convention and winding.
    pub(super) fn selected(&self, pipelines: &ScenePipelines) -> ScenePipelines {
        let receiver = self
            .gpu
            .ssao
            .map_or(SceneSsaoReceiver::Nearest, |ao| ao.receiver);
        pipelines
            .for_receiver(receiver)
            .for_raster(self.frame.specular_convention, self.frame.front_face())
    }
}

/// Renders one pass over fresh attachments filled with [`fill`]. `before` records into the
/// frame ahead of the pass, as page preparation does, and `draw` into the pass. Returns the
/// bytes of every colour attachment.
pub(super) fn render(
    context: &GpuContext,
    target: Target<'_>,
    before: impl FnOnce(&mut CommandEncoder),
    draw: impl FnOnce(&mut RenderPass<'_>),
) -> Vec<Vec<u8>> {
    let device = context.device();
    let attachments = Attachments::new(device, target.formats, target.samples);
    let resolve_views: Vec<_> = attachments
        .resolved
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let colors: Vec<_> = attachments
        .views
        .iter()
        .zip(target.formats)
        .enumerate()
        .map(|(index, (view, &format))| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: resolve_views.get(index),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(fill(index, format)),
                    store: if target.samples == 1 {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                },
            })
        })
        .collect();
    let mut encoder = context.submission().begin(device).unwrap();
    before(&mut encoder);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain capture"),
            color_attachments: &colors,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &attachments.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        draw(&mut pass);
    }
    let staging = copy_results(device, &mut encoder, attachments.results());
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, staging)
}

/// What one capture drew: the scene's attachments and the draw calls it recorded, which equal
/// the fixed path's.
pub(super) struct Captured {
    pub images: Vec<Vec<u8>>,
    pub scene: DrawCount,
}

/// What a capture draws with besides the rig: the pipelines the scene was built with, the group
/// bound at set three, the shadow group and the names the hash lines carry.
pub(super) struct Case<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub target: Target<'a>,
    pub pipelines: &'a ScenePipelines,
    pub screen: Option<&'a BindGroup>,
    pub shadow: Option<&'a BindGroup>,
}

/// Draws the rig's terrain through the scene and through the fixed path, checks that the two
/// agree on every attachment and that terrain drew something, prints the hash lines and returns
/// the scene's attachments.
pub(super) fn capture(rig: &Rig<'_>, case: &Case<'_>) -> Captured {
    let legacy = Legacy::new(rig.context, &rig.gpu, &rig.selected(case.pipelines));
    capture_against(rig, case, &legacy)
}

/// [`capture`] against fixed pipelines built earlier for the same pipelines and frame.
pub(super) fn capture_against(rig: &Rig<'_>, case: &Case<'_>, legacy: &Legacy) -> Captured {
    let context = rig.context;
    let bind = |pass: &mut RenderPass<'_>| {
        if let Some(screen) = case.screen {
            pass.set_bind_group(3, screen, &[]);
        }
    };
    let mut scene_count = DrawCount::default();
    let scene = render(
        context,
        case.target,
        |encoder| rig.prepare(encoder),
        |pass| {
            bind(pass);
            let drawn = rig.gpu.draw_color(pass, case.pipelines, case.shadow);
            assert_eq!(drawn.sky, DrawCount::default(), "the fixture draws no sky");
            scene_count = drawn.terrain;
        },
    );
    let mut fixed_count = DrawCount::default();
    let fixed = render(
        context,
        case.target,
        |encoder| rig.prepare(encoder),
        |pass| {
            bind(pass);
            pass.set_bind_group(0, &rig.gpu.legacy_frame_group(), &[]);
            fixed_count = legacy.draw(pass, &rig.gpu, case.shadow);
        },
    );
    let untouched = render(context, case.target, |_| {}, |_| {});
    assert_eq!(scene.len(), case.target.formats.len());
    for (index, ((scene, fixed), untouched)) in scene.iter().zip(&fixed).zip(&untouched).enumerate()
    {
        assert!(
            scene == fixed,
            "{} {}: attachment {index} differs from the fixed pipeline's output",
            case.name,
            case.source
        );
        if index == 0 {
            assert!(
                scene != untouched,
                "{} {}: terrain drew nothing into the colour attachment",
                case.name,
                case.source
            );
        }
    }
    assert_eq!(
        scene_count, fixed_count,
        "{} {}: draw counts differ",
        case.name, case.source
    );
    report(context, case, &scene);
    Captured {
        images: scene,
        scene: scene_count,
    }
}

fn report(context: &GpuContext, case: &Case<'_>, images: &[Vec<u8>]) {
    let adapter = context.adapter().get_info().name;
    for (index, image) in images.iter().enumerate() {
        println!(
            "TERRAIN-CAPTURE adapter={adapter:?} config={} source={} attachment={index} bytes={} hash={:016x}",
            case.name,
            case.source,
            image.len(),
            digest(image)
        );
    }
}

/// Whether the device draws `source`: layered terrain needs the device limits layered sources
/// check.
pub(super) fn drawable(context: &GpuContext, source: Source) -> bool {
    !source.layered() || crate::terrain::pages::layered_gpu::supported(&context.device().limits())
}

/// Captures `source` in every forward target set of the dynamic quads' harness, returning how
/// many captures ran and how many were skipped.
fn forward_sets(source: Source) -> (usize, usize) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut run, mut skipped) = (0, 0);
    for config in &CONFIGS {
        if !supported(&context, config) || !drawable(&context, source) {
            println!(
                "TERRAIN-CAPTURE config={} source={} skipped: format, samples or limits unsupported",
                config.name,
                source.name()
            );
            skipped += 1;
            continue;
        }
        forward(&context, config, source);
        run += 1;
    }
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=forward source={} compared={run} skipped={skipped}",
        source.name()
    );
    (run, skipped)
}

/// One configuration of the 12 forward target sets with one surface source.
fn forward(context: &GpuContext, config: &Config, source: Source) -> Captured {
    let built = build(context.device(), config);
    let rig = Rig::new(
        context,
        &built.pipelines,
        &source.scene(),
        (source, scenes::frame(scenes::pose())),
    );
    capture(
        &rig,
        &Case {
            name: config.name,
            source: source.name(),
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &built.pipelines,
            screen: built.screen.as_ref(),
            shadow: None,
        },
    )
}

#[test]
fn plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set() {
    let (run, skipped) = forward_sets(Source::Plain);
    assert_eq!(run + skipped, CONFIGS.len());
    assert!(run > 0, "no forward target set could run");
}

#[test]
fn page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set() {
    let (run, skipped) = forward_sets(Source::Pages);
    assert_eq!(run + skipped, CONFIGS.len());
    assert!(run > 0, "no forward target set could run");
}

#[test]
fn layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set() {
    let (run, skipped) = forward_sets(Source::Layered);
    assert_eq!(run + skipped, CONFIGS.len());
}

#[test]
fn layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set() {
    let (run, skipped) = forward_sets(Source::LayeredPages);
    assert_eq!(run + skipped, CONFIGS.len());
}
