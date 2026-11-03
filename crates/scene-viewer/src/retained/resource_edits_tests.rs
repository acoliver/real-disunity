use super::*;
use crate::{ObjectId, SceneObjectEdit};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneAlphaMode, SceneAppend, SceneGeometry,
    SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject, SceneParts,
    SceneResources, SceneRotation, SceneSpecularCube, SceneTexture, SceneTransform, SceneVertex,
    TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_scene::draw::DrawError;

fn host(limit: usize) -> RetainedHost {
    RetainedHost::with_preparation_limit(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
        limit,
    )
    .unwrap()
}

pub(crate) fn texture(value: u8) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![value, 0, 0, 255].into(),
    }
}

pub(crate) fn material_on(texture: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

/// A square of the given half extent in front of the resident quad.
pub(crate) fn mesh(half: i64) -> SceneMesh {
    let vertices = [
        [-half, -half, 50],
        [half, -half, 50],
        [half, half, 50],
        [-half, half, 50],
    ]
    .into_iter()
    .map(|position| SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: [0, 0, 1_000_000],
        texcoord_micro: [0, 0],
    })
    .collect();
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}

pub(crate) fn object(mesh: u32, material: u32) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(material),
    )
}

pub(crate) fn append() -> SceneAppend {
    SceneAppend {
        textures: vec![texture(200)],
        materials: vec![material_on(1)],
        meshes: vec![mesh(500), mesh(250)],
    }
}

/// The scene a full replacement with the combined tables builds.
pub(crate) fn combined(base: &RenderScene, append: &SceneAppend) -> RenderScene {
    let mut textures = base.resources().textures().to_vec();
    textures.extend(append.textures.iter().cloned());
    let mut materials = base.resources().materials().to_vec();
    materials.extend(append.materials.iter().copied());
    let mut meshes = base.geometry().meshes().to_vec();
    meshes.extend(append.meshes.iter().cloned());
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(meshes, base.geometry().objects().to_vec()).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn snapshot(host: &RetainedHost) -> (RenderScene, Vec<(ObjectId, SceneObject)>, u64, usize) {
    (
        host.scene().clone(),
        host.objects().collect(),
        host.accepted_generation(),
        host.preparations_in_flight(),
    )
}

#[test]
fn an_append_adds_meshes_and_a_material_that_a_new_object_uses() {
    let mut host = host(4);
    host.apply(RetainedUpdate::AppendResources(append()))
        .unwrap();
    assert_eq!(host.scene().geometry().meshes().len(), 3);
    assert_eq!(host.scene().resources().materials().len(), 2);
    host.apply(RetainedUpdate::Add {
        id: ObjectId(7),
        object: object(2, 1),
    })
    .unwrap();
    assert_eq!(host.object(ObjectId(7)), Some(object(2, 1)));
}

#[test]
fn the_materialized_scene_equals_a_full_replacement() {
    let base = crate::renderer::retained_tests::source();
    let added = append();
    let mut appended = host(4);
    appended
        .apply(RetainedUpdate::AppendResources(added.clone()))
        .unwrap();
    appended
        .apply(RetainedUpdate::Add {
            id: ObjectId(1),
            object: object(1, 1),
        })
        .unwrap();
    let mut replaced = host(4);
    let full = combined(&base, &added);
    replaced
        .apply(RetainedUpdate::Resources {
            resources: full.resources().clone(),
            meshes: full.geometry().meshes().to_vec(),
        })
        .unwrap();
    replaced
        .apply(RetainedUpdate::Add {
            id: ObjectId(1),
            object: object(1, 1),
        })
        .unwrap();
    appended.materialize_objects().unwrap();
    replaced.materialize_objects().unwrap();
    assert_eq!(appended.scene(), replaced.scene());
}

#[test]
fn a_bad_reference_names_the_item_and_changes_nothing() {
    let mut host = host(4);
    let before = snapshot(&host);
    let error = host
        .apply(RetainedUpdate::AppendResources(SceneAppend {
            textures: vec![texture(1)],
            materials: vec![material_on(1), material_on(9)],
            meshes: vec![mesh(500)],
        }))
        .unwrap_err();
    let text = error.to_string();
    assert!(text.contains("appended material 2"), "{text}");
    assert!(text.contains("texture handle 9"), "{text}");
    assert_eq!(snapshot(&host), before);
}

#[test]
fn a_mesh_slot_naming_a_missing_material_names_the_mesh() {
    let mut host = host(4);
    let before = snapshot(&host);
    let slot =
        worldrunner_render_plan::SceneLodMaterialSlot::new(MaterialHandle::from_raw(5), 0..3)
            .unwrap();
    let vertices = mesh(500).lods()[0].vertices().to_vec();
    let lod = SceneMeshLod::new(0, vertices, vec![0, 1, 2], None)
        .unwrap()
        .with_material_slots(vec![slot])
        .unwrap();
    let error = host
        .apply(RetainedUpdate::AppendResources(SceneAppend {
            meshes: vec![SceneMesh::new(vec![lod]).unwrap()],
            ..SceneAppend::default()
        }))
        .unwrap_err();
    assert!(error.to_string().contains("appended mesh 1"), "{error}");
    assert_eq!(snapshot(&host), before);
}

#[test]
fn a_specular_cube_needs_a_rebuild_and_changes_nothing() {
    let mut host = host(4);
    let before = snapshot(&host);
    let cube = SceneSpecularCube::rgba16_float(4, 1, vec![0; 768].into()).unwrap();
    let error = host
        .apply(RetainedUpdate::AppendResources(SceneAppend {
            textures: vec![SceneTexture::SpecularCube(cube)],
            ..SceneAppend::default()
        }))
        .unwrap_err();
    assert!(
        matches!(
            error,
            ViewerError::Draw(DrawError::NeedsRebuild {
                item: "texture",
                index: 1,
                ..
            })
        ),
        "{error}"
    );
    assert_eq!(snapshot(&host), before);
}

#[test]
fn an_empty_append_takes_no_slot() {
    let mut host = host(1);
    host.apply(RetainedUpdate::AppendResources(SceneAppend::default()))
        .unwrap();
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.preparations_in_flight(), 0);
}

#[test]
fn an_append_takes_one_preparation_slot() {
    let mut host = host(1);
    host.apply(RetainedUpdate::AppendResources(append()))
        .unwrap();
    assert_eq!(host.preparations_in_flight(), 1);
    assert!(matches!(
        host.apply(RetainedUpdate::AppendResources(append())),
        Err(ViewerError::BackPressure { limit: 1 })
    ));
}

#[test]
fn appends_stack_and_later_ones_may_use_earlier_items() {
    let mut host = host(4);
    host.apply(RetainedUpdate::AppendResources(append()))
        .unwrap();
    host.apply(RetainedUpdate::AppendResources(SceneAppend {
        textures: Vec::new(),
        materials: vec![material_on(1)],
        meshes: vec![mesh(100)],
    }))
    .unwrap();
    assert_eq!(host.scene().resources().materials().len(), 3);
    assert_eq!(host.scene().geometry().meshes().len(), 4);
    host.apply(RetainedUpdate::Objects(vec![
        SceneObjectEdit::Add {
            id: ObjectId(5),
            object: object(3, 2),
        },
        SceneObjectEdit::Add {
            id: ObjectId(6),
            object: object(2, 1),
        },
    ]))
    .unwrap();
}

#[test]
fn an_append_after_a_full_replacement_works() {
    let base = crate::renderer::retained_tests::source();
    let mut host = host(4);
    let full = combined(&base, &append());
    host.apply(RetainedUpdate::Resources {
        resources: full.resources().clone(),
        meshes: full.geometry().meshes().to_vec(),
    })
    .unwrap();
    host.apply(RetainedUpdate::AppendResources(SceneAppend {
        textures: vec![texture(9)],
        materials: vec![material_on(2)],
        meshes: Vec::new(),
    }))
    .unwrap();
    assert_eq!(host.scene().resources().textures().len(), 3);
    assert_eq!(host.scene().resources().materials().len(), 3);
    assert_eq!(host.scene().geometry().meshes().len(), 3);
}

#[test]
fn an_object_edit_naming_an_unappended_mesh_is_still_refused() {
    let mut host = host(4);
    assert!(
        host.apply(RetainedUpdate::Add {
            id: ObjectId(7),
            object: object(2, 1),
        })
        .is_err()
    );
}

#[test]
fn replacing_a_mesh_keeps_handles_and_objects() {
    let mut host = host(4);
    let objects: Vec<_> = host.objects().collect();
    host.apply(RetainedUpdate::ReplaceMesh {
        handle: MeshHandle::from_raw(0),
        mesh: mesh(300),
    })
    .unwrap();
    assert_eq!(host.scene().geometry().meshes()[0], mesh(300));
    assert_eq!(host.objects().collect::<Vec<_>>(), objects);
}

#[test]
fn replacing_a_texture_keeps_handles_and_materials() {
    let mut host = host(4);
    let materials = host.scene().resources().materials().to_vec();
    host.apply(RetainedUpdate::ReplaceTexture {
        handle: TextureHandle::from_raw(0),
        texture: texture(77),
    })
    .unwrap();
    assert_eq!(host.scene().resources().textures()[0], texture(77));
    assert_eq!(host.scene().resources().materials(), materials.as_slice());
}

#[test]
fn replacing_a_missing_item_names_the_handle_and_changes_nothing() {
    let mut host = host(4);
    let before = snapshot(&host);
    let error = host
        .apply(RetainedUpdate::ReplaceMesh {
            handle: MeshHandle::from_raw(4),
            mesh: mesh(300),
        })
        .unwrap_err();
    assert!(error.to_string().contains("mesh handle 4"), "{error}");
    let error = host
        .apply(RetainedUpdate::ReplaceTexture {
            handle: TextureHandle::from_raw(3),
            texture: texture(1),
        })
        .unwrap_err();
    assert!(error.to_string().contains("texture handle 3"), "{error}");
    assert_eq!(snapshot(&host), before);
}

#[test]
fn replacing_a_texture_a_terrain_palette_copies_needs_a_rebuild() {
    let mut host = RetainedHost::new(
        crate::renderer::retained_tests::terrain_source(),
        crate::tests::options(),
    )
    .unwrap();
    let before = snapshot(&host);
    let error = host
        .apply(RetainedUpdate::ReplaceTexture {
            handle: TextureHandle::from_raw(0),
            texture: texture(3),
        })
        .unwrap_err();
    assert!(
        matches!(
            error,
            ViewerError::Draw(DrawError::NeedsRebuild {
                item: "material",
                index: 0,
                ..
            })
        ),
        "{error}"
    );
    assert_eq!(snapshot(&host), before);
}

#[test]
fn replacing_a_texture_with_a_cube_needs_a_rebuild() {
    let mut host = host(4);
    let before = snapshot(&host);
    let cube = SceneSpecularCube::rgba16_float(4, 1, vec![0; 768].into()).unwrap();
    let error = host
        .apply(RetainedUpdate::ReplaceTexture {
            handle: TextureHandle::from_raw(0),
            texture: SceneTexture::SpecularCube(cube),
        })
        .unwrap_err();
    assert!(
        matches!(
            error,
            ViewerError::Draw(DrawError::NeedsRebuild {
                item: "texture",
                ..
            })
        ),
        "{error}"
    );
    assert_eq!(snapshot(&host), before);
}
