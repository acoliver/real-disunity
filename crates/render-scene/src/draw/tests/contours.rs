//! Visible-ID differentials through the stock sorted executor, with exact sample readback.
use super::*;
use crate::compositor::FrameRenderer;
use crate::test_support::upload_scene;
use worldrunner_render_plan::{
    SceneContourStyle, SceneContours, TexelFormat, TextureFilter, TextureSampler,
};
use worldrunner_render_post::{AttachmentView, ResourceRef, ToneMap};

mod capture_tests;
mod retained_tests;

const SIZE: [u32; 2] = [32, 24];

#[derive(Clone, Copy)]
enum Shape {
    Quad,
    Cutout,
    Occluded,
    Hidden,
}

fn source(id: u8, shape: Shape, prepass: bool) -> RenderScene {
    let hole = matches!(shape, Shape::Cutout);
    let occluder = matches!(shape, Shape::Occluded | Shape::Hidden);
    let mut styles = vec![None; 256];
    styles[255] = Some(SceneContourStyle::default());
    let mut parts = effect_parts();
    parts.ssao = prepass.then(crate::ssr::neutral_ao);
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_contours(SceneContours::new([1920, 1080], styles.clone()).unwrap())
        .unwrap();
    let mut vertices = quad().lods()[0].vertices().to_vec();
    for v in &mut vertices {
        v.position_millimeters[0] /= 2;
        v.position_millimeters[1] /= 2;
        v.position_millimeters[2] = 500;
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let object = SceneObject::new(
        identity(),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_contour_id(id);
    let mut objects = vec![object];
    if occluder {
        let hidden = matches!(shape, Shape::Hidden);
        objects.push(SceneObject::new(
            SceneTransform {
                position_millimeters: if hidden { [0, 0, -125] } else { [250, 0, -125] },
                scale_millionths: if hidden { 1_000_000 } else { 500_000 },
                ..identity()
            },
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(1),
        ));
    }
    RenderScene::new(SceneParts {
        resources: resources(hole),
        geometry: SceneGeometry::new(vec![mesh], objects).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

/// The quad scene with contour ID `id` and styles for IDs 1, 2 and 255, with the depth prepass
/// when `prepass`.
fn source_with_ids(id: u8, prepass: bool) -> RenderScene {
    let source = source(0, Shape::Quad, prepass);
    let mut styles = vec![None; 256];
    for selected in [1, 2, 255] {
        styles[selected] = Some(SceneContourStyle::default());
    }
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_contours(SceneContours::new([1920, 1080], styles).unwrap())
        .unwrap();
    source
        .with_objects_and_effects(
            source
                .geometry()
                .objects()
                .iter()
                .map(|object| object.with_contour_id(id))
                .collect(),
            Some(effects),
        )
        .unwrap()
}

fn resources(hole: bool) -> SceneResources {
    let mut pixels: Vec<_> = (0..16).flat_map(|_| [255, 0, 0, 255]).collect();
    if hole {
        for y in 1..3 {
            for x in 1..3 {
                pixels[(y * 4 + x) * 4 + 3] = 0;
            }
        }
    }
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 4,
        height: 4,
        mips: 1,
        bytes: pixels.into(),
    }
    .with_sampler(TextureSampler {
        filter: TextureFilter::Point,
        ..Default::default()
    });
    let occluder = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap();
    SceneResources::new(
        vec![texture, white_texture()],
        vec![
            material(
                if hole {
                    SceneAlphaMode::Mask {
                        cutoff_micro: 500_000,
                    }
                } else {
                    SceneAlphaMode::Opaque
                },
                true,
            ),
            occluder,
        ],
    )
    .unwrap()
}

struct Rig {
    renderer: FrameRenderer,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    frame: FrameUniform,
}

struct Image {
    mask: Option<Vec<u8>>,
    color: Vec<u8>,
}

impl Rig {
    fn new(context: &GpuContext, source: &RenderScene, samples: u32) -> Self {
        let device = context.device();
        let pipelines = if source.effects().unwrap().ssao().is_some() {
            ScenePipelines::with_ssao(device, TextureFormat::Rgba16Float, samples).unwrap()
        } else {
            ScenePipelines::new(device, TextureFormat::Rgba16Float, samples).unwrap()
        };
        let renderer = FrameRenderer::hdr(
            context,
            SIZE,
            COLOR_FORMAT,
            samples,
            ToneMap::Reinhard,
            source.effects(),
        )
        .unwrap();
        let gpu = upload_scene(device, context.submission(), source, &pipelines).unwrap();
        let view = SceneView::new(
            pose(),
            camera(),
            Viewport {
                width: SIZE[0],
                height: SIZE[1],
            },
        )
        .unwrap();
        let mut frame = FrameUniform::new(&view, source.effects()).unwrap();
        frame.view_projection = glam::Mat4::IDENTITY;
        frame.view = glam::Mat4::IDENTITY;
        Self {
            renderer,
            gpu,
            pipelines,
            frame,
        }
    }

    fn encode(
        &mut self,
        context: &GpuContext,
        encoder: &mut CommandEncoder,
    ) -> (Option<Texture>, Texture) {
        let device = context.device();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.renderer.targets().depth().texture(),
        )
        .unwrap();
        self.renderer
            .encoding()
            .encode_scene(
                device,
                encoder,
                (&mut self.gpu, &self.pipelines),
                (&color, &depth),
                self.frame,
            )
            .unwrap();
        let mask = self
            .renderer
            .encode_contour_capture(device, encoder)
            .unwrap();
        let output = worldrunner_render_post::color_texture(
            device,
            "contour differential output",
            self.renderer.targets().config().size,
            COLOR_FORMAT,
        );
        self.renderer
            .encode_output(
                encoder,
                &output.create_view(&TextureViewDescriptor::default()),
            )
            .unwrap();
        (mask, output)
    }

    fn draw(&mut self, context: &GpuContext) -> Image {
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let (mask, output) = self.encode(context, &mut encoder);
        encoder.finish().submit().unwrap();
        Image {
            mask: mask.as_ref().map(|t| read(context, t)),
            color: read(context, &output),
        }
    }
}

fn read(context: &GpuContext, texture: &Texture) -> Vec<u8> {
    let row = texture.width() * texture.format().block_copy_size(None).unwrap();
    let stride = row.div_ceil(256) * 256;
    let buffer = context.device().create_buffer(&BufferDescriptor {
        label: Some("contour readback"),
        size: u64::from(stride * texture.height()),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = context
        .device()
        .create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    context.queue().submit([encoder.finish()]);
    map_pixels(context.device(), &buffer)
        .chunks_exact(stride as usize)
        .flat_map(|r| r[..row as usize].iter().copied())
        .collect()
}

#[test]
fn gpu_contour_quad_cutout_and_occlusion_exact_masks_with_and_without_the_prepass() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for prepass in [false, true] {
        for shape in [Shape::Quad, Shape::Cutout, Shape::Occluded, Shape::Hidden] {
            let hole = matches!(shape, Shape::Cutout);
            let occluder = matches!(shape, Shape::Occluded);
            let source = source(255, shape, prepass);
            let mut rig = Rig::new(&context, &source, 1);
            let image = rig.draw(&context);
            let mask = image.mask.unwrap();
            assert_eq!(
                rig.renderer.contour_statistics(),
                crate::ContourStatistics {
                    passes: 1,
                    targets: 1,
                    samples: 1
                }
            );
            assert_eq!(mask.len(), (SIZE[0] * SIZE[1]) as usize);
            if hole {
                assert_eq!(mask[(12 * SIZE[0] + 16) as usize], 0, "hole centre");
                assert!(
                    mask.contains(&255),
                    "the cutout must retain visible coverage"
                );
            }
            for y in 0..SIZE[1] {
                for x in 0..SIZE[0] {
                    let covered = (8..24).contains(&x) && (6..18).contains(&y);
                    let hidden = matches!(shape, Shape::Hidden)
                        || (occluder && (16..24).contains(&x) && (9..15).contains(&y));
                    let index = (y * SIZE[0] + x) as usize;
                    let pixel = &image.color[index * 4..index * 4 + 4];
                    let visible = pixel[0] > pixel[1] && pixel[0] > pixel[2];
                    if !hole {
                        assert_eq!(visible, covered && !hidden, "colour x={x} y={y}");
                    }
                    assert_eq!(
                        mask[index],
                        if visible { 255 } else { 0 },
                        "x={x} y={y} hole={hole} hidden={occluder} prepass={prepass}"
                    );
                }
            }
        }
    }
}

#[test]
fn gpu_contour_unused_and_active_to_empty_are_byte_identical_and_absent() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for prepass in [false, true] {
        let disabled = source(0, Shape::Quad, prepass);
        let mut rig = Rig::new(&context, &disabled, 1);
        let baseline = rig.draw(&context);
        assert!(baseline.mask.is_none());
        assert_eq!(
            rig.renderer.contour_statistics(),
            crate::ContourStatistics::default()
        );
        let handle = rig.gpu.instance_handles().next().unwrap();
        let object = rig.gpu.instance(handle).unwrap().0;
        rig.gpu
            .set_instance_object(handle, object.with_contour_id(255))
            .unwrap();
        let active = rig.draw(&context);
        assert_eq!(baseline.color, active.color);
        assert!(active.mask.unwrap().contains(&255));
        rig.gpu.set_instance_object(handle, object).unwrap();
        let empty = rig.draw(&context);
        assert_eq!(baseline.color, empty.color);
        assert!(empty.mask.is_none());
        assert!(rig.renderer.contour_target().is_none());
        assert_eq!(
            rig.renderer.contour_statistics(),
            crate::ContourStatistics::default()
        );
    }
}

#[test]
fn gpu_contour_msaa_half_edge_retains_exact_per_sample_ids() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = source(255, Shape::Quad, false);
    let mut rig = Rig::new(&context, &source, 4);
    // Shift half a pixel so the vertical edge crosses two of the four sample positions.
    rig.frame.view_projection.w_axis.x = 1.0 / f32::from(u16::try_from(SIZE[0]).unwrap());
    let image = rig.draw(&context);
    let target = rig.renderer.contour_target().unwrap();
    assert_eq!(target.sample_count(), 4);
    let mask = image.mask.unwrap();
    assert_eq!(mask.len(), (SIZE[0] * SIZE[1] * 4) as usize);
    let at = |x, y, sample| mask[(y * SIZE[0] * 4 + sample * SIZE[0] + x) as usize];
    let mut edge: Vec<_> = (0..4).map(|sample| at(8, 12, sample)).collect();
    edge.sort_unstable();
    assert_eq!(edge, [0, 0, 255, 255]);
    for sample in 0..4 {
        assert_eq!(at(7, 12, sample), 0);
        assert_eq!(at(9, 12, sample), 255);
    }
}

#[test]
fn gpu_contour_abandonment_and_slots_keep_submitted_targets() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Rig::new(&context, &source(255, Shape::Quad, false), 1);
    rig.draw(&context);
    let submitted = rig.renderer.contour_target().unwrap();
    let handle = rig.gpu.instance_handles().next().unwrap();
    let object = rig.gpu.instance(handle).unwrap().0;
    rig.gpu
        .set_instance_object(handle, object.with_contour_id(0))
        .unwrap();
    let mut abandoned = context.submission().begin(context.device()).unwrap();
    let (mask, _) = rig.encode(&context, &mut abandoned);
    assert!(mask.is_none());
    drop(abandoned);
    assert_eq!(rig.renderer.contour_target().unwrap(), submitted);
    rig.renderer.select_frame_slot(1);
    rig.gpu.select_frame_slot(1);
    assert!(rig.draw(&context).mask.is_none());
    rig.renderer.select_frame_slot(0);
    assert_eq!(rig.renderer.contour_target().unwrap(), submitted);
    rig.gpu.select_frame_slot(0);
    assert!(rig.draw(&context).mask.is_none());
    assert!(rig.renderer.contour_target().is_none());
    rig.renderer.resize(context.device(), [48, 36]).unwrap();
    rig.gpu.set_instance_object(handle, object).unwrap();
    let resized = rig.draw(&context);
    assert_eq!(resized.mask.unwrap().len(), 48 * 36);
    let target = rig.renderer.contour_target().unwrap();
    assert_eq!([target.width(), target.height()], [48, 36]);
}

#[test]
fn gpu_contour_skin_and_wind_match_visible_color_coverage() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut skin_masks = Vec::new();
    for fixture in [skinning::posed(0), skinning::posed(400), velocity::windy()] {
        for prepass in [false, true] {
            let effects = source(0, Shape::Quad, prepass).effects().cloned();
            let scene = fixture
                .with_objects_and_effects(fixture.geometry().objects().to_vec(), effects)
                .unwrap();
            let mut rig = Rig::new(&context, &scene, 1);
            rig.renderer
                .set_frame_time(std::time::Duration::from_secs_f32(0.25));
            let baseline = rig.draw(&context);
            let handles: Vec<_> = rig.gpu.instance_handles().collect();
            for handle in handles {
                let object = rig.gpu.instance(handle).unwrap().0;
                rig.gpu
                    .set_instance_object(handle, object.with_contour_id(255))
                    .unwrap();
            }
            let selected = rig.draw(&context);
            assert_eq!(baseline.color, selected.color);
            let mask = selected.mask.unwrap();
            assert!(mask.contains(&255));
            for (pixel, id) in selected.color.chunks_exact(4).zip(&mask) {
                assert_eq!(*id == 255, pixel[..3].iter().any(|&channel| channel != 0));
            }
            if scene.geometry().objects()[0].skin_pose().is_some() {
                skin_masks.push(mask);
            }
        }
    }
    assert_eq!(skin_masks[0], skin_masks[1]);
    assert_eq!(skin_masks[2], skin_masks[3]);
    assert_ne!(
        skin_masks[0], skin_masks[2],
        "the shifted joint must change coverage"
    );
}
