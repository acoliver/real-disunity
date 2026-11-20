//! A game's passes sample the engine's constants and the installed water normal map, which the
//! renderer lends as imports.
use super::fixture::{Rig, SIZE, quad_scene};
use super::lent::{ClearKept, KEPT, Scenario, WATER_NORMAL, inventory, near, output};
use super::lighting::ssao;
use super::shadowed::effects_with;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    RenderScene, SceneAppend, SceneSpecularConvention, SceneTexture, SceneWaterSettings,
    TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_scene::{
    compositor::{BRDF_TABLE_SAMPLES, BRDF_TABLE_SIZE, FrameInputError, FrameWaterNormalError},
    sky::preparation::{EnvironmentPreparation, encode},
    wgpu,
};

/// The half-float texels of the table the stock environment preparation integrates, with the
/// settings the engine's table uses.
fn stock_table(rig: &Rig) -> Vec<[f32; 4]> {
    let device = rig.context.device();
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stock source cube"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let cube = source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut encoder = rig.context.submission().begin(device).unwrap();
    let maps = encode(
        device,
        &mut encoder,
        &cube,
        EnvironmentPreparation {
            convention: SceneSpecularConvention::default(),
            cube_size: 1,
            source_lod_tail: 1,
            mip_count: 1,
            specular_intensity_micro: 1_000_000,
            lut_size: BRDF_TABLE_SIZE,
            samples: BRDF_TABLE_SAMPLES,
        },
        AllocationBudget(1 << 20),
    )
    .unwrap();
    encoder.finish().submit().unwrap();
    let (_, table) = maps.views().unwrap();
    rig.read_half(table.texture())
}

#[test]
fn a_game_pass_samples_the_engine_constants_and_the_triangle_covers_the_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let (game_passes, _) = inventory(Scenario::Constants);
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = game_passes;
    });
    rig.render().unwrap();
    let image = output(&rig);
    assert!(
        image.iter().all(|texel| texel[3] == 255),
        "the full-screen triangle covers every pixel"
    );
    let band = |index: usize| Rig::pixel(&image, index * 16 + 8, 24);
    assert_eq!(band(0), [255, 255, 255, 255], "white");
    assert_eq!(band(1), [0, 0, 0, 255], "black");
    assert!(
        near(band(2), [128, 128, 255, 255], 1),
        "flat normal: {:?}",
        band(2)
    );

    let table = KEPT
        .with_borrow(|kept| kept.brdf.clone())
        .expect("the table was lent");
    let lent = rig.read_half(&table);
    let stock = stock_table(&rig);
    assert_eq!(
        lent, stock,
        "the lent table reads back equal to the stock one"
    );
    let size = BRDF_TABLE_SIZE as usize;
    let expected = stock[20 * size + 33];
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a value in 0..=255"
    )]
    let coded = |value: f32| (value * 255.0).round() as u8;
    let found = band(3);
    assert!(
        near(found, [coded(expected[0]), coded(expected[1]), 0, 255], 1),
        "the pass reads texel (33, 20) of the table: {found:?} against {expected:?}"
    );
    assert!(
        stock.iter().any(|texel| texel[0] > 0.0 && texel[1] > 0.0),
        "the table holds split-sum coefficients"
    );
}

fn water_body() -> worldrunner_render_plan::SceneWaterBody {
    worldrunner_render_plan::SceneWaterBody {
        material: SceneWaterSettings::STOCK,
        surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: 0,
        east_min_millimeters: -10000,
        north_min_millimeters: -10000,
        east_extent_millimeters: 20000,
        north_extent_millimeters: 20000,
        color_micro: [50000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50000],
        roughness_micro: 200_000,
    }
}

/// The quad scene with a flat normal map as texture 1 and, when `bodies` is not empty, water
/// whose first body samples it.
fn water_scene(bodies: &[bool]) -> (RenderScene, Vec<worldrunner_render_plan::SceneWaterBody>) {
    let mut scene = quad_scene();
    scene
        .append_resources(SceneAppend {
            textures: vec![SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![128, 128, 255, 255].into(),
            }],
            ..SceneAppend::default()
        })
        .unwrap();
    let bodies: Vec<_> = bodies
        .iter()
        .map(|mapped| {
            let mut body = water_body();
            body.material.normal = mapped.then(|| TextureHandle::from_raw(WATER_NORMAL));
            body
        })
        .collect();
    let effects = effects_with(Some(ssao()))
        .with_water_bodies(bodies.clone())
        .unwrap();
    let scene = scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    (scene, bodies)
}

fn frame_of_water(bodies: &[bool]) -> Vec<Result<bool, FrameWaterNormalError>> {
    let (scene, installed) = water_scene(bodies);
    let (game_passes, seen) = inventory(Scenario::Water);
    let mut rig = Rig::with_ssao(&scene, SIZE, ssao(), |settings| {
        settings.game_passes = game_passes;
        settings.water = installed;
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    KEPT.with_borrow(|kept| {
        assert_eq!(
            kept.textures.as_ref().unwrap().current().unwrap_err(),
            FrameInputError::OutsideFrame
        );
    });
    std::mem::take(&mut seen.lock().unwrap().water)
}

#[test]
fn the_installed_water_normal_map_is_lent_when_water_is_installed_and_refused_otherwise() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    assert_eq!(
        frame_of_water(&[true, false]),
        [
            Ok(true),
            Err(FrameWaterNormalError::NoNormalMap { body: 1 }),
            Err(FrameWaterNormalError::UnknownBody { body: 2, bodies: 2 }),
        ],
        "the first body's map is the resident texture; the second has none"
    );
    assert_eq!(
        frame_of_water(&[]),
        [Err(FrameWaterNormalError::NoWater); 3],
        "a renderer without water lends no map"
    );
}
