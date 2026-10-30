use super::*;
use worldrunner_render_plan::{TexelFormat, TextureFilter, TextureSampler, TextureWrap};

pub(super) fn plane(textures: Vec<SceneTexture>, material: &SceneMaterial) -> RenderScene {
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-1000, -16000, 0], [-125_000, 0]),
                vertex([1000, -16000, 0], [125_000, 0]),
                vertex([1000, 16000, 0], [125_000, 4_000_000]),
                vertex([-1000, 16000, 0], [-125_000, 4_000_000]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap();
    let scene = build(
        textures[0].clone(),
        &super::material(SceneAlphaMode::Opaque, true),
        mesh,
        1,
    );
    scene
        .with_resources(
            SceneResources::new(textures, vec![*material]).unwrap(),
            scene.geometry().clone(),
            scene.effects().cloned(),
        )
        .unwrap()
}

pub(super) fn grazing_frame() -> FrameUniform {
    let mut frame = frame_from(pose());
    let eye = glam::Vec3::new(0.0, -32.0, 2.0);
    frame.view_projection =
        glam::camera::rh::proj::directx::orthographic(-1.1, 1.1, -1.1, 1.1, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(eye, glam::Vec3::ZERO, glam::Vec3::Z);
    frame.camera_position = eye;
    frame.sun_irradiance = glam::Vec3::ZERO;
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    frame
}

fn colored_mips() -> SceneTexture {
    let mut bytes = Vec::new();
    for mip in 0..9 {
        let size = 256_u32 >> mip;
        let color = if mip % 2 == 0 {
            [255, 0, 64, 255]
        } else {
            [0, 255, 192, 255]
        };
        for y in 0..size {
            for x in 0..size {
                let texel = if (x < size / 2) == (y < size / 2) {
                    color
                } else {
                    [color[1], color[0], color[2], color[3]]
                };
                bytes.extend(texel);
            }
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 256,
        height: 256,
        mips: 9,
        bytes: bytes.into(),
    }
}

pub(super) fn resident_images() -> Vec<Vec<u8>> {
    let context = context();
    let frame = grazing_frame();
    let raw = colored_mips();
    [
        None,
        Some(TextureSampler::default()),
        Some(TextureSampler {
            filter: TextureFilter::Point,
            wrap_u: TextureWrap::Clamp,
            wrap_v: TextureWrap::Clamp,
        }),
    ]
    .into_iter()
    .map(|sampler| {
        let texture = sampler.map_or_else(|| raw.clone(), |s| raw.clone().with_sampler(s));
        render_frame(
            &context,
            &plane(vec![texture], &material(SceneAlphaMode::Opaque, true)),
            &frame,
        )
    })
    .collect()
}

#[test]
fn resident_default_and_point_clamp_keep_trilinear_repeat_output() {
    let images = resident_images();
    assert!(images[0].chunks_exact(4).any(|p| p[0] > 10 && p[1] > 10));
    assert_eq!(images[0], images[1]);
    assert_eq!(images[0], images[2]);
}
