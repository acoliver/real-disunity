use super::foliage::{geometry_sources, pass_color};
use super::*;
use worldrunner_render_plan::{SceneFoldSampling, SceneTerrainContact, SceneTint};

#[test]
fn fold_only_preserves_surface_channels_at_local_heights_and_raised_placements() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for height in [-500, 0, 500, 2000] {
        let base = super::fold_translucency::folded_scene(height);
        let mut materials = base.resources().materials().to_vec();
        materials[0] = materials[0].with_terrain_contact(
            SceneTerrainContact::new(SceneTint::new([0, 500_000, 0]).unwrap(), 0, 1_000_000, 0)
                .unwrap()
                .with_normal_fold(2000, 0)
                .unwrap()
                .with_fold_sampling(SceneFoldSampling::Vertex),
        );
        let scene = RenderScene::new(SceneParts {
            resources: SceneResources::new(base.resources().textures().to_vec(), materials)
                .unwrap(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: base.effects().cloned(),
        })
        .unwrap();
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        let mut camera = pose();
        camera.eye_millimeters[2] += height - 500;
        camera.target_millimeters[2] += height - 500;
        gpu.set_frame(context.queue(), frame_from(camera));
        let [forward, _, metadata, _] = geometry_sources();
        let output = "vec4<f32>(albedo_sample.r * 0.2 + 0.202, roughness * 0.2 + 0.202, metalness * 0.2 + 0.202, 1.0)";
        let forward = forward.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);", &format!("return {output};"));
        let metadata = metadata.replace(
            "return vec4<f32>(normal, roughness);",
            &format!("return {};", output.replace("albedo_sample", "albedo")),
        );
        let first = pass_color(&context, &gpu, &layouts, &forward, "fs_opaque");
        let second = pass_color(&context, &gpu, &layouts, &metadata, "fs_opaque");
        let center = (32 * TARGET as usize + 32) * 4;
        // Forward and metadata shaders are separate compiled code for the same ideal value.
        worldrunner_render_gpu::tolerance_test_support::assert_paths_agree(
            &first[center..center + 4],
            &second[center..center + 4],
            &format!("height={height} forward versus metadata"),
        );
        // White albedo and roughness 1 give 0.2 + 0.202, metalness 0 gives 0.202.
        for (channel, linear) in [0.402, 0.402, 0.202].into_iter().enumerate() {
            worldrunner_render_gpu::tolerance_test_support::assert_code_within(
                first[center + channel],
                worldrunner_render_gpu::tolerance_test_support::encoded_steps(linear, true),
                0.0,
                &format!("height={height}, channel {channel}"),
            );
        }
        assert_eq!(first[center + 3], 255, "height={height}, alpha");
    }
}
