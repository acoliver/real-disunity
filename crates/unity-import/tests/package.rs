use std::fmt::Write;

#[path = "support/assets.rs"]
mod assets;
#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/objects.rs"]
mod objects;

use fixture::Fixture;
use objects::{game_object, import, transform};
use worldrunner_unity_import::{
    PackageAsset, PackageMaterial, PackageScript, Reference, RendererMaterial, Scene,
    package::{identify, script_file_id},
};

const UNITY_UI: &str = "f70555f144d8491a825f0804e09c671c";
const URP_LIT: &str = "a97c105638bdf8b4a8650670310a4cd3";

#[test]
fn dll_script_file_ids_match_the_ids_unity_wrote_for_pongs_ui() {
    // These fileIDs are the ones in Pong's original scene and Scores prefab.
    let cases = [
        (708_705_254, PackageScript::Text),
        (1_980_459_831, PackageScript::CanvasScaler),
        (1_301_386_320, PackageScript::GraphicRaycaster),
        (-619_905_303, PackageScript::EventSystem),
        (1_077_351_063, PackageScript::StandaloneInputModule),
    ];
    for (file_id, script) in cases {
        let (namespace, name) = script.type_name();
        assert_eq!(script_file_id(namespace, name), file_id, "{name}");
        assert_eq!(
            identify(UNITY_UI.parse().unwrap(), file_id),
            Some(PackageAsset::Script(script))
        );
    }
    assert_eq!(identify(UNITY_UI.parse().unwrap(), 12_345), None);
}

#[test]
fn fixed_identities_need_both_guid_and_file_id() {
    let lit = URP_LIT.parse().unwrap();
    assert_eq!(
        identify(lit, 2_100_000),
        Some(PackageAsset::Material(PackageMaterial::SpriteLitDefault))
    );
    assert_eq!(identify(lit, 2_100_001), None);
    let builtin = "0000000000000000e000000000000000".parse().unwrap();
    assert_eq!(
        identify(builtin, 13_312),
        Some(PackageAsset::Script(PackageScript::Tile))
    );
    assert_eq!(identify(builtin, 10_001), None);
}

fn holder(references: &[(&str, &str)]) -> String {
    let mut fields = String::new();
    for (key, reference) in references {
        writeln!(fields, "  {key}: {reference}").unwrap();
    }
    [
        game_object(1, "Holder", &[2, 3]),
        transform(2, 1, 0.0, 0),
        format!("--- !u!114 &3\nMonoBehaviour:\n  m_GameObject: {{fileID: 1}}\n{fields}"),
    ]
    .concat()
}

fn field(scene: &Scene, key: &str) -> Reference {
    let (_, object) = scene
        .objects()
        .find(|(_, object)| object.class_id == 114)
        .unwrap();
    object.fields.get(key).unwrap().as_reference().unwrap()
}

#[test]
fn scene_references_resolve_project_files_before_packages_before_built_ins() {
    let fixture = Fixture::new("2017.1.0f3");
    let text = format!("{{fileID: 708705254, guid: {UNITY_UI}, type: 3}}");
    let unknown_ui = format!("{{fileID: 1, guid: {UNITY_UI}, type: 3}}");
    let lit = format!("{{fileID: 2100000, guid: {URP_LIT}, type: 2}}");
    let cookie = "{fileID: 10001, guid: 0000000000000000e000000000000000, type: 0}";
    let scene = holder(&[
        ("text", &text),
        ("unknownUi", &unknown_ui),
        ("lit", &lit),
        ("cookie", cookie),
    ]);
    let scene = import(&fixture, "Assets/Main.unity", &scene).unwrap();
    assert_eq!(
        field(&scene, "text"),
        Reference::Package(PackageAsset::Script(PackageScript::Text))
    );
    assert!(matches!(
        field(&scene, "unknownUi"),
        Reference::Missing { file_id: 1, .. }
    ));
    assert_eq!(
        field(&scene, "lit"),
        Reference::Package(PackageAsset::Material(PackageMaterial::SpriteLitDefault))
    );
    assert!(matches!(
        field(&scene, "cookie"),
        Reference::External {
            file_id: 10_001,
            ..
        }
    ));

    let vendored = Fixture::new("6000.3.5f2");
    vendored.asset(
        "Assets/Lit.mat",
        URP_LIT,
        "NativeFormatImporter",
        "--- !u!21 &2100000\nMaterial:\n  m_Name: Lit\n",
    );
    let scene = import(&vendored, "Assets/Main.unity", &holder(&[("lit", &lit)])).unwrap();
    assert!(
        matches!(field(&scene, "lit"), Reference::Object(_)),
        "a project file with the GUID is used instead of the package identity"
    );
}

#[test]
fn renderers_resolve_package_sprite_materials() {
    let fixture = Fixture::new("6000.3.5f2");
    let scene = [
        game_object(1, "Sprite", &[2, 3]),
        transform(2, 1, 0.0, 0),
        format!("--- !u!212 &3\nSpriteRenderer:\n  m_GameObject: {{fileID: 1}}\n  m_Materials:\n  - {{fileID: 2100000, guid: {URP_LIT}, type: 2}}\n"),
    ]
    .concat();
    let scene = import(&fixture, "Assets/Main.unity", &scene).unwrap();
    let (renderer, _) = scene.objects().find(|(_, o)| o.class_id == 212).unwrap();
    assert_eq!(
        scene.renderer_material(renderer).unwrap(),
        RendererMaterial::SpriteLitDefault
    );
}
