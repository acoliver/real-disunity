//! A game's passes read the resident material maps of the scene terrain's surfaces, gathered into
//! arrays by layer. The game creates no GPU resource of its own, and the copies run in the
//! frame's own encoder.
use super::fixture::{quad_scene, terrain_scene};
use super::lent::semantic;
use super::lent_terrain_game::{
    Ask, CHANNELS, Clear, KEPT, Kept, assert_bands, last_arrays, layered_scene, run, shape,
};
use worldrunner_render_post::{FrameError, PrepareContext};
use worldrunner_render_scene::{
    compositor::{FrameInputError, TerrainMaterialArrayError, TerrainMaterialChannel},
    wgpu,
};
// ---------------------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------------------

#[test]
fn a_game_pass_reads_each_layer_and_mip_of_the_material_arrays_and_a_reordered_list_reorders_them()
{
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run(&layered_scene(0), Ask::Arrays);
    run.script.lock().unwrap().layers = vec![0, 1, 2, 3];
    run.rig.render().unwrap();
    assert_eq!(run.seen.lock().unwrap().arrays, []);
    assert_bands(&run.output(), &[0, 1, 2, 3], 0);

    let [albedo, normal, height] = last_arrays();
    for (array, format) in [
        (&albedo, wgpu::TextureFormat::Rgba8Unorm),
        (&normal, wgpu::TextureFormat::Rgba8Unorm),
        (&height, wgpu::TextureFormat::R8Unorm),
    ] {
        assert_eq!(array.format(), format);
        assert_eq!(array.size().width, 2);
        assert_eq!(array.size().depth_or_array_layers, 4);
        assert_eq!(array.mip_level_count(), 2, "every mip is copied");
        assert!(
            array
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
        );
    }
    let first = run.texture_bytes();

    // The same list in the next frame is the same array, and costs nothing more.
    run.rig.render().unwrap();
    assert_eq!(
        last_arrays(),
        [albedo.clone(), normal.clone(), height.clone()]
    );
    assert_eq!(run.texture_bytes(), first, "the cache is made once");

    run.script.lock().unwrap().layers = vec![3, 1, 0, 2];
    run.rig.render().unwrap();
    assert_bands(&run.output(), &[3, 1, 0, 2], 0);
    assert_ne!(
        last_arrays()[0],
        albedo,
        "a reordered list is another array"
    );
    // Albedo and normal arrays are 2 by 2 texels of four bytes and one of one, four layers
    // each; the height array is the same in one byte.
    let array_bytes = (4 + 1) * 4 * 4 * 2 + (4 + 1) * 4;
    assert!(
        run.texture_bytes() - first >= array_bytes,
        "the arrays are counted in the census: {} then {}",
        first,
        run.texture_bytes()
    );
}

#[test]
fn a_cached_array_is_rebuilt_after_the_scene_textures_or_terrain_change() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run(&layered_scene(0), Ask::Arrays);
    run.script.lock().unwrap().layers = vec![0, 1, 2, 3];
    run.rig.render().unwrap();
    let before = last_arrays();
    let bytes = run.texture_bytes();

    run.append_texture();
    let appended = run.texture_bytes();
    assert!(appended > bytes, "the appended texture is resident");
    run.rig.render().unwrap();
    let after_append = last_arrays();
    assert_ne!(
        after_append[0], before[0],
        "an appended texture makes a new snapshot"
    );
    assert_bands(&run.output(), &[0, 1, 2, 3], 0);
    assert_eq!(
        run.texture_bytes(),
        appended,
        "the arrays of the old snapshot left the census with it"
    );

    run.replace_scene(&layered_scene(2));
    run.rig.render().unwrap();
    assert_ne!(last_arrays()[0], after_append[0]);
    assert_bands(&run.output(), &[0, 1, 2, 3], 2);
}

#[test]
fn every_refusal_of_a_material_array_is_typed() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run(&layered_scene(0), Ask::Refusals);
    let limit = run.rig.context.device().limits().max_texture_array_layers;
    let albedo = TerrainMaterialChannel::AlbedoMetallic;
    let normal = TerrainMaterialChannel::NormalRoughness;
    run.script.lock().unwrap().requests = vec![
        (albedo, vec![]),
        (albedo, vec![9]),
        (albedo, vec![0; limit as usize + 1]),
        (albedo, vec![0, 4]),
        (normal, vec![0, 5]),
        (albedo, vec![0, 6]),
        (TerrainMaterialChannel::Height, vec![5, 7]),
        (albedo, vec![0, 1]),
    ];
    run.rig.render().unwrap();
    let rgba = wgpu::TextureFormat::Rgba8Unorm;
    let disagree = |layer, expected, found| {
        Err(TerrainMaterialArrayError::Disagree {
            layer,
            expected,
            found,
        })
    };
    assert_eq!(
        run.seen.lock().unwrap().refusals,
        [
            Err(TerrainMaterialArrayError::NoLayers),
            Err(TerrainMaterialArrayError::UnknownType {
                raw_type: 9,
                surfaces: 8
            }),
            Err(TerrainMaterialArrayError::TooManyLayers {
                requested: limit as usize + 1,
                limit
            }),
            disagree(1, shape(rgba, 2, 2), shape(rgba, 4, 3)),
            disagree(1, shape(rgba, 2, 2), shape(rgba, 2, 1)),
            disagree(
                1,
                shape(rgba, 2, 2),
                shape(wgpu::TextureFormat::Rgba8UnormSrgb, 2, 2)
            ),
            disagree(
                1,
                shape(wgpu::TextureFormat::R8Unorm, 2, 1),
                shape(wgpu::TextureFormat::R8Unorm, 4, 1)
            ),
            Ok(()),
        ]
    );

    let device = run.rig.context.device();
    let mut encoder = run.rig.context.submission().begin(device).unwrap();
    let mut context = PrepareContext::new(device, &mut encoder).unwrap();
    KEPT.with_borrow(|kept| {
        let engine = kept.engine.as_ref().unwrap();
        assert_eq!(
            engine
                .material_array(&mut context, semantic("lent.outside"), albedo, &[0])
                .unwrap_err(),
            TerrainMaterialArrayError::Input(FrameInputError::NotPrepared),
            "a context opened by hand belongs to no frame that published the scene"
        );
    });
    context.finish();
    encoder.finish().cancel();
}

#[test]
fn a_terrain_without_layered_maps_and_a_scene_without_terrain_are_refused_by_type() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let channels = CHANNELS.map(|channel| (channel, vec![0]));
    let mut plain = run(&terrain_scene(), Ask::Refusals);
    plain.script.lock().unwrap().requests = channels.to_vec();
    plain.rig.render().unwrap();
    let expected: Vec<_> = CHANNELS
        .into_iter()
        .map(|channel| {
            Err(TerrainMaterialArrayError::NoChannel {
                raw_type: 0,
                channel,
            })
        })
        .collect();
    assert_eq!(plain.seen.lock().unwrap().refusals, expected);
    drop(plain);
    KEPT.with_borrow_mut(|kept| *kept = Kept::default());

    let mut bare = run(&quad_scene(), Ask::Refusals);
    bare.script.lock().unwrap().requests = channels.to_vec();
    bare.rig.render().unwrap();
    assert_eq!(
        bare.seen.lock().unwrap().refusals,
        vec![Err(TerrainMaterialArrayError::NoTerrain); 3]
    );
}

#[test]
fn a_cancelled_frame_leaves_no_half_built_array_and_the_next_frame_copies_the_layers() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run(&layered_scene(0), Ask::Arrays);
    run.script.lock().unwrap().layers = vec![0, 1, 2, 3];
    run.rig.render_cancelled().unwrap();
    let cancelled = last_arrays();
    let bytes = run.texture_bytes();

    // The cancelled frame submitted nothing, so the array still has to be filled: the next
    // frame that asks stages the copies again into the array the cache kept.
    run.rig.render().unwrap();
    assert_bands(&run.output(), &[0, 1, 2, 3], 0);
    assert_eq!(last_arrays(), cancelled, "the cache keeps one array");
    assert_eq!(run.texture_bytes(), bytes, "no second array was made");

    // A cancelled frame after the copies were submitted leaves the filled array as it was.
    run.rig.render_cancelled().unwrap();
    run.rig.render().unwrap();
    assert_bands(&run.output(), &[0, 1, 2, 3], 0);
    assert_eq!(run.texture_bytes(), bytes);
}

#[test]
fn a_frame_that_fails_after_asking_for_an_array_copies_nothing_and_the_next_frame_does() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run(&layered_scene(0), Ask::Arrays);
    {
        let mut script = run.script.lock().unwrap();
        script.layers = vec![3, 2, 1, 0];
        script.fail = true;
    }
    let device = run.rig.context.device();
    let mut encoder = run.rig.context.submission().begin(device).unwrap();
    assert!(matches!(
        run.rig.encode(&mut encoder),
        Err(FrameError::MissingPass(_))
    ));
    encoder.finish().cancel();

    run.script.lock().unwrap().fail = false;
    run.rig.render().unwrap();
    assert_bands(&run.output(), &[3, 2, 1, 0], 0);
}

#[test]
fn two_features_asking_in_one_frame_share_one_array_and_one_set_of_copies() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run(&layered_scene(0), Ask::Arrays);
    run.script.lock().unwrap().layers = vec![1, 0, 3, 2];
    run.script.lock().unwrap().repeat = true;
    run.rig.render().unwrap();
    assert_bands(&run.output(), &[1, 0, 3, 2], 0);
    let [first, again] = KEPT.with_borrow(|kept| kept.twice.last().unwrap().clone());
    assert_eq!(
        first, again,
        "a second request in the frame is the same array"
    );
}
