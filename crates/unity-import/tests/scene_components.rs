//! Camera, renderer, body and collider views read what the games author and refuse the rest.
#[path = "support/assets.rs"]
mod assets;
#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/objects.rs"]
mod objects;

use fixture::Fixture;
use objects::{game_object, import, transform};
use worldrunner_unity_import::{
    BodyType, Camera, CameraOutput, ClearFlags, Collider, Collider2D, ColliderShape, Interpolation,
    ObjectId, PhysicsMaterial, Reference, RendererMaterial, RendererSettings, Rigidbody2D, Scene,
    SpriteRenderer,
};

const BOUNCY: &str = "c0000000000000000000000000000003";
const SHEET: &str = "c0000000000000000000000000000009";
const SPRITES_DEFAULT: &str = "{fileID: 10754, guid: 0000000000000000f000000000000000, type: 0}";

fn component(class: u32, name: &str, id: u64, fields: &str) -> String {
    format!("--- !u!{class} &{id}\n{name}:\n  m_GameObject: {{fileID: 1}}\n{fields}")
}

const CAMERA: &str = "  m_Enabled: 1\n  m_ClearFlags: 2\n  m_BackGroundColor: {r: 0, g: 0, b: 0, a: 1}\n  m_NormalizedViewPortRect:\n    serializedVersion: 2\n    x: 0\n    y: 0\n    width: 1\n    height: 1\n  near clip plane: 0.3\n  far clip plane: 1000\n  orthographic: 1\n  orthographic size: 8\n  m_Depth: -1\n  m_CullingMask:\n    serializedVersion: 2\n    m_Bits: 4294967295\n  m_TargetTexture: {fileID: 0}\n";
const BODY: &str = "  m_BodyType: 0\n  m_Simulated: 1\n  m_Mass: 0.0001\n  m_LinearDrag: 0\n  m_AngularDrag: 0\n  m_GravityScale: 0\n  m_Interpolate: 1\n  m_SleepingMode: 1\n  m_Constraints: 5\n";
const BOX: &str = "  m_Enabled: 1\n  m_IsTrigger: 1\n  m_UsedByEffector: 0\n  m_UsedByComposite: 0\n  m_Offset: {x: 0.37, y: -0.2}\n  m_Size: {x: 0.32, y: 2.56}\n  m_EdgeRadius: 0\n";

fn sprite_renderer(extra: &str) -> String {
    format!(
        "  m_Enabled: 1\n  m_Materials:\n  - {SPRITES_DEFAULT}\n  m_SortingLayerID: 0\n  m_SortingOrder: -10\n  m_Sprite: {{fileID: 21300000, guid: {SHEET}, type: 3}}\n  m_Color: {{r: 0, g: 1, b: 0.5, a: 1}}\n  m_FlipX: 1\n  m_DrawMode: 0\n{extra}"
    )
}

/// One object holding a component of `class` with `fields` as file ID 12, in a scene whose
/// render settings use `skybox`.
fn scene_with(class: u32, name: &str, fields: &str, skybox: &str) -> Scene {
    let fixture = Fixture::new("2017.1.0f3");
    fixture.asset(
        "Assets/Bouncy.physicsMaterial2D",
        BOUNCY,
        "NativeFormatImporter",
        "--- !u!62 &6200000\nPhysicsMaterial2D:\n  friction: 0.25\n  bounciness: 1\n",
    );
    fixture.asset("Assets/sheet.png", SHEET, "TextureImporter", "png");
    let text = [
        format!("--- !u!104 &3\nRenderSettings:\n  m_SkyboxMaterial: {skybox}\n"),
        game_object(1, "Object", &[2, 12]),
        transform(2, 1, 0.0, 0),
        component(class, name, 12, fields),
    ]
    .concat();
    import(&fixture, "Assets/Main.unity", &text).unwrap()
}

fn target(scene: &Scene) -> ObjectId {
    scene
        .objects()
        .find(|(_, object)| object.source.file_id == 12)
        .unwrap()
        .0
}

#[test]
fn cameras_read_clear_lens_and_depth() {
    let scene = scene_with(20, "Camera", CAMERA, "{fileID: 0}");
    let expected = Camera {
        enabled: true,
        clear: ClearFlags::SolidColor,
        background: [0., 0., 0., 1.],
        orthographic_size: 8.,
        near: 0.3,
        far: 1000.,
        depth: -1.,
        orthographic: true,
        field_of_view: 60.,
        culling_mask: u32::MAX,
        rect: [0., 0., 1., 1.],
        target_display: 0,
        output: CameraOutput {
            allow_hdr: true,
            allow_msaa: true,
            allow_dynamic_resolution: false,
        },
        use_occlusion_culling: true,
    };
    assert_eq!(scene.camera(target(&scene)).unwrap(), expected);
    let skybox = CAMERA.replace("m_ClearFlags: 2", "m_ClearFlags: 1");
    let scene = scene_with(20, "Camera", &skybox, "{fileID: 0}");
    assert_eq!(
        scene.camera(target(&scene)).unwrap().clear,
        ClearFlags::Skybox
    );
}

#[test]
fn unsupported_camera_settings_fail_with_their_object() {
    let error = |fields: &str, skybox: &str| {
        let scene = scene_with(20, "Camera", fields, skybox);
        scene.camera(target(&scene)).unwrap_err().to_string()
    };
    let none = "{fileID: 0}";
    let cases = [
        (CAMERA.replace("orthographic: 1", "orthographic: 0"), none, "perspective"),
        (CAMERA.replace("width: 1", "width: 0.5"), none, "viewport"),
        (CAMERA.replace("4294967295", "1"), none, "culling mask"),
        (CAMERA.replace("m_TargetTexture: {fileID: 0}", "m_TargetTexture: {fileID: 8400000, guid: c0000000000000000000000000000008, type: 2}"), none, "target texture"),
        (CAMERA.replace("m_ClearFlags: 2", "m_ClearFlags: 1"), "{fileID: 2100000, guid: c0000000000000000000000000000007, type: 2}", "skybox material"),
        (CAMERA.replace("m_ClearFlags: 2", "m_ClearFlags: 9"), none, "m_ClearFlags"),
    ];
    for (fields, skybox, expected) in cases {
        let message = error(&fields, skybox);
        assert!(
            message.contains("Camera 12") && message.contains(expected),
            "{message}"
        );
    }
}

#[test]
fn sprite_renderers_read_sprite_tint_flips_and_order() {
    let scene = scene_with(212, "SpriteRenderer", &sprite_renderer(""), "{fileID: 0}");
    let expected = SpriteRenderer {
        renderer: RendererSettings {
            enabled: true,
            sorting_order: -10,
            material: RendererMaterial::SpritesDefault,
        },
        sprite: Reference::External {
            guid: SHEET.parse().unwrap(),
            file_id: 21_300_000,
        },
        color: [0., 1., 0.5, 1.],
        flip_x: true,
        flip_y: false,
    };
    assert_eq!(scene.sprite_renderer(target(&scene)).unwrap(), expected);
    for (fields, expected) in [
        (
            sprite_renderer("").replace("m_DrawMode: 0", "m_DrawMode: 1"),
            "Simple draw mode",
        ),
        (
            sprite_renderer("").replace("m_SortingLayerID: 0", "m_SortingLayerID: 7"),
            "Default sorting layer",
        ),
        (
            sprite_renderer("  m_MaskInteraction: 1\n"),
            "mask interaction",
        ),
    ] {
        let scene = scene_with(212, "SpriteRenderer", &fields, "{fileID: 0}");
        let message = scene
            .sprite_renderer(target(&scene))
            .unwrap_err()
            .to_string();
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn bodies_read_pongs_unity_2017_fields() {
    let fields = format!("{BODY}  m_Material: {{fileID: 6200000, guid: {BOUNCY}, type: 2}}\n");
    let scene = scene_with(50, "Rigidbody2D", &fields, "{fileID: 0}");
    let body = scene.rigidbody_2d(target(&scene)).unwrap();
    let material = body.material.unwrap();
    let expected = Rigidbody2D {
        body_type: BodyType::Dynamic,
        simulated: true,
        mass: 0.0001,
        gravity_scale: 0.,
        // Freeze position X and rotation.
        constraints: 5,
        interpolation: Interpolation::Interpolate,
        material: Some(material),
    };
    assert_eq!(body, expected);
    let bouncy = PhysicsMaterial {
        friction: 0.25,
        bounciness: 1.,
        ..PhysicsMaterial::DEFAULT
    };
    assert_eq!(scene.physics_material(material).unwrap(), bouncy);
    for (fields, expected) in [
        (
            BODY.replace("m_LinearDrag: 0", "m_LinearDrag: 0.5"),
            "damping",
        ),
        (
            BODY.replace("m_SleepingMode: 1", "m_SleepingMode: 2"),
            "start asleep",
        ),
        (
            BODY.replace("m_Constraints: 5", "m_Constraints: 8"),
            "m_Constraints",
        ),
        (
            format!("{BODY}  m_UseAutoMass: 1\n"),
            "automatic rigidbody mass",
        ),
        (
            format!(
                "{BODY}  m_Material: {{fileID: 2100000, guid: c0000000000000000000000000000006, type: 2}}\n"
            ),
            "not in the project",
        ),
    ] {
        let scene = scene_with(50, "Rigidbody2D", &fields, "{fileID: 0}");
        let message = scene.rigidbody_2d(target(&scene)).unwrap_err().to_string();
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn colliders_read_their_shape_and_shared_settings() {
    let scene = scene_with(61, "BoxCollider2D", BOX, "{fileID: 0}");
    let expected = Collider {
        settings: Collider2D {
            enabled: true,
            trigger: true,
            offset: [0.37, -0.2],
            material: None,
        },
        shape: ColliderShape::Box { size: [0.32, 2.56] },
    };
    assert_eq!(scene.collider(target(&scene)).unwrap(), expected);
    let circle = "  m_Enabled: 1\n  m_Radius: 0.25\n";
    let scene = scene_with(58, "CircleCollider2D", circle, "{fileID: 0}");
    let shape = scene.collider(target(&scene)).unwrap().shape;
    assert_eq!(shape, ColliderShape::Circle { radius: 0.25 });
    let scene = scene_with(
        19_719_996,
        "TilemapCollider2D",
        "  m_Enabled: 1\n",
        "{fileID: 0}",
    );
    assert_eq!(
        scene.collider(target(&scene)).unwrap().shape,
        ColliderShape::Tilemap
    );
}

#[test]
fn unsupported_collider_settings_fail_with_their_object() {
    let cases = [
        (
            61,
            "BoxCollider2D",
            BOX.replace("m_UsedByEffector: 0", "m_UsedByEffector: 1"),
            "effectors",
        ),
        (
            61,
            "BoxCollider2D",
            BOX.replace("m_EdgeRadius: 0", "m_EdgeRadius: 0.1"),
            "edge radius",
        ),
        (
            61,
            "BoxCollider2D",
            format!("{BOX}  m_CompositeOperation: 1\n"),
            "composite",
        ),
        (
            61,
            "BoxCollider2D",
            format!("{BOX}  m_ExcludeLayers:\n    serializedVersion: 2\n    m_Bits: 8\n"),
            "layer overrides",
        ),
        (
            61,
            "BoxCollider2D",
            format!("{BOX}  m_CallbackLayers:\n    serializedVersion: 2\n    m_Bits: 1\n"),
            "contact layer filters",
        ),
        (
            19_719_996,
            "TilemapCollider2D",
            "  m_ExtrusionFactor: 0.1\n".into(),
            "extrusion",
        ),
        (
            50,
            "Rigidbody2D",
            BODY.into(),
            "not a supported 2D collider",
        ),
    ];
    for (class, name, fields, expected) in cases {
        let scene = scene_with(class, name, &fields, "{fileID: 0}");
        let message = scene.collider(target(&scene)).unwrap_err().to_string();
        assert!(
            message.contains(&format!("{name} 12")) && message.contains(expected),
            "{message}"
        );
    }
}

#[test]
fn tilemap_renderers_and_camera_participants() {
    let fields = format!(
        "  m_Enabled: 1\n  m_Materials:\n  - {SPRITES_DEFAULT}\n  m_SortingOrder: 3\n  m_Mode: 0\n  m_SortOrder: 0\n"
    );
    let scene = scene_with(483_693_784, "TilemapRenderer", &fields, "{fileID: 0}");
    let renderer = scene.tilemap_renderer(target(&scene)).unwrap();
    assert_eq!(renderer.renderer.sorting_order, 3);
    for (fields, expected) in [
        (
            fields.replace("m_Mode: 0", "m_Mode: 1"),
            "Chunk render mode",
        ),
        (
            fields.replace("m_SortOrder: 0", "m_SortOrder: 2"),
            "bottom-left sort order",
        ),
    ] {
        let scene = scene_with(483_693_784, "TilemapRenderer", &fields, "{fileID: 0}");
        let message = scene
            .tilemap_renderer(target(&scene))
            .unwrap_err()
            .to_string();
        assert!(message.contains(expected), "{message}");
    }
    let scene = scene_with(92, "GUILayer", "  m_Enabled: 0\n", "{fileID: 0}");
    let (kind, enabled) = scene.camera_participant(target(&scene)).unwrap();
    assert_eq!(kind as i32, 92);
    assert!(!enabled);
}
