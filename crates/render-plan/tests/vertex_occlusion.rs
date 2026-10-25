use worldrunner_render_plan::{SceneMeshLod, SceneVertex};

fn triangle() -> SceneMeshLod {
    SceneMeshLod::new(
        0,
        vec![
            SceneVertex {
                position_millimeters: [0; 3],
                normal_micro: [0, 0, 1_000_000],
                texcoord_micro: [0; 2],
            };
            3
        ],
        vec![0, 1, 2],
        None,
    )
    .unwrap()
}

#[test]
fn vertex_occlusion_validates_cardinality_and_endpoints() {
    assert!(triangle().vertex_occlusion_micro().is_none());
    assert!(
        triangle()
            .with_vertex_occlusion(vec![0, 1_000_000])
            .is_err()
    );
    assert!(
        triangle()
            .with_vertex_occlusion(vec![0, 500_000, 1_000_001])
            .is_err()
    );
    let lod = triangle()
        .with_vertex_occlusion(vec![0, 500_000, 1_000_000])
        .unwrap();
    assert_eq!(
        lod.vertex_occlusion_micro(),
        Some([0, 500_000, 1_000_000].as_slice())
    );
    lod.validate().unwrap();
}
