use super::*;
use worldrunner_render_plan::SceneFadeDirection;

pub(super) fn scenes() -> Vec<(&'static str, RenderScene, CameraPose)> {
    let stock = scene(SceneAlphaMode::Opaque, false, 3);
    let scaled = stock
        .with_objects_and_effects(
            vec![SceneObject::new(
                SceneTransform {
                    scale_millionths: 2_000_000,
                    ..identity()
                },
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
            stock.effects().cloned(),
        )
        .unwrap();
    let mut cases = vec![
        ("stock instancing", stock, pose()),
        ("scaled mesh", scaled, pose()),
        ("CPU-deformed mesh", deformed(), pose()),
        ("terrain", terrain_scene(2), pose()),
        ("wind velocity", velocity::windy(), pose()),
    ];
    let cutoff = material(
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        true,
    )
    .with_alpha_factor(500_000)
    .unwrap()
    .with_alpha_to_coverage(true)
    .unwrap();
    cases.push((
        "alpha equals cutoff",
        build(white_texture(), &cutoff, quad(), 1),
        pose(),
    ));
    for distance in [2000, 2001, 3999, 4000, 5999, 6000] {
        cases.push((
            "LOD complementary halves",
            fade_handoff::handoff_scene(
                false,
                false,
                &[SceneFadeDirection::Keep, SceneFadeDirection::Complement],
            ),
            CameraPose {
                eye_millimeters: [0, -distance, 1000],
                target_millimeters: [0, 0, 1000],
            },
        ));
    }
    cases
}

pub(super) fn tie_scene(within: bool) -> RenderScene {
    let mesh = if within {
        let mut vertices = quad().lods()[0].vertices().to_vec();
        let mut other = vertices.clone();
        for v in &mut vertices {
            v.texcoord_micro = [MICRO / 4, 0];
        }
        for v in &mut other {
            v.texcoord_micro = [MICRO * 3 / 4, 0];
        }
        vertices.extend(other);
        SceneMesh::new(vec![
            SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7], None).unwrap(),
        ])
        .unwrap()
    } else {
        quad()
    };
    let mut objects = vec![SceneObject::new(
        identity(),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )];
    if !within {
        objects.push(SceneObject::new(
            identity(),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(1),
        ));
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                SceneTexture::Texels {
                    format: worldrunner_render_plan::TexelFormat::Rgba8Unorm,
                    color_space: TextureColorSpace::Srgb,
                    width: 2,
                    height: 1,
                    mips: 1,
                    bytes: vec![255, 0, 0, 255, 255, 255, 255, 255].into(),
                },
                white_texture(),
            ],
            vec![
                material(SceneAlphaMode::Opaque, true),
                SceneMaterial::new(SceneMaterialParts {
                    albedo: TextureHandle::from_raw(1),
                    normal: None,
                    roughness_metallic: None,
                    ambient_occlusion: None,
                    self_illumination: None,
                    roughness_micro: 200_000,
                    metalness_micro: 800_000,
                    alpha_mode: SceneAlphaMode::Opaque,
                    double_sided: true,
                })
                .unwrap(),
            ],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![mesh], objects).unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

pub(super) fn ties() -> Vec<(&'static str, RenderScene, CameraPose)> {
    vec![
        ("ties within a draw", tie_scene(true), pose()),
        ("ties across draws", tie_scene(false), pose()),
        ("clear depth one", depth_boundary(1000), pose()),
    ]
}

pub(super) fn depth_boundary(z: i64) -> RenderScene {
    let mut vertices = quad().lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[2] = z;
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, true),
        mesh,
        1,
    )
}

fn deformed() -> RenderScene {
    let mut vertices = quad().lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[2] += if vertex.position_millimeters[0] < 0 {
            100
        } else {
            300
        };
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, true),
        mesh,
        1,
    )
}

/// The tie scene reduced to its last surface in API order: the second quad of the
/// shared mesh, or the second object when the quads are separate draws.
pub(super) fn last_winner(source: &RenderScene) -> RenderScene {
    let lod = &source.geometry().meshes()[0].lods()[0];
    let objects = source.geometry().objects();
    let within = objects.len() == 1;
    let quad = if within { 4..8 } else { 0..4 };
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            lod.vertices()[quad].to_vec(),
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap();
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: SceneGeometry::new(vec![mesh], objects[objects.len() - 1..].to_vec()).unwrap(),
        terrain: None,
        effects: source.effects().cloned(),
    })
    .unwrap()
}
