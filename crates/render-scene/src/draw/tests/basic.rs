use super::*;

#[test]
fn an_uploaded_scene_groups_its_objects_into_one_batch_per_mesh_and_material() {
    let context = context();
    let scene = scene(SceneAlphaMode::Opaque, false, 4);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines)
        .expect("the fixture scene uploads");
    assert_eq!(gpu.batch_counts(), [1, 0, 0, 0]);
}

#[test]
fn a_masked_material_lands_in_its_own_pipeline_bucket() {
    // The bucket index decides which pipeline is bound, and therefore whether
    // the alpha-testing fragment entry point runs at all. A misfiled batch
    // would render foliage as opaque rectangles.
    let context = context();
    let scene = scene(
        SceneAlphaMode::Mask {
            cutoff_micro: 501_961,
        },
        true,
        1,
    );
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines)
        .expect("the fixture scene uploads");
    assert_eq!(gpu.batch_counts(), [0, 0, 0, 1]);
}

#[test]
fn terrain_no_longer_enters_the_material_batches_however_many_surfaces_it_names() {
    // Terrain used to be split into one batch per material, which is the fault
    // this port removes: a batch can only name one material, so the boundary
    // between two of them had to fall on a quad edge. It is now one mesh drawn
    // from its own pipeline, and the count that proves it is the batch
    // accounting staying flat as the palette grows. If terrain crept back into
    // the material buckets these totals would climb with the surface count.
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for surfaces in 1..=3 {
        let scene = terrain_scene(surfaces);
        let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines)
            .expect("the fixture scene uploads");
        assert_eq!(
            gpu.batch_counts(),
            [1, 0, 0, 0],
            "{surfaces} terrain surfaces changed the material batching"
        );
    }
}

#[test]
fn terrain_reaches_the_target_rather_than_being_dropped_on_the_way() {
    // The terrain grid is four times the quad's extent, so a scene carrying it
    // must light strictly more pixels than the same scene without. A silently
    // empty vertex buffer, a transposed attribute layout or an index range
    // that never leaves zero would all leave the two counts equal.
    let context = context();
    let bare = lit_pixels(&render(&context, &scene(SceneAlphaMode::Opaque, false, 1)));
    let composed = lit_pixels(&render(&context, &terrain_scene(1)));
    assert!(
        composed > bare,
        "terrain lit {composed} pixels against {bare} without it"
    );
}

#[test]
fn drawing_one_lit_quad_produces_pixels_that_are_not_the_clear_colour() {
    // This is the first moment the whole chain runs: packed vertices feeding
    // declared attributes, an instance buffer feeding the model matrix, a
    // material bind group feeding the sampler, and the frame uniform feeding
    // the projection. Any disagreement among them leaves the target black.
    let context = context();
    let pixels = render(&context, &scene(SceneAlphaMode::Opaque, false, 1));
    let lit = lit_pixels(&pixels);
    assert!(
        lit > 0,
        "the quad rendered nothing; the target is entirely the clear colour"
    );
}

#[test]
fn the_quad_covers_a_substantial_share_of_the_frame_rather_than_a_stray_sliver() {
    // A projection or basis error usually still puts *something* on screen,
    // typically a degenerate edge-on sliver or a single corner. Requiring a
    // large share of the frame is what separates "drew the quad" from "drew a
    // few pixels of something".
    let context = context();
    let pixels = render(&context, &scene(SceneAlphaMode::Opaque, false, 1));
    let lit = lit_pixels(&pixels);
    let total = (TARGET * TARGET) as usize;
    assert!(
        lit * 10 > total,
        "only {lit} of {total} pixels were covered, which is too few for a two-metre quad at two metres"
    );
}

#[test]
fn a_single_sided_quad_wound_the_other_way_is_culled_entirely() {
    // Proves the front-face convention actually reaches the driver. If winding
    // were ignored, this scene would look identical to the one above and the
    // culling test would be vacuous.
    let context = context();
    let scene = build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, false),
        reversed_quad(),
        1,
    );
    let pixels = render(&context, &scene);
    assert_eq!(
        lit_pixels(&pixels),
        0,
        "a back-facing single-sided quad must be culled"
    );
}

#[test]
fn a_double_sided_quad_wound_the_other_way_still_renders() {
    let context = context();
    let scene = build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, true),
        reversed_quad(),
        1,
    );
    assert!(lit_pixels(&render(&context, &scene)) > 0);
}

#[test]
fn the_materials_own_albedo_reaches_the_shader_rather_than_the_placeholder() {
    // Four of the five material texture slots fall back to a neutral 1x1
    // placeholder, so a bind group that wired albedo to the placeholder too
    // would still render a plausible lit surface. Giving the albedo a colour
    // no placeholder carries is what makes that mistake visible: a red quad
    // proves the sampled texels came from the scene's own table.
    let context = context();
    let scene = build(
        texture(RED_565),
        &material(SceneAlphaMode::Opaque, false),
        quad(),
        1,
    );
    let pixels = render(&context, &scene);
    // Widened before comparing: a saturated channel plus the margin overflows
    // u8 and would panic in debug rather than report a colour.
    let colored = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|texel| {
            let (red, green, blue) = (
                u32::from(texel[0]),
                u32::from(texel[1]),
                u32::from(texel[2]),
            );
            red > green + 32 && red > blue + 32
        })
        .count();
    let frame = usize::try_from(TARGET * TARGET).expect("the fixture frame fits");
    assert!(
        colored * 10 > frame,
        "only {colored} pixels were red-dominant; the albedo did not reach the shader"
    );
}

#[test]
fn an_unreadable_texture_names_the_offending_table_entry() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let broken = build(
        SceneTexture::Encoded {
            encoding: TextureEncoding::DdsContainer,
            color_space: TextureColorSpace::Srgb,
            bytes: vec![0u8; 8].into(),
        },
        &material(SceneAlphaMode::Opaque, false),
        quad(),
        1,
    );
    let error = SceneGpu::upload(context.device(), context.submission(), &broken, &pipelines)
        .expect_err("a truncated container cannot be prepared");
    assert!(matches!(error, DrawError::Texture { index: 0, .. }));
}

#[test]
fn a_yaw_locked_card_turns_to_face_a_camera_its_placement_faced_away_from() {
    let context = context();
    // The placement is a half turn, so the authored card faces due north while
    // the camera stands to the south. Single-sided, that is not a dim card but
    // no card at all, which is what makes the pair of counts decisive.
    let placed = lit_pixels(&render(&context, &card_scene(None, turned_away())));
    assert_eq!(placed, 0, "the placement points the card away from the eye");

    let billboarded = lit_pixels(&render(
        &context,
        &card_scene(Some(BillboardPolicy::YawLocked), turned_away()),
    ));
    assert!(
        billboarded > 0,
        "a yaw-locked card must replace its authored rotation, not compose with it"
    );
}

#[test]
fn crossed_quads_are_drawn_as_placed_rather_than_turned_toward_the_camera() {
    let context = context();
    // Crossed quads earn their second quad precisely by not rotating: the
    // policy is a description of the geometry, not a request to face the eye.
    // Were it treated as a camera-facing mode, this count would rise above
    // zero and the two quads would swing together into one doubled surface.
    let drawn = lit_pixels(&render(
        &context,
        &card_scene(Some(BillboardPolicy::CrossQuads), turned_away()),
    ));
    assert_eq!(drawn, 0, "crossed quads keep the rotation they were given");
}

#[test]
fn a_spherical_card_tilts_toward_an_elevated_camera_while_a_yaw_locked_one_stays_upright() {
    let context = context();
    // Seen from well above, a card pinned upright is foreshortened to roughly
    // the cosine of the elevation, while one free to pitch presents its whole
    // face. Counting covered pixels separates the two axes of freedom without
    // asserting an exact projection.
    let yaw_locked = lit_pixels(&render_from(
        &context,
        &card_scene(Some(BillboardPolicy::YawLocked), turned_away()),
        overhead_pose(),
    ));
    let spherical = lit_pixels(&render_from(
        &context,
        &card_scene(Some(BillboardPolicy::Spherical), turned_away()),
        overhead_pose(),
    ));

    assert!(
        yaw_locked > 0,
        "the upright card still faces the eye in yaw"
    );
    assert!(
        spherical > yaw_locked * 2,
        "a spherical card should present far more of itself than an upright one: \
         spherical {spherical}, yaw-locked {yaw_locked}"
    );
}
