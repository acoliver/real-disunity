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
use crate::draw::{DrawCount, SceneGpu, terrain_paths};
use crate::dynamic::capture_tests::{
    Attachments, CONFIGS, Config, SIZE, build, context, copy_results, digest, fill, read_results,
    supported,
};
use crate::frame::FrameUniform;
use crate::renderer::ScenePipelines;
use crate::renderer::depth::DepthMode;

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

    pub(super) fn scene(self) -> RenderScene {
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
    render_depth(
        context,
        target,
        (DepthMode::Independent, None),
        before,
        draw,
    )
}

/// Draws the pass `seed` records, which the next pass of the frame loads its depth and stencil
/// from, as the metadata prepass does for the forward pass.
pub(super) type Seed<'a> = &'a dyn Fn(&mut RenderPass<'_>);

/// [`render`] with the depth representation of `mode`. Modes other than
/// [`DepthMode::Independent`] draw into a combined depth and stencil attachment, which the
/// result reads back after the colour attachments: the depth plane, then the stencil plane.
/// `seed` records a pass ahead of the drawn one that clears the depth and stencil and leaves
/// them for the drawn pass to load.
pub(super) fn render_depth(
    context: &GpuContext,
    target: Target<'_>,
    (mode, seed): (DepthMode, Option<Seed<'_>>),
    before: impl FnOnce(&mut CommandEncoder),
    draw: impl FnOnce(&mut RenderPass<'_>),
) -> Vec<Vec<u8>> {
    let device = context.device();
    let attachments = Attachments::new(device, target.formats, target.samples);
    let combined = (mode != DepthMode::Independent).then(|| {
        assert_eq!(target.samples, 1, "paired depth is single sample");
        combined_texture(device)
    });
    let combined_view = combined
        .as_ref()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
    let resolve_views: Vec<_> = attachments
        .resolved
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let mut encoder = context.submission().begin(device).unwrap();
    before(&mut encoder);
    let seeded = seed.is_some();
    if let (Some(seed), Some(view)) = (seed, combined_view.as_ref()) {
        seed_pass((device, &mut encoder), target, view, seed);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain capture"),
            color_attachments: &colors(target, (&attachments.views, &resolve_views)),
            depth_stencil_attachment: Some(combined_view.as_ref().map_or_else(
                || wgpu::RenderPassDepthStencilAttachment {
                    view: &attachments.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                },
                |view| depth_attachment(view, seeded),
            )),
            ..wgpu::RenderPassDescriptor::default()
        });
        draw(&mut pass);
    }
    let mut staging = copy_results(device, &mut encoder, attachments.results());
    if let Some(texture) = &combined {
        for aspect in [
            wgpu::TextureAspect::DepthOnly,
            wgpu::TextureAspect::StencilOnly,
        ] {
            staging.push(copy_plane(device, &mut encoder, texture, aspect));
        }
    }
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, staging)
}

/// The combined depth and stencil attachment of the paired depth modes.
fn combined_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain capture depth and stencil"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Depth32FloatStencil8,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Records `seed` into a pass that clears `depth` and leaves it for the next pass to load.
fn seed_pass(
    (device, encoder): (&wgpu::Device, &mut CommandEncoder),
    target: Target<'_>,
    depth: &wgpu::TextureView,
    seed: Seed<'_>,
) {
    let scratch = Attachments::new(device, target.formats, 1);
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("terrain capture seed"),
        color_attachments: &colors(target, (&scratch.views, &[])),
        depth_stencil_attachment: Some(depth_attachment(depth, false)),
        ..wgpu::RenderPassDescriptor::default()
    });
    seed(&mut pass);
}

/// The colour attachments of a pass over `views`, cleared to [`fill`].
fn colors<'v>(
    target: Target<'_>,
    (views, resolves): (&'v [wgpu::TextureView], &'v [wgpu::TextureView]),
) -> Vec<Option<wgpu::RenderPassColorAttachment<'v>>> {
    views
        .iter()
        .zip(target.formats)
        .enumerate()
        .map(|(index, (view, &format))| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: resolves.get(index),
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
        .collect()
}

/// The combined depth and stencil attachment of a pass that keeps both, cleared to the far plane
/// and zero or loaded from the pass that seeded them.
fn depth_attachment(
    view: &wgpu::TextureView,
    load: bool,
) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: if load {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(1.0)
            },
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: Some(wgpu::Operations {
            load: if load {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(0)
            },
            store: wgpu::StoreOp::Store,
        }),
    }
}

/// Copies one aspect of the combined depth and stencil texture into a mappable buffer, as
/// [`copy_results`] does for colour.
fn copy_plane(
    device: &wgpu::Device,
    encoder: &mut CommandEncoder,
    texture: &wgpu::Texture,
    aspect: wgpu::TextureAspect,
) -> (wgpu::Buffer, usize, usize) {
    let texel = if aspect == wgpu::TextureAspect::DepthOnly {
        4
    } else {
        1
    };
    let row = SIZE * texel;
    let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture plane readback"),
        size: u64::from(padded * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect,
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
///
/// The depth representation is the one `case.pipelines` were built for. In the paired modes the
/// depth and stencil planes are compared along with the colour attachments, and each mode's
/// effect on them is checked: shared and prepass terrain writes depth, prepass terrain marks its
/// winners in the stencil, and reuse terrain draws exactly those winners, leaves the depth as
/// the prepass wrote it and consumes every stencil mark.
pub(super) fn capture_against(rig: &Rig<'_>, case: &Case<'_>, legacy: &Legacy) -> Captured {
    let frame = Frame::new(rig, case);
    let mode = case.pipelines.depth;
    let mut scene_count = DrawCount::default();
    let recorded = terrain_paths::recorded();
    let scene = frame.render(|pass| {
        frame.bind(pass);
        let drawn = rig.gpu.draw_color(pass, case.pipelines, case.shadow);
        assert_eq!(drawn.sky, DrawCount::default(), "the fixture draws no sky");
        scene_count = drawn.terrain;
    });
    assert_recorded_by_runtime(case, recorded);
    let mut fixed_count = DrawCount::default();
    let fixed = frame.render(|pass| {
        frame.bind(pass);
        if mode != DepthMode::Independent {
            pass.set_stencil_reference(1);
        }
        pass.set_bind_group(0, &rig.gpu.legacy_frame_group(), &[]);
        fixed_count = legacy.draw(pass, &rig.gpu, case.shadow);
    });
    let untouched = frame.render(|_| {});
    compare(case, [&scene, &fixed, &untouched]);
    assert_eq!(
        scene_count, fixed_count,
        "{} {}: draw counts differ",
        case.name, case.source
    );
    report(rig.context, case, &scene);
    Captured {
        images: scene,
        scene: scene_count,
    }
}

/// What a capture draws around the terrain: the group bound at set three, and the prepass that
/// a reuse draw loads its depth and stencil from.
struct Frame<'a> {
    rig: &'a Rig<'a>,
    case: &'a Case<'a>,
    prepass: Option<Legacy>,
}

impl<'a> Frame<'a> {
    fn new(rig: &'a Rig<'a>, case: &'a Case<'a>) -> Self {
        // The prepass the forward draw reuses: the fixed prepass terrain over the same frame.
        let prepass = (case.pipelines.depth == DepthMode::Reuse).then(|| {
            let selected = rig.selected(&case.pipelines.for_depth(DepthMode::Prepass));
            Legacy::new(rig.context, &rig.gpu, &selected)
        });
        Self { rig, case, prepass }
    }

    fn bind(&self, pass: &mut RenderPass<'_>) {
        if let Some(screen) = self.case.screen {
            pass.set_bind_group(3, screen, &[]);
        }
    }

    /// The pass ahead of the drawn one, which leaves the prepass's depth and stencil.
    fn seed(&self, pass: &mut RenderPass<'_>, prepass: &Legacy) {
        self.bind(pass);
        pass.set_stencil_reference(1);
        pass.set_bind_group(0, &self.rig.gpu.legacy_frame_group(), &[]);
        prepass.draw(pass, &self.rig.gpu, self.case.shadow);
    }

    /// Renders one pass over fresh attachments, preceded by the seed when there is a prepass.
    fn render(&self, draw: impl FnOnce(&mut RenderPass<'_>)) -> Vec<Vec<u8>> {
        let seed = |pass: &mut RenderPass<'_>| {
            let prepass = self.prepass.as_ref().expect("only a reuse capture seeds");
            self.seed(pass, prepass);
        };
        let seeded: Option<Seed<'_>> = self.prepass.is_some().then_some(&seed);
        render_depth(
            self.rig.context,
            self.case.target,
            (self.case.pipelines.depth, seeded),
            |encoder| self.rig.prepare(encoder),
            draw,
        )
    }
}

/// Checks the scene's attachments against the fixed path's, byte for byte, that terrain changed
/// the colour from an untouched render, and the planes the depth mode leaves.
fn compare(case: &Case<'_>, [scene, fixed, untouched]: [&[Vec<u8>]; 3]) {
    let mode = case.pipelines.depth;
    let colors = case.target.formats.len();
    let planes = if mode == DepthMode::Independent { 0 } else { 2 };
    assert_eq!(scene.len(), colors + planes);
    for (index, ((scene, fixed), untouched)) in scene.iter().zip(fixed).zip(untouched).enumerate() {
        assert!(
            scene == fixed,
            "{} {} {mode:?}: {} differs from the fixed pipeline's output",
            case.name,
            case.source,
            attachment_name(index, colors)
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
    if planes > 0 {
        check_planes(
            case,
            (&scene[colors], &scene[colors + 1]),
            &untouched[colors..],
        );
    }
}

/// The scene's terrain draw went through the program runtime, once, and the fixed terrain
/// pipelines of its context were neither used nor built. Without this a fixed-path fallback
/// would draw the same bytes as the frozen copy and pass.
fn assert_recorded_by_runtime(case: &Case<'_>, before: terrain_paths::Recorded) {
    let after = terrain_paths::recorded();
    let name = format!("{} {} {:?}", case.name, case.source, case.pipelines.depth);
    assert_eq!(
        after.runtime,
        before.runtime + 1,
        "{name}: terrain was not recorded through the program runtime"
    );
    assert_eq!(
        after.fixed, before.fixed,
        "{name}: terrain fell back to the fixed path"
    );
    assert!(
        !case.pipelines.fixed_terrain_built(),
        "{name}: a fixed terrain pipeline was built"
    );
    let stats = case.pipelines.terrain_runtime_stats();
    assert!(
        stats.pipelines_built > 0 && stats.sets_prepared > 0,
        "{name}: the context's runtime prepared no terrain pipeline or set: {stats:?}"
    );
}

fn attachment_name(index: usize, colors: usize) -> String {
    match index.checked_sub(colors) {
        None => format!("attachment {index}"),
        Some(0) => "depth plane".to_owned(),
        Some(_) => "stencil plane".to_owned(),
    }
}

/// What the paired depth modes leave in the depth and stencil planes. `untouched` is the planes
/// of a render that drew no terrain in the drawn pass: cleared, or as the prepass left them.
fn check_planes(case: &Case<'_>, (depth, stencil): (&[u8], &[u8]), untouched: &[Vec<u8>]) {
    let marked = |plane: &[u8]| plane.iter().filter(|&&texel| texel != 0).count();
    let name = format!("{} {}", case.name, case.source);
    match case.pipelines.depth {
        DepthMode::Independent => {}
        DepthMode::Shared => {
            assert!(
                depth != untouched[0],
                "{name}: shared terrain wrote no depth"
            );
            assert_eq!(
                marked(stencil),
                0,
                "{name}: shared terrain touched the stencil"
            );
        }
        DepthMode::Prepass => {
            assert!(
                depth != untouched[0],
                "{name}: prepass terrain wrote no depth"
            );
            assert!(
                marked(stencil) > 0,
                "{name}: prepass terrain marked no winner"
            );
        }
        DepthMode::Reuse => {
            assert!(
                marked(&untouched[1]) > 0,
                "{name}: the prepass marked no winner"
            );
            assert_eq!(
                depth, untouched[0],
                "{name}: reuse terrain changed the prepass depth"
            );
            assert_eq!(
                marked(stencil),
                0,
                "{name}: reuse terrain left winners unconsumed"
            );
        }
    }
}

fn report(context: &GpuContext, case: &Case<'_>, images: &[Vec<u8>]) {
    let adapter = context.adapter().get_info().name;
    let mode = format!("{:?}", case.pipelines.depth).to_lowercase();
    let colors = case.target.formats.len();
    for (index, image) in images.iter().enumerate() {
        let attachment = match index.checked_sub(colors) {
            None => index.to_string(),
            Some(0) => "depth".to_owned(),
            Some(_) => "stencil".to_owned(),
        };
        println!(
            "TERRAIN-CAPTURE adapter={adapter:?} config={} depth={mode} source={} attachment={attachment} bytes={} hash={:016x}",
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

/// Captures `source` in every forward target set of the dynamic quads' harness that `mode`
/// applies to, returning how many captures ran and how many were skipped. The paired depth modes
/// apply to the single-sample sets only.
fn forward_sets(source: Source, mode: DepthMode) -> (usize, usize) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut run, mut skipped) = (0, 0);
    for config in CONFIGS
        .iter()
        .filter(|config| mode == DepthMode::Independent || config.samples == 1)
    {
        if !supported(&context, config) || !drawable(&context, source) {
            println!(
                "TERRAIN-CAPTURE config={} depth={mode:?} source={} skipped: format, samples or limits unsupported",
                config.name,
                source.name()
            );
            skipped += 1;
            continue;
        }
        forward(&context, config, (source, mode));
        run += 1;
    }
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=forward depth={mode:?} source={} compared={run} skipped={skipped}",
        source.name()
    );
    (run, skipped)
}

/// The forward target sets `mode` applies to.
fn applicable(mode: DepthMode) -> usize {
    CONFIGS
        .iter()
        .filter(|config| mode == DepthMode::Independent || config.samples == 1)
        .count()
}

/// One configuration of the 12 forward target sets with one surface source, drawn with the
/// pipelines of one depth mode.
fn forward(context: &GpuContext, config: &Config, (source, mode): (Source, DepthMode)) -> Captured {
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
            pipelines: &built.pipelines.for_depth(mode),
            screen: built.screen.as_ref(),
            shadow: None,
        },
    )
}

macro_rules! forward_tests {
    ($($test:ident: $source:expr, $mode:expr;)*) => {$(
        #[test]
        fn $test() {
            let (run, skipped) = forward_sets($source, $mode);
            assert_eq!(run + skipped, applicable($mode));
            if matches!($source, Source::Plain | Source::Pages) {
                assert!(run > 0, "no forward target set could run");
            }
        }
    )*};
}

forward_tests! {
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set:
        Source::Plain, DepthMode::Independent;
    page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set:
        Source::Pages, DepthMode::Independent;
    layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set:
        Source::Layered, DepthMode::Independent;
    layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_forward_target_set:
        Source::LayeredPages, DepthMode::Independent;
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_depth_reuse:
        Source::Plain, DepthMode::Reuse;
    page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_depth_reuse:
        Source::Pages, DepthMode::Reuse;
    layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_depth_reuse:
        Source::Layered, DepthMode::Reuse;
    layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_depth_reuse:
        Source::LayeredPages, DepthMode::Reuse;
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_shared_depth:
        Source::Plain, DepthMode::Shared;
    page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_shared_depth:
        Source::Pages, DepthMode::Shared;
    layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_shared_depth:
        Source::Layered, DepthMode::Shared;
    layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_with_shared_depth:
        Source::LayeredPages, DepthMode::Shared;
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_every_single_sample_set_in_the_prepass:
        Source::Plain, DepthMode::Prepass;
}
