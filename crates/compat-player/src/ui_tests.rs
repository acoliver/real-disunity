use super::*;
use serde_json::json;
use worldrunner::scene::gpu::{CameraPose, OrthographicLens, Viewport};

fn view() -> SceneView {
    SceneView::orthographic(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 5.,
            near_meters: 0.1,
            far_meters: 100.,
        },
        Viewport {
            width: 200,
            height: 100,
        },
    )
    .unwrap()
}
fn snapshot(mode: &str) -> Value {
    json!({"objects":[
        {"handle":1,"transform":{"parent":null},"components":[
            {"kind":"Canvas","fields":{"render_mode":mode}}]},
        {"handle":2,"transform":{"parent":1},"components":[]}
    ]})
}
fn glyph() -> Value {
    json!({"width":2,"height":2,"alpha_base64":STANDARD.encode([0,64,128,255]),
        "world_corners":[[1,2,3],[3,2,3],[3,6,3],[1,6,3]],
        "screen_corners":[[0,0],[20,0],[20,40],[0,40]]})
}
fn layout(glyphs: Vec<Value>) -> Value {
    json!({"viewport":[200,100],"texts":[{"object":2,"color":[0.2,0.4,0.6,0.5],
        "sorting_layer":4,"sorting_order":7,"glyphs":Value::Array(glyphs)}]})
}
fn quad_corners(quad: &DynamicQuad) -> [Vec3; 4] {
    let [l, b, r, t] = quad.rect;
    [[l, 0., b], [r, 0., b], [r, 0., t], [l, 0., t]].map(|p| {
        Vec3::from_array(quad.position)
            + Quat::from_array(quad.rotation) * (Vec3::from_array(quad.scale) * Vec3::from_array(p))
    })
}
fn assert_near(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 0.0001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn native_masks_keep_top_row_uv_tint_and_sprite_texture_offset() {
    let mut cache = UiGlyphs::default();
    let mut textures = vec![SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into_boxed_slice(),
    }];
    let mut quads = vec![DynamicQuad::default()];
    assert!(
        cache
            .append(
                &layout(vec![glyph()]),
                &snapshot("screen_space_camera"),
                &view(),
                &mut textures,
                &mut quads
            )
            .unwrap()
    );
    assert_eq!(quads.len(), 2);
    assert_eq!(quads[1].texture, Some(1));
    assert_eq!(
        quads[1].uv_rect.map(f32::to_bits),
        [0., 0., 1., 1.].map(f32::to_bits)
    );
    assert_eq!(
        quads[1].tint.map(f32::to_bits),
        [0.2, 0.4, 0.6, 0.5].map(f32::to_bits)
    );
    assert_eq!((quads[1].sorting_layer, quads[1].sorting_order), (4, 7));
    let SceneTexture::Texels {
        width,
        height,
        bytes,
        ..
    } = &textures[1]
    else {
        panic!()
    };
    assert_eq!((*width, *height), (2, 2));
    assert_eq!(
        &**bytes,
        &[
            255, 255, 255, 0, 255, 255, 255, 64, 255, 255, 255, 128, 255, 255, 255, 255
        ]
    );
    for (actual, expected) in quad_corners(&quads[1])
        .into_iter()
        .zip([[1., 3., 2.], [3., 3., 2.], [3., 3., 6.], [1., 3., 6.]].map(Vec3::from_array))
    {
        assert_near(actual, expected);
    }
}

#[test]
fn cache_reuses_content_and_changes_when_native_mask_or_dimensions_change() {
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    let native = snapshot("world_space");
    let first = layout(vec![glyph(), glyph()]);
    assert!(
        cache
            .append(&first, &native, &view(), &mut textures, &mut quads)
            .unwrap()
    );
    assert_eq!(textures.len(), 1);
    quads.clear();
    assert!(
        !cache
            .append(&first, &native, &view(), &mut textures, &mut quads)
            .unwrap()
    );
    let mut changed = glyph();
    changed["alpha_base64"] = json!(STANDARD.encode([255, 64, 128, 0]));
    assert!(
        cache
            .append(
                &layout(vec![changed]),
                &native,
                &view(),
                &mut textures,
                &mut quads
            )
            .unwrap()
    );
    let mut resized = glyph();
    resized["width"] = json!(1);
    resized["height"] = json!(4);
    assert!(
        cache
            .append(
                &layout(vec![resized]),
                &native,
                &view(),
                &mut textures,
                &mut quads
            )
            .unwrap()
    );
    assert_eq!(textures.len(), 3);
}

#[test]
fn overlay_native_pixels_project_into_the_supplied_camera_and_clip_range() {
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    let view = view();
    cache
        .append(
            &layout(vec![glyph()]),
            &snapshot("screen_space_overlay"),
            &view,
            &mut textures,
            &mut quads,
        )
        .unwrap();
    let pixels = [[0., 0.], [20., 0.], [20., 40.], [0., 40.]];
    for (point, pixel) in quad_corners(&quads[0]).into_iter().zip(pixels) {
        let clip = view.view_projection_matrix().project_point3(point);
        assert_near(
            Vec3::new((clip.x + 1.) * 100., (clip.y + 1.) * 50., 0.),
            Vec3::new(pixel[0], pixel[1], 0.),
        );
        assert!((0.0..1.0).contains(&clip.z));
        assert!((point.y - (-8.9)).abs() < 0.001);
    }
}

#[test]
fn rotated_and_reflected_native_corners_are_preserved() {
    let mut g = glyph();
    let rotation = Quat::from_rotation_x(0.7) * Quat::from_rotation_z(0.4);
    let points = [[0., 0., 0.], [-2., 0., 0.], [-2., 4., 0.], [0., 4., 0.]]
        .map(|p| rotation * Vec3::from_array(p) + Vec3::new(7., 8., 9.));
    g["world_corners"] = json!(points.map(|point| point.to_array()));
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    cache
        .append(
            &layout(vec![g]),
            &snapshot("world_space"),
            &view(),
            &mut textures,
            &mut quads,
        )
        .unwrap();
    for (actual, source) in quad_corners(&quads[0]).into_iter().zip(points) {
        assert_near(actual, Vec3::new(source.x, source.z, source.y));
    }
}

#[test]
fn malformed_late_glyph_rejects_without_appending_or_poisoning_cache() {
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    let mut bad = glyph();
    bad["alpha_base64"] = json!(STANDARD.encode([1, 2, 3, 4, 5]));
    let error = cache
        .append(
            &layout(vec![glyph(), bad]),
            &snapshot("world_space"),
            &view(),
            &mut textures,
            &mut quads,
        )
        .unwrap_err();
    assert!(error.contains("decoded length"));
    assert!(textures.is_empty() && quads.is_empty());
    assert!(
        cache
            .append(
                &layout(vec![glyph()]),
                &snapshot("world_space"),
                &view(),
                &mut textures,
                &mut quads
            )
            .unwrap()
    );
    assert_eq!(quads[0].texture, Some(0));
}

#[test]
fn native_glyph_count_and_mask_budgets_reject_transactionally() {
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    let too_many = layout(vec![glyph(); 4097]);
    let error = cache
        .append(
            &too_many,
            &snapshot("world_space"),
            &view(),
            &mut textures,
            &mut quads,
        )
        .unwrap_err();
    assert!(error.contains("glyph/quad budget"));
    let mut huge = glyph();
    huge["width"] = json!(4096);
    huge["height"] = json!(1025);
    let error = cache
        .append(
            &layout(vec![huge]),
            &snapshot("world_space"),
            &view(),
            &mut textures,
            &mut quads,
        )
        .unwrap_err();
    assert!(error.contains("alpha mask budget"));
    assert!(textures.is_empty() && quads.is_empty());
}

#[test]
fn nonrepresentable_native_corners_reject_instead_of_changing_layout() {
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    for corners in [
        [[1, 2, 3], [3, 2, 3], [4, 6, 3], [1, 6, 3]],
        [[1, 2, 3], [3, 2, 3], [4, 6, 3], [2, 6, 3]],
        [[1, 2, 3], [1, 2, 3], [1, 6, 3], [1, 6, 3]],
    ] {
        let mut g = glyph();
        g["world_corners"] = json!(corners);
        assert!(
            cache
                .append(
                    &layout(vec![g]),
                    &snapshot("world_space"),
                    &view(),
                    &mut textures,
                    &mut quads
                )
                .is_err()
        );
    }
    assert!(textures.is_empty() && quads.is_empty());
}

#[test]
fn persistent_cache_limit_preserves_existing_assets_and_allows_reuse() {
    let glyphs: Vec<_> = (0_u32..1024)
        .map(|i| {
            let mut g = glyph();
            g["alpha_base64"] = json!(STANDARD.encode(i.to_le_bytes()));
            g
        })
        .collect();
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    let native = snapshot("world_space");
    let retained = layout(glyphs);
    assert!(
        cache
            .append(&retained, &native, &view(), &mut textures, &mut quads)
            .unwrap()
    );
    let before = (textures.clone(), quads.clone());
    let error = cache
        .append(
            &layout(vec![glyph()]),
            &native,
            &view(),
            &mut textures,
            &mut quads,
        )
        .unwrap_err();
    assert!(error.contains("cache/table budget"));
    assert_eq!((&textures, &quads), (&before.0, &before.1));
    quads.clear();
    assert!(
        !cache
            .append(&retained, &native, &view(), &mut textures, &mut quads)
            .unwrap()
    );
    assert_eq!((textures.len(), quads.len()), (1024, 1024));
}

#[test]
fn empty_native_text_adds_nothing_and_resize_requires_matching_layout() {
    let (mut cache, mut textures, mut quads) = (UiGlyphs::default(), vec![], vec![]);
    assert!(
        !cache
            .append(
                &layout(vec![]),
                &Value::Null,
                &view(),
                &mut textures,
                &mut quads
            )
            .unwrap()
    );
    let mut stale = layout(vec![glyph()]);
    stale["viewport"] = json!([400, 200]);
    let error = cache
        .append(
            &stale,
            &snapshot("screen_space_overlay"),
            &view(),
            &mut textures,
            &mut quads,
        )
        .unwrap_err();
    assert!(error.contains("viewport"));
    assert!(textures.is_empty() && quads.is_empty());
}
