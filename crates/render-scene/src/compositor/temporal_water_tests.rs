//! Water as the producer of the temporal reactive mask, through the inner renderer of a
//! `ConfiguredFrameRenderer` with `UpscaleMode::Temporal` (decision Q6: the jitter gap of the
//! configured path is not part of this check). The scene of `taa_end_to_end_tests` is drawn into the
//! render-extent targets, then a `Load` opaque pass and the water pass run through `encode_frame`.
use super::taa_end_to_end_tests::{OUTPUT, Scene, context};
use crate::{
    CameraPose, SceneView, Viewport,
    compositor::{ConfiguredFrameRenderer, FrameRenderer, RendererConfiguration, attachment},
    device::GpuContext,
    frame::FrameUniform,
};
use wgpu::util::DeviceExt;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    SceneCamera, SceneWaterBody, SceneWaterSettings, SceneWaterSurface,
    post::{RenderScale, UpscaleMode, UpscaleSettings},
};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, DrawContext, FeatureDeclaration, FrameConfig, FrameError,
    HdrPostSettings, PassDeclaration, PassSlot, PostResolve, PrepareContext, PreparedFeature,
    Readback, ResourceRef, ToneMap,
};

const OUTPUT_EXTENT: [u32; 2] = [64, 64];
/// The policy constant of the water producer (decision Q5), in 8 bit units.
const REACTIVE_UNORM: [u8; 2] = [229, 230];

/// `far` moves the surface 1 km east, out of the view, which gives the dry scene of a frame.
pub(super) fn body(far: bool) -> SceneWaterBody {
    SceneWaterBody {
        material: SceneWaterSettings::STOCK,
        surface: SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: 0,
        east_min_millimeters: if far { 1_000_000 } else { -30_000 },
        north_min_millimeters: -9_500,
        east_extent_millimeters: 60_000,
        north_extent_millimeters: 18_000,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
    }
}

/// A camera 1.7 m above the water plane looking along +y: the lower part of the image is water.
fn frame(size: u32) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: size,
            height: size,
        },
    )
    .unwrap();
    FrameUniform::new(&view, None).unwrap()
}

pub(super) fn temporal(
    context: &GpuContext,
    scale_micro: u32,
    producer: bool,
) -> ConfiguredFrameRenderer {
    temporal_with(context, scale_micro, (producer, false))
}

pub(super) fn temporal_with(
    context: &GpuContext,
    scale_micro: u32,
    water: (bool, bool),
) -> ConfiguredFrameRenderer {
    let render = RenderScale::new(scale_micro)
        .unwrap()
        .extent(OUTPUT_EXTENT)
        .unwrap();
    let mut settings = RendererConfiguration::legacy(OUTPUT_EXTENT, OUTPUT);
    settings.frame = FrameConfig::new(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent(render);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post = HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    let mut renderer =
        ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap();
    install(context, renderer.renderer_mut(), water);
    renderer
}

/// A renderer without temporal reconstruction, so the frame carries no mask.
fn plain(context: &GpuContext, producer: bool) -> FrameRenderer {
    sized(context, 32, (producer, false))
}

/// A renderer of `size` square texels without temporal reconstruction.
pub(super) fn sized(context: &GpuContext, size: u32, water: (bool, bool)) -> FrameRenderer {
    let mut renderer =
        FrameRenderer::hdr(context, [size, size], OUTPUT, 1, ToneMap::Reinhard, None).unwrap();
    install(context, &mut renderer, water);
    renderer
}

fn install(context: &GpuContext, renderer: &mut FrameRenderer, (producer, far): (bool, bool)) {
    renderer.install_water(context.device(), body(far)).unwrap();
    renderer.set_water_reactive_mask(producer);
}

/// The scene of `taa_end_to_end_tests` as the producer of the opaque pass.
struct SceneDraw<'a> {
    scene: &'a Scene,
    group: wgpu::BindGroup,
}

/// The camera group of the scene pipeline for `view_projection`.
fn camera_group(
    scene: &Scene,
    device: &wgpu::Device,
    view_projection: glam::Mat4,
) -> wgpu::BindGroup {
    let contents: Vec<u8> = view_projection
        .to_cols_array()
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("water mask camera"),
        contents: &contents,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("water mask camera"),
        layout: &scene.pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

impl PreparedFeature for SceneDraw<'_> {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, _: &mut PrepareContext<'_>) {}
    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.scene.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..15, 0..1);
    }
}

/// Draws the scene in the opaque pass, then the water pass over it, at `seconds` of water time.
fn submit(context: &GpuContext, renderer: &mut FrameRenderer, seconds: f32, with_scene: bool) {
    submit_to(context, renderer, (seconds, 0), (with_scene, None));
}

/// `submit` in frame slot `slot`, optionally resolving the frame into `output`.
pub(super) fn submit_to(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    (seconds, slot): (f32, usize),
    (with_scene, output): (bool, Option<&wgpu::TextureView>),
) {
    let device = context.device();
    renderer.select_frame_slot(slot);
    renderer.set_frame_time(seconds).unwrap();
    let prepared = renderer
        .prepare_uniform(frame(renderer.targets().config().size[0]))
        .unwrap();
    let scene = Scene::new(device, 1);
    let mut draw = SceneDraw {
        group: camera_group(&scene, device, prepared.view_projection),
        scene: &scene,
    };
    let transaction = context
        .submission()
        .prepare(device, |encoder, _| {
            let targets = renderer.targets();
            let color =
                AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture())?;
            let depth =
                AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture())?;
            let mut opaque = PassDeclaration::new(PassSlot::Opaque);
            opaque.color = Some(attachment(color.resource(), &color, false));
            opaque.depth = Some(attachment(depth.resource(), &depth, true));
            let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
            declaration.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
            declaration.passes[0].depth_format = Some(wgpu::TextureFormat::Depth32Float);
            let mut features = Vec::new();
            if with_scene {
                features.push(BorrowedFeature {
                    declaration,
                    prepared: &mut draw,
                });
            }
            renderer.encode_frame(
                device,
                encoder,
                (&[opaque], &[color, depth]),
                &mut features,
                (prepared, None),
            )?;
            if let Some(output) = output {
                renderer.encode_output(encoder, output).unwrap();
            }
            Ok::<_, FrameError>(())
        })
        .unwrap();
    transaction.submit().unwrap();
}

pub(super) fn scene_bytes(context: &GpuContext, renderer: &FrameRenderer) -> Vec<u8> {
    let texture = renderer.targets().scene().texture().clone();
    Readback::new(
        context.device(),
        FrameConfig::new(renderer.targets().config().size, texture.format(), 1),
        1 << 20,
    )
    .unwrap()
    .read(context.device(), context.submission(), &texture)
    .unwrap()
}

fn read_mask(context: &GpuContext, texture: &wgpu::Texture) -> Vec<[u8; 2]> {
    let device = context.device();
    let (width, height) = (texture.width(), texture.height());
    let pitch = (width * 2).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mask readback"),
        size: u64::from(pitch * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = context.submission().begin(device).unwrap();
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    encoder.finish().submit().unwrap();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let row = usize::try_from(width).unwrap() * 2;
    let pitch = usize::try_from(pitch).unwrap();
    data.chunks_exact(pitch)
        .flat_map(|line| line[..row].chunks_exact(2))
        .map(|texel| [texel[0], texel[1]])
        .collect()
}

#[test]
fn water_writes_the_policy_constant_under_its_surface_and_zero_elsewhere() {
    let context = context();
    for scale in [500_000, 670_000, 770_000] {
        let mut with = temporal(&context, scale, true);
        let mut without = temporal(&context, scale, false);
        submit(&context, with.renderer_mut(), 0.5, true);
        submit(&context, without.renderer_mut(), 0.5, true);
        let mask = read_mask(
            &context,
            with.targets().temporal_mask_texture().expect("a mask"),
        );
        let silent = read_mask(
            &context,
            without.targets().temporal_mask_texture().expect("a mask"),
        );
        let lit = scene_bytes(&context, with.renderer_mut());
        // Scene colour with the water pass skipped marks the covered texels.
        let mut bare = temporal_with(&context, scale, (false, true));
        submit(&context, bare.renderer_mut(), 0.5, true);
        let dry = scene_bytes(&context, bare.renderer_mut());
        let covered: Vec<bool> = lit
            .chunks_exact(8)
            .zip(dry.chunks_exact(8))
            .map(|(wet, dry)| wet != dry)
            .collect();
        let under = covered.iter().filter(|&&c| c).count();
        assert!(
            under > 0 && under < covered.len(),
            "{scale}: {under} of {} texels are water",
            covered.len()
        );
        for (index, ((value, &wet), off)) in mask.iter().zip(&covered).zip(&silent).enumerate() {
            assert_eq!(*off, [0, 0], "{scale} {index}: the producer is off");
            assert_eq!(value[1], 0, "{scale} {index}: water writes no composition");
            if wet {
                assert!(
                    REACTIVE_UNORM.contains(&value[0]),
                    "{scale} {index}: {value:?} under the surface"
                );
            } else {
                assert_eq!(value[0], 0, "{scale} {index}: opaque texels stay untouched");
            }
        }
    }
}

#[test]
fn the_producer_leaves_the_water_and_scene_colour_unchanged() {
    let context = context();
    for scale in [500_000, 670_000] {
        let mut with = temporal(&context, scale, true);
        let mut without = temporal(&context, scale, false);
        for index in 0..3 {
            let seconds = 0.25 * f32::from(u8::try_from(index).unwrap());
            submit(&context, with.renderer_mut(), seconds, true);
            submit(&context, without.renderer_mut(), seconds, true);
            assert_eq!(
                scene_bytes(&context, with.renderer_mut()),
                scene_bytes(&context, without.renderer_mut()),
                "{scale} frame {index}: same code on one device"
            );
        }
    }
}

#[test]
fn a_frame_without_a_mask_draws_the_water_as_before() {
    let context = context();
    let mut on = plain(&context, true);
    let mut off = plain(&context, false);
    assert!(on.targets().temporal_mask().is_none());
    submit(&context, &mut on, 0.5, true);
    submit(&context, &mut off, 0.5, true);
    assert_eq!(scene_bytes(&context, &on), scene_bytes(&context, &off));
}

/// The slots the water feature adds to a plan of one opaque pass, and its feature pass count.
fn planned(
    context: &GpuContext,
    renderer: &FrameRenderer,
    with_mask: bool,
) -> (Vec<PassSlot>, usize) {
    let targets = renderer.targets();
    let color =
        AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture()).unwrap();
    let depth =
        AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture()).unwrap();
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(attachment(color.resource(), &color, false));
    opaque.depth = Some(attachment(depth.resource(), &depth, true));
    let mut passes = vec![opaque];
    let mut views = vec![color, depth];
    let mask = with_mask.then(|| targets.temporal_mask_texture().expect("a mask"));
    let declaration = renderer
        .water
        .as_ref()
        .unwrap()
        .feature(context.device(), &frame(32), (0.0, 0), (None, mask))
        .schedule(&mut passes, &mut views)
        .unwrap();
    let slots = passes.iter().map(|pass| pass.slot).collect();
    (slots, declaration.passes.len())
}

#[test]
fn the_mask_pass_follows_the_water_pass_only_with_a_mask_and_the_producer() {
    let context = context();
    let mut with = temporal(&context, 670_000, true);
    let mut off = temporal(&context, 670_000, false);
    let water = [PassSlot::Opaque, PassSlot::TransparentWater];
    let masked = [
        PassSlot::Opaque,
        PassSlot::TransparentWater,
        PassSlot::TransparentWaterMask,
    ];
    assert_eq!(
        planned(&context, with.renderer_mut(), true),
        (masked.to_vec(), 2)
    );
    assert_eq!(
        planned(&context, with.renderer_mut(), false),
        (water.to_vec(), 1)
    );
    assert_eq!(
        planned(&context, off.renderer_mut(), true),
        (water.to_vec(), 1)
    );
    let one_sample = plain(&context, true);
    assert_eq!(planned(&context, &one_sample, false), (water.to_vec(), 1));
}
