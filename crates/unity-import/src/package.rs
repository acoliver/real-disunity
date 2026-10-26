//! Assets that come from Unity itself or from packages rather than from the project.
//!
//! Their files are not in the game repositories and the built-in packages cannot be downloaded
//! at the games' pinned versions, so they are identified by GUID and fileID. Each entry below
//! records how the identity was established. Anything not listed stays an external or missing
//! reference, and consumers reject it.

mod md4;

use crate::guid::Guid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackageAsset {
    Script(PackageScript),
    Material(PackageMaterial),
    Font(PackageFont),
    Sprite(PackageSprite),
}

/// Engine and package component types that scenes reference by script.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackageScript {
    Text,
    CanvasScaler,
    GraphicRaycaster,
    EventSystem,
    StandaloneInputModule,
    Tile,
    Light2D,
    UniversalAdditionalCameraData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackageMaterial {
    /// Unity's built-in unlit, alpha-blended `Sprites-Default`.
    SpritesDefault,
    /// URP's `Sprite-Lit-Default`, lit by 2D lights.
    SpriteLitDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum PackageFont {
    /// Unity's built-in `Arial.ttf`.
    Arial,
}

/// Sprites that ship in Unity packages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum PackageSprite {
    /// The `Square` that the built-in 2D Sprite package 1.0.0 creates from the
    /// Create > 2D > Sprites menu, in its `v2/Square.png`.
    Square,
}

impl PackageSprite {
    /// The GUID of the texture the sprite belongs to.
    pub fn texture_guid(self) -> Guid {
        match self {
            Self::Square => SPRITE_SQUARE,
        }
    }

    /// The fileID references to the sprite use.
    pub fn file_id(self) -> i64 {
        match self {
            Self::Square => SQUARE_SPRITE,
        }
    }

    /// The package sprite whose texture is `guid`, if one is known.
    pub fn of_texture(guid: Guid) -> Option<Self> {
        (guid == SPRITE_SQUARE).then_some(Self::Square)
    }
}

impl PackageScript {
    /// The C# namespace and class name.
    pub fn type_name(self) -> (&'static str, &'static str) {
        match self {
            Self::Text => ("UnityEngine.UI", "Text"),
            Self::CanvasScaler => ("UnityEngine.UI", "CanvasScaler"),
            Self::GraphicRaycaster => ("UnityEngine.UI", "GraphicRaycaster"),
            Self::EventSystem => ("UnityEngine.EventSystems", "EventSystem"),
            Self::StandaloneInputModule => ("UnityEngine.EventSystems", "StandaloneInputModule"),
            Self::Tile => ("UnityEngine.Tilemaps", "Tile"),
            Self::Light2D => ("UnityEngine.Rendering.Universal", "Light2D"),
            Self::UniversalAdditionalCameraData => (
                "UnityEngine.Rendering.Universal",
                "UniversalAdditionalCameraData",
            ),
        }
    }
}

const fn guid(value: u128) -> Guid {
    Guid::from_u128(value)
}

/// `UnityEngine.UI.dll` as shipped with Unity 2017. Every Pong UI script fileID under it equals
/// [`script_file_id`] of a UI type, which establishes both the assembly and the types.
const UNITY_UI_DLL: Guid = guid(0xf705_55f1_44d8_491a_825f_0804_e09c_671c);
const UNITY_UI_SCRIPTS: [PackageScript; 5] = [
    PackageScript::Text,
    PackageScript::CanvasScaler,
    PackageScript::GraphicRaycaster,
    PackageScript::EventSystem,
    PackageScript::StandaloneInputModule,
];

/// Unity's built-in extra resources and default resources.
const BUILTIN_EXTRA: Guid = guid(0xe000_0000_0000_0000);
const DEFAULT_RESOURCES: Guid = guid(0xf000_0000_0000_0000);
/// The fileID of the `MonoScript` of a `.cs` file.
const SOURCE_SCRIPT: i64 = 11_500_000;
/// The fileID of the material in a `.mat` file.
const MATERIAL: i64 = 2_100_000;

/// `Editor/ObjectMenuCreation/DefaultAssets/Textures/v2/Square.png` in `com.unity.2d.sprite`
/// 1.0.0, built into Unity 6000.3.5f2, and its one sprite's `internalID`.
const SPRITE_SQUARE: Guid = guid(0x3119_25a0_02f4_447b_3a28_9271_69b8_3ea6);
const SQUARE_SPRITE: i64 = 7_482_667_652_216_324_306;

/// Fixed identities. Built-in Tile is named by Breakout's tile asset's own
/// `m_EditorClassIdentifier`. The URP entries are the GUIDs in URP 17's public package `.meta`
/// files, reported on issue #70. The scenes that use them hold exactly those types' fields. The
/// 2D Sprite package's Square is the one asset of its own `.meta` file in the editor build.
const FIXED: [(Guid, i64, PackageAsset); 7] = [
    (
        BUILTIN_EXTRA,
        13_312,
        PackageAsset::Script(PackageScript::Tile),
    ),
    (
        BUILTIN_EXTRA,
        10_102,
        PackageAsset::Font(PackageFont::Arial),
    ),
    (
        DEFAULT_RESOURCES,
        10_754,
        PackageAsset::Material(PackageMaterial::SpritesDefault),
    ),
    (
        guid(0x0737_97af_b82c_5a14_38f3_2886_6b10_b3f0),
        SOURCE_SCRIPT,
        PackageAsset::Script(PackageScript::Light2D),
    ),
    (
        guid(0xa794_41f3_48de_8974_3a29_39f4_d699_eac1),
        SOURCE_SCRIPT,
        PackageAsset::Script(PackageScript::UniversalAdditionalCameraData),
    ),
    (
        guid(0xa97c_1056_38bd_f8b4_a865_0670_310a_4cd3),
        MATERIAL,
        PackageAsset::Material(PackageMaterial::SpriteLitDefault),
    ),
    (
        SPRITE_SQUARE,
        SQUARE_SPRITE,
        PackageAsset::Sprite(PackageSprite::Square),
    ),
];

/// The package asset a `{fileID, guid}` reference names, if it is a known one.
pub fn identify(guid: Guid, file_id: i64) -> Option<PackageAsset> {
    if guid == UNITY_UI_DLL {
        return UNITY_UI_SCRIPTS
            .into_iter()
            .find(|script| {
                let (namespace, name) = script.type_name();
                script_file_id(namespace, name) == file_id
            })
            .map(PackageAsset::Script);
    }
    FIXED
        .iter()
        .find(|(known, id, _)| *known == guid && *id == file_id)
        .map(|(_, _, asset)| *asset)
}

/// The fileID Unity gives a script class compiled into a DLL: the first four bytes of
/// MD4(`"s\0\0\0"` + namespace + class name), read as a little-endian `i32`.
pub fn script_file_id(namespace: &str, name: &str) -> i64 {
    let mut message = b"s\0\0\0".to_vec();
    message.extend_from_slice(namespace.as_bytes());
    message.extend_from_slice(name.as_bytes());
    let hash = md4::digest(&message);
    i64::from(i32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]))
}
