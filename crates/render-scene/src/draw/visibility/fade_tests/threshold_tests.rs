use super::*;

fn terminal_band() -> SceneFadeBand {
    SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade()
}

fn threshold_scene(texels: [u8; 4]) -> RenderScene {
    let base = scene();
    let mut textures = base.resources().textures().to_vec();
    let threshold = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 1,
        bytes: texels.to_vec().into(),
    });
    let material = base.resources().materials()[0]
        .with_fade_direction(SceneFadeDirection::Inclusive)
        .with_fade_quantization(SceneFadeQuantization::Byte)
        .with_fade_coverage(SceneFadeCoverage::Threshold {
            texture: threshold,
            jitter_pixels: [-3, 7],
        });
    let object = SceneObject::new(
        transform([0, 0, 500]),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_fade(terminal_band());
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material]).unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), vec![object]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn select(instances: &Instances, source: &RenderScene, selection: &mut Selection) {
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    // 7 m from the placement, past the terminal band's 6 m end.
    selection.rebuild(instances, &meshes, Mat4::IDENTITY, [0, -7000, 500]);
}

#[test]
fn inclusive_threshold_culling_reads_each_grids_actual_minimum() {
    for (texels, culled) in [
        ([1, 119, 120, 255], 1),
        ([255; 4], 1),
        ([1, 0, 120, 255], 0),
        ([0; 4], 0),
    ] {
        let source = threshold_scene(texels);
        let mut selected = Selection::default();
        select(&Instances::new(&source), &source, &mut selected);
        assert_eq!(selected.stats.considered, 1);
        assert_eq!(selected.stats.culled, culled, "{texels:?}");
        assert_eq!(
            selected.stats.drawn_per_lod.iter().sum::<usize>(),
            1 - culled
        );
        assert_eq!(selected.ranges.len(), 1 - culled);
    }
}

#[test]
#[should_panic(expected = "appended materials have hashed fade coverage")]
fn appending_a_threshold_material_fails_without_its_texture_table() {
    let source = threshold_scene([255; 4]);
    let mut instances = Instances::new(&source);
    instances.extend_resources(source.resources().materials(), 1);
}

#[test]
fn caster_and_slice_selections_cull_fade_empty_placements_like_the_camera() {
    let base = scene();
    let source = fixture(&base.resources().materials()[0], &[Some(terminal_band())]);
    let mut camera = Selection::default();
    select(&Instances::new(&source), &source, &mut camera);
    assert_eq!(camera.stats.culled, 1, "the fixture must be fade-empty");
    let hull = crate::draw::slice_hull::SliceHull::new(
        &[Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 1.0)],
        Mat4::IDENTITY,
        1.0,
    );
    for view in [
        Selection {
            casters_only: true,
            ..Selection::default()
        },
        Selection {
            slice: Some(hull),
            ..Selection::default()
        },
    ] {
        let mut instances = Instances::new(&source);
        let mut culled = view.clone();
        select(&instances, &source, &mut culled);
        instances.fade_cull_disabled = true;
        let mut kept = view;
        select(&instances, &source, &mut kept);
        assert_eq!(kept.ranges.len(), 1, "the view must admit the placement");
        assert_eq!(culled.stats.culled, kept.stats.culled + 1);
        assert!(culled.ranges.is_empty());
    }
}
