//! Byte-level captures of the lit object family: objects, foliage and impostors.
//!
//! Each configuration draws the same objects twice on one device: once through the scene and
//! once through the frozen copy of the fixed path in [`crate::renderer::legacy_objects`], with
//! pipelines built from the frozen shader snapshots. Every colour attachment is read back, and
//! at one sample the depth plane too, and compared byte for byte. Each is also compared with an
//! untouched render, which keeps "the objects drew nothing" from passing.
//!
//! Each run prints one `OBJECT-CAPTURE` line per configuration, scene and attachment with a
//! hash of the bytes, so captures taken before and after a change can be compared as text.
//! Hashes differ between adapters and are only comparable on one. A capture that cannot run on
//! the adapter fails: the summary line says `skipped=0` when none did.
//!
//! The scenes cover the 32 `PipelineKey` variants. [`scenes::surface_specs`],
//! [`scenes::impostor_specs`] and relief impostors draw through `SceneGpu::draw_color`. The keys
//! no material reaches are drawn by [`Masked`], which sends the index of one of the
//! [`scenes::base_specs`] cards to the pipeline of the key `index ^ mask`, on the scene's and on
//! the fixed path's pipelines alike.

use std::collections::BTreeSet;

use wgpu::{BindGroup, RenderPass, TextureFormat};
use worldrunner_render_plan::{RenderScene, SceneSsao, SceneSsaoReceiver};

use crate::device::GpuContext;
use crate::draw::{DrawCount, SceneGpu};
use crate::dynamic::capture_tests::{
    Attachments, CONFIGS, Config, Kind, SIZE, VELOCITY, build, context, copy_results, digest, fill,
    read_results, supported,
};
use crate::frame::FrameUniform;
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::renderer::legacy_objects::{Counted, Recipe, Recorded, compose};

mod gather;
mod legacy;
pub(in crate::object) mod scenes;
mod shadow;
mod variants;

pub(in crate::object) use legacy::{Legacy, Masked};

/// The attachments of a pass and the sample count of its pipelines.
#[derive(Clone, Copy)]
pub(in crate::object) struct Target<'a> {
    pub formats: &'a [TextureFormat],
    pub samples: u32,
}

/// One scene uploaded for a context, with the frame it draws.
pub(in crate::object) struct Rig<'a> {
    pub context: &'a GpuContext,
    pub gpu: SceneGpu,
    pub frame: FrameUniform,
    /// How far the cards move between the previous transforms and the current ones, when the
    /// pipelines write the object-motion residual.
    motion: Option<i64>,
    count: usize,
}

impl<'a> Rig<'a> {
    /// Uploads `scene` for `pipelines`. When they write the object-motion residual, every card
    /// is [`scenes::MOTION_MILLIMETERS`] from where its previous transform was recorded each
    /// time the rig is primed, so the previous-instance stream differs from the current one.
    pub(in crate::object) fn new(
        context: &'a GpuContext,
        pipelines: &ScenePipelines,
        scene: &RenderScene,
    ) -> Self {
        Self::moving(context, pipelines, scene, true)
    }

    /// [`Self::new`], with the cards left where their previous transforms were recorded when
    /// `moving` is false.
    pub(in crate::object) fn moving(
        context: &'a GpuContext,
        pipelines: &ScenePipelines,
        scene: &RenderScene,
        moving: bool,
    ) -> Self {
        let mut gpu =
            SceneGpu::upload(context.device(), context.submission(), scene, pipelines).unwrap();
        let motion = pipelines.writes_velocity().then(|| {
            gpu.track_previous_transforms(true);
            if moving {
                scenes::MOTION_MILLIMETERS
            } else {
                0
            }
        });
        let frame = scenes::frame(scenes::pose());
        gpu.set_frame(context.queue(), frame);
        let mut rig = Self {
            context,
            gpu,
            frame,
            motion,
            count: scene.geometry().objects().len(),
        };
        rig.prime();
        rig
    }

    /// Records the cards' current placements as their previous ones and then moves them, so the
    /// next frame flushed finds the previous-instance stream behind the current one. A frame
    /// that flushes consumes the difference, so every frame primes first. Without a velocity
    /// target there is no previous stream and nothing to do.
    pub(in crate::object) fn prime(&mut self) {
        let Some(shift) = self.motion else {
            return;
        };
        let (device, authority) = (self.context.device(), self.context.submission());
        let handles: Vec<_> = self.gpu.instance_handles().collect();
        let place = |gpu: &mut SceneGpu, shift| {
            for (index, handle) in handles.iter().enumerate() {
                gpu.set_instance_transform(*handle, scenes::placement(index, self.count, shift))
                    .unwrap();
            }
        };
        place(&mut self.gpu, 0);
        // Two flushes publish the placements and then converge the previous ones to them.
        for _ in 0..2 {
            self.gpu.flush_instances(device, authority).unwrap();
        }
        place(&mut self.gpu, shift);
    }

    /// Makes the scene's receiver the one `receiver` names.
    pub(in crate::object) fn receiving(mut self, receiver: SceneSsaoReceiver) -> Self {
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
    pub(in crate::object) fn selected(&self, pipelines: &ScenePipelines) -> ScenePipelines {
        let receiver = self
            .gpu
            .ssao
            .map_or(SceneSsaoReceiver::Nearest, |ao| ao.receiver);
        pipelines
            .for_receiver(receiver)
            .for_raster(self.frame.specular_convention, self.frame.front_face())
    }
}

/// Renders one pass over fresh attachments filled with [`fill`] and a depth plane cleared to
/// the far plane. The frame flushes the scene's instance updates in its own encoder, as the
/// frame renderer does, and `draw` records into the pass. Returns the bytes of every colour
/// attachment and, at one sample, of the depth plane after them.
pub(in crate::object) fn render(
    context: &GpuContext,
    target: Target<'_>,
    gpu: &mut SceneGpu,
    draw: impl FnOnce(&mut RenderPass<'_>, &SceneGpu),
) -> Vec<Vec<u8>> {
    let device = context.device();
    let attachments = Attachments::new(device, target.formats, target.samples);
    let single = target.samples == 1;
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("object capture depth"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: target.samples,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if single {
                wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
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
                    store: if single {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                },
            })
        })
        .collect();
    let mut encoder = context.submission().begin(device).unwrap();
    gpu.flush_instances_encoded(device, &mut encoder).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("object capture"),
            color_attachments: &colors,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: if single {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        draw(&mut pass, gpu);
    }
    let mut staging = copy_results(device, &mut encoder, attachments.results());
    if single {
        staging.push(copy_depth(device, &mut encoder, &depth));
    }
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, staging)
}

/// Copies the depth plane into a mappable buffer, as [`copy_results`] does for colour.
fn copy_depth(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
) -> (wgpu::Buffer, usize, usize) {
    let row = SIZE * 4;
    let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("object capture depth readback"),
        size: u64::from(padded * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    (buffer, row as usize, padded as usize)
}

/// What a capture draws with besides the rig: the pipelines the scene was built with and the
/// frozen source the fixed path is built from, the groups bound at sets two and three, and the
/// names the hash lines carry.
#[derive(Clone, Copy)]
pub(in crate::object) struct Case<'a> {
    pub name: &'a str,
    pub scene: &'a str,
    pub target: Target<'a>,
    pub pipelines: &'a ScenePipelines,
    pub source: &'a str,
    pub screen: Option<&'a BindGroup>,
    pub shadow: Option<&'a BindGroup>,
    /// Sends pipeline index `i` to the pipeline of key `i ^ mask`. Zero draws through
    /// `SceneGpu::draw_color` without any change.
    pub mask: usize,
}

/// What one capture drew.
pub(in crate::object) struct Captured {
    pub images: Vec<Vec<u8>>,
    /// The keys, as indices into [`PipelineKey::ALL`], the draw used.
    pub keys: BTreeSet<usize>,
}

/// Draws the rig's objects through the scene and through the fixed path, checks that the two
/// agree on every attachment and that the objects drew something, prints the hash lines and
/// returns the scene's attachments.
pub(in crate::object) fn capture(rig: &mut Rig<'_>, legacy: &Legacy, case: &Case<'_>) -> Captured {
    let bind = |pass: &mut RenderPass<'_>| {
        if let Some(shadow) = case.shadow {
            pass.set_bind_group(2, shadow, &[]);
        }
        if let Some(screen) = case.screen {
            pass.set_bind_group(3, screen, &[]);
        }
    };
    let selected = rig.selected(case.pipelines);
    let context = rig.context;
    rig.prime();
    let scene = render(context, case.target, &mut rig.gpu, |pass, gpu| {
        bind(pass);
        if case.mask == 0 {
            let drawn = gpu.draw_color(pass, case.pipelines, case.shadow);
            assert_eq!(
                drawn.terrain,
                DrawCount::default(),
                "the fixture has no terrain"
            );
            assert_eq!(drawn.sky, DrawCount::default(), "the fixture draws no sky");
        } else {
            let frame = gpu.legacy_objects().frame;
            gpu.draw_objects(
                pass,
                &frame,
                &Masked(selected.object_pipelines(), case.mask),
                None,
            );
        }
    });
    let mut recorded = Recorded::default();
    rig.prime();
    let fixed = render(context, case.target, &mut rig.gpu, |pass, gpu| {
        bind(pass);
        recorded = legacy.draw(pass, gpu, case.mask);
    });
    rig.prime();
    let untouched = render(context, case.target, &mut rig.gpu, |_, _| {});
    let targets: Vec<_> = legacy
        .pipelines()
        .targets()
        .iter()
        .flatten()
        .map(|target| target.format)
        .collect();
    assert_eq!(
        targets, case.target.formats,
        "{}: the fixed pipelines write other targets than the pass has",
        case.name
    );
    compare(case, [&scene, &fixed, &untouched]);
    assert!(
        recorded.calls > 0,
        "{}: the fixed path drew nothing",
        case.name
    );
    report(context, case, &scene);
    Captured {
        images: scene,
        keys: keys_used(&rig.gpu, case.mask),
    }
}

/// The keys the scene's draws select, after the mask.
fn keys_used(gpu: &SceneGpu, mask: usize) -> BTreeSet<usize> {
    let objects = gpu.legacy_objects();
    let mut keys = BTreeSet::new();
    for draw in &objects.draws {
        let levels = std::iter::once(&draw.level).chain(draw.transition.iter().map(|(_, l)| l));
        for level in levels {
            match &level.slots {
                Some(slots) => keys.extend(slots.iter().map(|slot| slot.pipeline ^ mask)),
                None => {
                    keys.insert(draw.pipeline ^ mask);
                }
            }
        }
    }
    keys
}

fn attachment_name(index: usize, case: &Case<'_>) -> String {
    if index < case.target.formats.len() {
        format!("attachment {index}")
    } else {
        "depth plane".to_owned()
    }
}

/// Checks the scene's attachments against the fixed path's, byte for byte, and that the objects
/// changed the colour, the velocity target when there is one, and the depth plane from an
/// untouched render. The message names the guard test too: a snapshot that is stale shows up
/// here as a difference.
fn compare(case: &Case<'_>, [scene, fixed, untouched]: [&[Vec<u8>]; 3]) {
    let colors = case.target.formats.len();
    let planes = usize::from(case.target.samples == 1);
    assert_eq!(scene.len(), colors + planes);
    for (index, ((scene, fixed), untouched)) in scene.iter().zip(fixed).zip(untouched).enumerate() {
        assert!(
            scene == fixed,
            "{} {}: {} differs from the fixed pipeline's output (if \
             the_live_object_wgsl_equals_the_frozen_snapshot fails too, copy the live WGSL into \
             object/fixtures)",
            case.name,
            case.scene,
            attachment_name(index, case)
        );
        let written = index == 0
            || (index < colors && case.target.formats[index] == VELOCITY)
            || index == colors;
        assert!(
            !written || scene != untouched,
            "{} {}: the objects left {} as it was",
            case.name,
            case.scene,
            attachment_name(index, case)
        );
    }
}

fn report(context: &GpuContext, case: &Case<'_>, images: &[Vec<u8>]) {
    let adapter = context.adapter().get_info().name;
    for (index, image) in images.iter().enumerate() {
        let attachment = if index < case.target.formats.len() {
            index.to_string()
        } else {
            "depth".to_owned()
        };
        println!(
            "OBJECT-CAPTURE adapter={adapter:?} config={} scene={} mask={} attachment={attachment} bytes={} hash={:016x}",
            case.name,
            case.scene,
            case.mask,
            image.len(),
            digest(image)
        );
    }
}

/// The scene features a configuration's object source is specialised for.
fn recipe(kind: Kind) -> Recipe {
    match kind {
        Kind::Plain(_) => Recipe::default(),
        Kind::Velocity => Recipe {
            velocity: true,
            ..Recipe::default()
        },
        Kind::Ssao => Recipe {
            ssao: true,
            ..Recipe::default()
        },
        Kind::SsaoVelocity => Recipe {
            ssao: true,
            velocity: true,
            ..Recipe::default()
        },
        Kind::Reflection { velocity } => Recipe {
            reflection: true,
            velocity,
            ..Recipe::default()
        },
    }
}

/// The frozen source of `recipe`. While the live WGSL equals the snapshots, it is also the live
/// source the scene's pipelines were built from, which checks the composition recipe.
fn frozen_source(pipelines: &ScenePipelines, recipe: Recipe) -> String {
    let source = compose(recipe);
    if crate::shader::ENVIRONMENT_WGSL == compose(Recipe::default()) && !recipe.shadowed {
        assert!(
            source == crate::renderer::legacy_objects::Build::live_source(pipelines),
            "{recipe:?}: the frozen composition differs from the live one"
        );
    }
    source
}

/// A scene, the specs of its cards and the pipeline masks it is drawn with.
struct Scene {
    name: &'static str,
    specs: Vec<scenes::Spec>,
    masks: &'static [usize],
}

/// The scenes of a full capture. The base cards are drawn with the masks that reach the relief
/// surfaces and the coverage keys with no mask, which together are all 32 keys.
fn all_scenes() -> Vec<Scene> {
    vec![
        Scene {
            name: "surface",
            specs: scenes::surface_specs(),
            masks: &[0],
        },
        Scene {
            name: "impostor",
            specs: scenes::impostor_specs(false),
            masks: &[0],
        },
        Scene {
            name: "relief",
            specs: scenes::impostor_specs(true),
            masks: &[0],
        },
        Scene {
            name: "base",
            specs: scenes::base_specs(),
            masks: &[0, 4, 8, 12],
        },
    ]
}

/// Captures every scene of [`all_scenes`] with the pipelines of `case`, returning the keys they
/// used and how many captures ran.
///
/// `legacy` holds the fixed pipelines of the context. They are built when the first scene is
/// uploaded, from the pipelines that scene selects, and then reused: compiling 32 pipelines is
/// the cost of a capture, and every scene of a context selects the same ones.
pub(in crate::object) fn capture_scenes(
    context: &GpuContext,
    case: &Case<'_>,
    receiver: Option<SceneSsaoReceiver>,
    legacy: &mut Option<Legacy>,
) -> (BTreeSet<usize>, usize) {
    let mut keys = BTreeSet::new();
    let mut run = 0;
    for scene in all_scenes() {
        let rig = Rig::new(context, case.pipelines, &scenes::scene(&scene.specs));
        let mut rig = match receiver {
            Some(receiver) => rig.receiving(receiver),
            None => rig,
        };
        let fixed = legacy.get_or_insert_with(|| {
            Legacy::new(context, &rig.selected(case.pipelines), case.source)
        });
        for &mask in scene.masks {
            let captured = capture(
                &mut rig,
                fixed,
                &Case {
                    scene: scene.name,
                    mask,
                    ..*case
                },
            );
            keys.extend(captured.keys);
            run += 1;
        }
    }
    (keys, run)
}

/// How many captures a full run of [`capture_scenes`] makes.
fn captures_per_config() -> usize {
    all_scenes().iter().map(|scene| scene.masks.len()).sum()
}

/// One of the 12 forward target sets, all scenes.
fn forward(context: &GpuContext, config: &Config) -> (usize, usize) {
    let built = build(context.device(), config);
    let source = frozen_source(&built.pipelines, recipe(config.kind));
    let (keys, run) = capture_scenes(
        context,
        &Case {
            name: config.name,
            scene: "",
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &built.pipelines,
            source: &source,
            screen: built.screen.as_ref(),
            shadow: None,
            mask: 0,
        },
        None,
        &mut None,
    );
    (keys.len(), run)
}

fn forward_set(index: usize) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let config = &CONFIGS[index];
    if !supported(&context, config) {
        println!(
            "OBJECT-CAPTURE-SUMMARY set=forward config={} keys=0 compared=0 skipped={}",
            config.name,
            captures_per_config()
        );
        panic!(
            "{}: the adapter cannot draw this target set, and a skipped capture fails",
            config.name
        );
    }
    let (keys, run) = forward(&context, config);
    println!(
        "OBJECT-CAPTURE-SUMMARY set=forward config={} keys={keys}/{} compared={run} skipped=0",
        config.name,
        PipelineKey::ALL.len()
    );
    assert_eq!(keys, PipelineKey::ALL.len(), "every key is drawn");
    assert_eq!(run, captures_per_config());
}

macro_rules! forward_tests {
    ($($test:ident: $index:expr;)*) => {$(
        #[test]
        fn $test() {
            forward_set($index);
        }
    )*};
}

forward_tests! {
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_in_srgb_at_one_sample: 0;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_in_srgb_at_four_samples: 1;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_in_hdr_at_one_sample: 2;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_in_hdr_at_four_samples: 3;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_velocity_at_one_sample: 4;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_velocity_at_four_samples: 5;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_ssao_at_one_sample: 6;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_ssao_and_velocity_at_four_samples: 7;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_reflection_at_one_sample: 8;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_reflection_at_four_samples: 9;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_reflection_and_velocity_at_one_sample: 10;
    objects_match_the_fixed_pipeline_byte_for_byte_for_every_key_with_reflection_and_velocity_at_four_samples: 11;
}

/// The command counts of the fixed path over the rig's scene, drawn into a counter.
pub(in crate::object) fn counted(rig: &Rig<'_>, legacy: &Legacy) -> (Counted, Recorded) {
    let mut counted = Counted::default();
    let recorded = legacy.draw(&mut counted, &rig.gpu, 0);
    (counted, recorded)
}
