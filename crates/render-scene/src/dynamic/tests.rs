use super::DynamicQuad;
use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::frame::FrameUniform;
use crate::offscreen::{OffscreenTarget, RgbaImage};
use crate::renderer::ScenePipelines;
use crate::{CameraPose, OrthographicLens, SceneView, Viewport};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};

struct Fixture {
    context: GpuContext,
    pipelines: ScenePipelines,
    gpu: SceneGpu,
    target: OffscreenTarget,
}

impl Fixture {
    fn new() -> Self {
        let context =
            pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
                .expect("actual GPU required");
        let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
        let gpu = SceneGpu::empty(context.device(), context.queue(), &pipelines);
        let viewport = Viewport {
            width: 128,
            height: 64,
        };
        let target = OffscreenTarget::new(context.device(), viewport).unwrap();
        let view = SceneView::orthographic(
            CameraPose {
                eye_millimeters: [0, -10_000, 0],
                target_millimeters: [0, 0, 0],
            },
            OrthographicLens {
                vertical_half_extent_meters: 2.0,
                near_meters: 0.1,
                far_meters: 100.0,
            },
            viewport,
        )
        .unwrap();
        gpu.set_frame(context.queue(), FrameUniform::unlit(&view));
        Self {
            context,
            pipelines,
            gpu,
            target,
        }
    }

    fn set(&mut self, quads: &[DynamicQuad]) {
        self.gpu
            .set_dynamic_quads(
                self.context.device(),
                self.context.queue(),
                &self.pipelines,
                quads,
            )
            .unwrap();
    }

    fn image(&mut self) -> RgbaImage {
        self.target
            .render(&self.context, &self.gpu, &self.pipelines)
            .unwrap()
    }
}

fn pixel(image: &RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * image.viewport.width as usize + x) * 4;
    image.pixels[offset..offset + 4].try_into().unwrap()
}

fn close(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 2),
        "actual {actual:?}, expected {expected:?}"
    );
}

#[test]
fn gpu_orthographic_unlit_quads_have_depth_independent_pixel_landmarks() {
    let mut f = Fixture::new();
    let left = DynamicQuad {
        position: [-2.0, 0.0, 0.5],
        tint: [1.0, 0.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    let right = DynamicQuad {
        position: [2.0, 20.0, 0.5],
        tint: [0.0, 1.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    f.set(&[left, right]);
    let image = f.image();
    for x in [25, 32, 39] {
        close(pixel(&image, x, 24), [255, 0, 0, 255]);
    }
    for x in [89, 96, 103] {
        close(pixel(&image, x, 24), [0, 255, 0, 255]);
    }
    for (x, y) in [(23, 24), (41, 24), (87, 24), (105, 24), (32, 15), (32, 33)] {
        close(pixel(&image, x, y), [0, 0, 0, 255]);
    }
}

#[test]
fn gpu_sorted_alpha_uses_layer_then_order_and_stable_ties_without_depth_writes() {
    let mut f = Fixture::new();
    let background = DynamicQuad {
        tint: [1.0, 0.0, 0.0, 1.0],
        sorting_layer: -1,
        ..DynamicQuad::default()
    };
    let blue = DynamicQuad {
        tint: [0.0, 0.0, 1.0, 0.5],
        sorting_order: 10,
        ..DynamicQuad::default()
    };
    let green = DynamicQuad {
        tint: [0.0, 1.0, 0.0, 0.5],
        sorting_order: 0,
        ..DynamicQuad::default()
    };
    f.set(&[blue, background, green]);
    close(pixel(&f.image(), 64, 32), [137, 137, 188, 255]);
    f.set(&[
        background,
        blue,
        DynamicQuad {
            sorting_order: 10,
            ..green
        },
    ]);
    close(pixel(&f.image(), 64, 32), [137, 188, 137, 255]);
}

#[test]
fn gpu_runtime_quad_rotation_scale_and_rejected_update_preserve_pixels() {
    let mut f = Fixture::new();
    let quad = DynamicQuad {
        scale: [3.0, 1.0, 0.5],
        rotation: glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2).to_array(),
        tint: [1.0, 1.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    f.set(&[quad]);
    let before = f.image();
    close(pixel(&before, 64, 12), [255, 255, 0, 255]);
    close(pixel(&before, 64, 52), [255, 255, 0, 255]);
    close(pixel(&before, 70, 32), [0, 0, 0, 255]);
    assert!(
        f.gpu
            .set_dynamic_quads(
                f.context.device(),
                f.context.queue(),
                &f.pipelines,
                &[DynamicQuad {
                    texture: Some(7),
                    ..quad
                }]
            )
            .is_err()
    );
    assert_eq!(f.image(), before);
    f.set(&[]);
    close(pixel(&f.image(), 64, 32), [0, 0, 0, 255]);
}

#[test]
fn gpu_runtime_texture_uvs_tint_and_alpha_reach_existing_readback() {
    let mut f = Fixture::new();
    let texture = quadrant_texture();
    f.gpu
        .set_dynamic_textures(
            f.context.device(),
            f.context.queue(),
            &f.pipelines,
            &[texture],
        )
        .unwrap();
    let quad = DynamicQuad {
        scale: [2.0, 1.0, 2.0],
        texture: Some(0),
        ..DynamicQuad::default()
    };
    f.set(&[quad]);
    let image = f.image();
    close(pixel(&image, 56, 24), [255, 0, 0, 255]);
    close(pixel(&image, 72, 24), [0, 255, 0, 255]);
    close(pixel(&image, 56, 40), [0, 0, 255, 255]);
    close(pixel(&image, 72, 40), [188, 188, 188, 255]);
    f.set(&[DynamicQuad {
        uv_rect: [1.0, 0.0, 0.0, 1.0],
        tint: [0.5, 1.0, 1.0, 1.0],
        ..quad
    }]);
    close(pixel(&f.image(), 72, 24), [188, 0, 0, 255]);
}

#[test]
fn gpu_camera_background_uses_existing_target_clear_color() {
    let mut f = Fixture::new();
    let image = f
        .target
        .render_with_clear_color(
            &f.context,
            &f.gpu,
            &f.pipelines,
            wgpu::Color {
                r: 0.25,
                g: 0.0,
                b: 0.5,
                a: 0.5,
            },
        )
        .unwrap();
    close(pixel(&image, 64, 32), [137, 0, 188, 128]);
}

#[test]
fn gpu_invalid_dynamic_inputs_leave_resident_geometry_and_textures_unchanged() {
    let mut f = Fixture::new();
    let quad = DynamicQuad {
        tint: [1.0, 0.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    f.set(&[quad]);
    let before = f.image();
    for invalid in [
        DynamicQuad {
            position: [f32::NAN, 0.0, 0.0],
            ..quad
        },
        DynamicQuad {
            rotation: [0.0; 4],
            ..quad
        },
        DynamicQuad {
            scale: [f32::INFINITY, 1.0, 1.0],
            ..quad
        },
        DynamicQuad {
            rect: [0.0; 4],
            ..quad
        },
        DynamicQuad {
            uv_rect: [-1.0; 4],
            ..quad
        },
        DynamicQuad {
            tint: [2.0; 4],
            ..quad
        },
    ] {
        assert!(
            f.gpu
                .set_dynamic_quads(
                    f.context.device(),
                    f.context.queue(),
                    &f.pipelines,
                    &[quad, invalid]
                )
                .is_err()
        );
        assert_eq!(f.image(), before);
    }
}

#[test]
fn gpu_invalid_texture_updates_preserve_resident_texture_and_pixels() {
    let mut f = Fixture::new();
    f.gpu
        .set_dynamic_textures(
            f.context.device(),
            f.context.queue(),
            &f.pipelines,
            &[quadrant_texture()],
        )
        .unwrap();
    f.set(&[DynamicQuad {
        texture: Some(0),
        ..DynamicQuad::default()
    }]);
    let before = f.image();
    let malformed = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 0,
        bytes: vec![255; 4].into(),
    };
    assert!(
        f.gpu
            .set_dynamic_textures(
                f.context.device(),
                f.context.queue(),
                &f.pipelines,
                &[malformed]
            )
            .is_err()
    );
    assert_eq!(f.image(), before);
    let width = f.context.device().limits().max_texture_dimension_2d + 1;
    let oversized = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width,
        height: 1,
        mips: 1,
        bytes: vec![255; width as usize * 4].into(),
    };
    assert!(
        f.gpu
            .set_dynamic_textures(
                f.context.device(),
                f.context.queue(),
                &f.pipelines,
                &[oversized]
            )
            .is_err()
    );
    assert_eq!(f.image(), before);
}

fn quadrant_texture() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 4,
        height: 4,
        mips: 1,
        bytes: [
            [
                [255, 0, 0, 255],
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 255, 0, 255],
            ],
            [
                [255, 0, 0, 255],
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 255, 0, 255],
            ],
            [
                [0, 0, 255, 255],
                [0, 0, 255, 255],
                [255, 255, 255, 128],
                [255, 255, 255, 128],
            ],
            [
                [0, 0, 255, 255],
                [0, 0, 255, 255],
                [255, 255, 255, 128],
                [255, 255, 255, 128],
            ],
        ]
        .into_iter()
        .flatten()
        .flatten()
        .collect::<Vec<_>>()
        .into(),
    }
}

#[test]
fn gpu_orthographic_camera_roll_and_clipping_use_the_resolved_view() {
    let mut f = Fixture::new();
    let view = SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 2.0,
            near_meters: 0.1,
            far_meters: 100.0,
        },
        [1.0, 0.0, 0.0],
        f.target.viewport(),
    )
    .unwrap();
    f.gpu
        .set_frame(f.context.queue(), FrameUniform::unlit(&view));
    let red = DynamicQuad {
        position: [1.0, 0.0, 0.0],
        tint: [1.0, 0.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    f.set(&[
        red,
        DynamicQuad {
            position: [-1.0, -20.0, 0.0],
            ..red
        },
        DynamicQuad {
            position: [-1.0, 100.0, 0.0],
            ..red
        },
    ]);
    let image = f.image();
    close(pixel(&image, 64, 16), [255, 0, 0, 255]);
    close(pixel(&image, 80, 32), [0, 0, 0, 255]);
    close(pixel(&image, 64, 48), [0, 0, 0, 255]);
}

#[test]
fn gpu_replacing_runtime_textures_clears_quads_and_uses_new_texels() {
    let mut f = Fixture::new();
    f.set(&[DynamicQuad::default()]);
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0, 0, 255, 255].into(),
    };
    f.gpu
        .set_dynamic_textures(
            f.context.device(),
            f.context.queue(),
            &f.pipelines,
            &[texture],
        )
        .unwrap();
    close(pixel(&f.image(), 64, 32), [0, 0, 0, 255]);
    f.set(&[DynamicQuad {
        texture: Some(0),
        ..DynamicQuad::default()
    }]);
    close(pixel(&f.image(), 64, 32), [0, 0, 255, 255]);
}
