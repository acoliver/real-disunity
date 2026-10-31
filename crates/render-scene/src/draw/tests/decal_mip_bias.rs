//! The material mip bias in the frame block moves the decal albedo to a finer mip level. The
//! rig in `mip_rig` supplies a plain renderer (bias +0.0) and a temporal half-scale renderer
//! (bias exactly -2). The decal albedo has a constant colour per mip level and the 1 m decal on a
//! vertical card spans `64 / 2^j` of the 32 pixels, so the implicit LOD of the 64 texel texture is
//! exactly the integer `j`.
use super::decals::decal;
use super::mip_bias::mipped_texture;
use super::mip_rig::{RENDER, assert_level, center, plain, temporal};
use super::*;
use worldrunner_render_plan::{SceneDecalAxis, SceneDecalMapping};

/// Camera distance in millimeters at which a 1 m decal spans `64 / 2^level` of 32 pixels under
/// the 60 degree vertical field of view: `2^level / (4 tan 30 degrees)` meters, 1732.05 and
/// 3464.10, rounded to a millimeter. The LOD moves by under 5e-4.
fn distance_for(level: u32) -> i64 {
    match level {
        2 => 1732,
        3 => 3464,
        _ => unreachable!("only levels 2 and 3 have a fixture distance"),
    }
}

fn scene() -> RenderScene {
    let projection = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let mut projected = decal();
    projected.axis = SceneDecalAxis::Y;
    projected.dimensions_millimeters = [1000; 3];
    projected.transform.position_millimeters = [0, 0, 1000];
    projected.edge_fade_micro = 0;
    projected.angle_cutoff_micro = 0;
    projected.angle_fade_micro = 0;
    projected.mapping = SceneDecalMapping {
        double_sided: true,
        ..SceneDecalMapping::default()
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture(), mipped_texture()],
            vec![material(SceneAlphaMode::Opaque, false), projection],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![card(None)],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
    .with_decals(vec![projected])
    .unwrap()
}

fn frame_at(distance: i64) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -distance, 1000],
            target_millimeters: [0, 0, 1000],
        },
        camera(),
        Viewport {
            width: RENDER,
            height: RENDER,
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

#[test]
fn a_temporal_half_scale_renderer_selects_decal_mips_two_levels_finer() {
    let context = context();
    let scene = scene();
    for level in [2_u32, 3] {
        let distance = distance_for(level);
        let reference = center(&context, &mut plain(&context), &scene, &frame_at(distance));
        assert_level(reference, level as usize, "bias 0");
        let mut configured = temporal(&context);
        let biased = center(&context, &mut configured, &scene, &frame_at(distance));
        assert_level(biased, level as usize - 2, "bias -2");
    }
}
