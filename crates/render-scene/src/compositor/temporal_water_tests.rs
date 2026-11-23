//! Helpers that draw water over the scene of `taa_end_to_end_tests` through the inner renderer of
//! a temporal `ConfiguredFrameRenderer`: the scene goes into the render-extent targets, then a
//! `Load` opaque pass and the water pass run through `encode_frame`. The one check here refuses a
//! scene whose water differs from the installed water.
use super::taa_end_to_end_tests::{OUTPUT, Scene, context};
use crate::{
    CameraPose, SceneView, Viewport,
    compositor::{FrameRenderer, attachment},
    device::GpuContext,
    frame::FrameUniform,
};
use wgpu::util::DeviceExt;
use worldrunner_render_plan::{SceneCamera, SceneWaterBody, SceneWaterSettings, SceneWaterSurface};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, DrawContext, FeatureDeclaration, FrameConfig, FrameError,
    PassDeclaration, PassSlot, PrepareContext, PreparedFeature, Readback, ResourceRef, ToneMap,
};

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
pub(super) fn frame(size: u32) -> FrameUniform {
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

/// A renderer of `size` square texels without temporal reconstruction and with the water body
/// installed; `far` moves the body out of view.
fn sized(context: &GpuContext, size: u32, far: bool) -> FrameRenderer {
    let mut renderer =
        FrameRenderer::hdr(context, [size, size], OUTPUT, 1, ToneMap::Reinhard, None).unwrap();
    renderer.install_water(context, body(far)).unwrap();
    renderer
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
        label: Some("water camera"),
        contents: &contents,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("water camera"),
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

/// Draws the scene in the opaque pass, then the water pass over it, in frame slot `slot` at
/// `seconds` of water time.
pub(super) fn submit_to(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    (seconds, slot): (f32, usize),
    with_scene: bool,
) {
    let device = context.device();
    renderer.select_frame_slot(slot);
    renderer.set_frame_time(std::time::Duration::from_secs_f32(seconds));
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
                None,
                &mut features,
                &(prepared, None),
            )?;
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

#[test]
fn a_scene_whose_water_differs_from_the_installed_water_is_refused_by_name() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let renderer = sized(&context, 32, false);
    renderer
        .validate_water(&[body(false)])
        .expect("the installed body matches");
    for scene in [&[][..], &[body(true)][..]] {
        assert_eq!(
            renderer.validate_water(scene),
            Err(FrameError::WaterSettings(
                "the scene's water bodies differ from the water the renderer was configured with"
            )),
            "{} scene bodies",
            scene.len()
        );
    }
}
