use super::*;
use crate::scene::{
    MaterialHandle, MeshHandle, SceneAlphaMode, SceneGeometry, SceneLodMaterialSlot, SceneMaterial,
    SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject, SceneParts, SceneResources,
    SceneRotation, SceneSpecularCube, SceneTexture, SceneTransform, SceneVertex, TexelFormat,
    TextureColorSpace, TextureHandle,
};

fn texture(value: u8) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![value, value, value, 255].into(),
    }
}

fn material_on(texture: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

fn lod(slots: Option<Vec<SceneLodMaterialSlot>>) -> SceneMeshLod {
    let vertices = [[0, 0, 0], [1000, 0, 0], [0, 1000, 0]]
        .into_iter()
        .map(|position| SceneVertex {
            tangent_micro: None,
            position_millimeters: position,
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: [0, 0],
        })
        .collect();
    let lod = SceneMeshLod::new(0, vertices, vec![0, 1, 2], None).unwrap();
    match slots {
        Some(slots) => lod.with_material_slots(slots).unwrap(),
        None => lod,
    }
}

fn mesh() -> SceneMesh {
    SceneMesh::new(vec![lod(None)]).unwrap()
}

fn mesh_with_slot(material: u32) -> SceneMesh {
    let slot = SceneLodMaterialSlot::new(MaterialHandle::from_raw(material), 0..3).unwrap();
    SceneMesh::new(vec![lod(Some(vec![slot]))]).unwrap()
}

fn object(mesh: u32, material: u32) -> SceneObject {
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

fn build(
    textures: Vec<SceneTexture>,
    materials: Vec<SceneMaterial>,
    meshes: Vec<SceneMesh>,
) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(meshes, vec![object(0, 0)]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn base() -> RenderScene {
    build(
        vec![texture(10), texture(20)],
        vec![material_on(0)],
        vec![mesh()],
    )
}

fn payload_address(scene: &RenderScene, index: usize) -> *const u8 {
    match &scene.resources().textures()[index] {
        SceneTexture::Texels { bytes, .. } => bytes.as_ptr(),
        _ => panic!("texels"),
    }
}

#[test]
fn appended_scene_equals_the_full_replacement() {
    let mut scene = base();
    scene
        .append_resources(SceneAppend {
            textures: vec![texture(30)],
            materials: vec![material_on(1), material_on(2)],
            meshes: vec![mesh(), mesh_with_slot(2)],
        })
        .unwrap();
    let full = build(
        vec![texture(10), texture(20), texture(30)],
        vec![material_on(0), material_on(1), material_on(2)],
        vec![mesh(), mesh(), mesh_with_slot(2)],
    );
    assert_eq!(scene, full);
}

#[test]
fn append_keeps_existing_payloads_in_place() {
    let mut scene = base();
    let before = [payload_address(&scene, 0), payload_address(&scene, 1)];
    scene
        .append_resources(SceneAppend {
            textures: vec![texture(30)],
            materials: vec![material_on(2)],
            meshes: vec![mesh()],
        })
        .unwrap();
    assert_eq!(
        [payload_address(&scene, 0), payload_address(&scene, 1)],
        before
    );
}

#[test]
fn an_empty_append_changes_nothing() {
    let mut scene = base();
    scene.append_resources(SceneAppend::default()).unwrap();
    assert_eq!(scene, base());
}

#[test]
fn a_material_may_use_old_and_new_textures() {
    let mut scene = base();
    scene
        .append_resources(SceneAppend {
            textures: vec![texture(30)],
            materials: vec![material_on(1), material_on(2)],
            meshes: Vec::new(),
        })
        .unwrap();
    assert_eq!(scene.resources().materials().len(), 3);
    assert_eq!(scene.resources().textures().len(), 3);
}

#[test]
fn a_material_on_a_missing_texture_fails_and_leaves_the_scene_unchanged() {
    let mut scene = base();
    let error = scene
        .append_resources(SceneAppend {
            textures: vec![texture(30)],
            materials: vec![material_on(3)],
            meshes: vec![mesh()],
        })
        .unwrap_err();
    assert!(matches!(
        error,
        SceneValidationError::HandleOutOfRange {
            resource: "texture",
            handle: 3,
            limit: 3,
            ..
        }
    ));
    assert_eq!(scene, base());
}

#[test]
fn an_invalid_new_texture_fails_and_leaves_the_scene_unchanged() {
    let mut scene = base();
    let broken = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 1,
        bytes: vec![0; 3].into(),
    };
    assert!(
        scene
            .append_resources(SceneAppend {
                textures: vec![broken],
                materials: Vec::new(),
                meshes: Vec::new(),
            })
            .is_err()
    );
    assert_eq!(scene, base());
}

#[test]
fn a_new_mesh_slot_may_name_old_or_new_materials_but_not_missing_ones() {
    let mut scene = base();
    scene
        .append_resources(SceneAppend {
            textures: Vec::new(),
            materials: vec![material_on(1)],
            meshes: vec![mesh_with_slot(0), mesh_with_slot(1)],
        })
        .unwrap();
    let error = scene
        .append_resources(SceneAppend {
            textures: Vec::new(),
            materials: Vec::new(),
            meshes: vec![mesh_with_slot(2)],
        })
        .unwrap_err();
    assert!(matches!(
        error,
        SceneValidationError::HandleOutOfRange {
            resource: "material",
            handle: 2,
            ..
        }
    ));
    assert_eq!(scene.geometry().meshes().len(), 3);
}

#[test]
fn replacing_a_texture_keeps_every_handle() {
    let mut scene = base();
    scene
        .replace_texture(TextureHandle::from_raw(1), texture(99))
        .unwrap();
    let expected = build(
        vec![texture(10), texture(99)],
        vec![material_on(0)],
        vec![mesh()],
    );
    assert_eq!(scene, expected);
}

#[test]
fn replacing_a_texture_with_a_cube_that_a_material_samples_fails() {
    let mut scene = base();
    let cube = SceneTexture::SpecularCube(
        SceneSpecularCube::rgba16_float(4, 1, vec![0; 768].into()).unwrap(),
    );
    let error = scene
        .replace_texture(TextureHandle::from_raw(0), cube)
        .unwrap_err();
    assert!(matches!(
        error,
        SceneValidationError::UnsupportedInput { .. }
    ));
    assert_eq!(scene, base());
}

#[test]
fn replacing_a_missing_texture_or_mesh_names_the_table() {
    let mut scene = base();
    assert!(matches!(
        scene
            .replace_texture(TextureHandle::from_raw(2), texture(1))
            .unwrap_err(),
        SceneValidationError::HandleOutOfRange {
            resource: "texture",
            handle: 2,
            limit: 2,
            ..
        }
    ));
    assert!(matches!(
        scene
            .replace_mesh(MeshHandle::from_raw(1), mesh())
            .unwrap_err(),
        SceneValidationError::HandleOutOfRange {
            resource: "mesh",
            handle: 1,
            limit: 1,
            ..
        }
    ));
    assert_eq!(scene, base());
}

#[test]
fn replacing_a_mesh_keeps_handles_and_checks_its_material_slots() {
    let mut scene = base();
    scene
        .replace_mesh(MeshHandle::from_raw(0), mesh_with_slot(0))
        .unwrap();
    assert_eq!(scene.geometry().meshes()[0], mesh_with_slot(0));
    assert_eq!(scene.geometry().objects(), &[object(0, 0)]);
    let error = scene
        .replace_mesh(MeshHandle::from_raw(0), mesh_with_slot(1))
        .unwrap_err();
    assert!(matches!(
        error,
        SceneValidationError::HandleOutOfRange {
            resource: "material",
            handle: 1,
            ..
        }
    ));
    assert_eq!(scene.geometry().meshes()[0], mesh_with_slot(0));
}
