use std::fmt;

/// A Unity class ID, as listed in Unity's YAML class ID reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub i32);

macro_rules! classes {
    ($($name:ident = $id:literal => $text:literal),* $(,)?) => {
        impl ClassId {
            $(pub const $name: Self = Self($id);)*

            /// Unity's name for the class, when the reader knows it.
            #[must_use]
            pub fn name(self) -> Option<&'static str> {
                match self.0 {
                    $($id => Some($text),)*
                    _ => None,
                }
            }
        }
    };
}

classes! {
    GAME_OBJECT = 1 => "GameObject",
    TRANSFORM = 4 => "Transform",
    TIME_MANAGER = 5 => "TimeManager",
    AUDIO_MANAGER = 11 => "AudioManager",
    INPUT_MANAGER = 13 => "InputManager",
    CAMERA = 20 => "Camera",
    MATERIAL = 21 => "Material",
    MESH_RENDERER = 23 => "MeshRenderer",
    TEXTURE_2D = 28 => "Texture2D",
    GRAPHICS_SETTINGS = 30 => "GraphicsSettings",
    MESH_FILTER = 33 => "MeshFilter",
    MESH = 43 => "Mesh",
    QUALITY_SETTINGS = 47 => "QualitySettings",
    SHADER = 48 => "Shader",
    TEXT_ASSET = 49 => "TextAsset",
    RIGIDBODY_2D = 50 => "Rigidbody2D",
    RIGIDBODY = 54 => "Rigidbody",
    PHYSICS_MANAGER = 55 => "PhysicsManager",
    MESH_COLLIDER = 64 => "MeshCollider",
    BOX_COLLIDER = 65 => "BoxCollider",
    COMPUTE_SHADER = 72 => "ComputeShader",
    ANIMATION_CLIP = 74 => "AnimationClip",
    TAG_MANAGER = 78 => "TagManager",
    AUDIO_LISTENER = 81 => "AudioListener",
    AUDIO_SOURCE = 82 => "AudioSource",
    AUDIO_CLIP = 83 => "AudioClip",
    RENDER_TEXTURE = 84 => "RenderTexture",
    CUBEMAP = 89 => "Cubemap",
    AVATAR = 90 => "Avatar",
    ANIMATOR_CONTROLLER = 91 => "AnimatorController",
    SCRIPT_MAPPER = 94 => "ScriptMapper",
    ANIMATOR = 95 => "Animator",
    DELAYED_CALL_MANAGER = 98 => "DelayedCallManager",
    RENDER_SETTINGS = 104 => "RenderSettings",
    LIGHT = 108 => "Light",
    ANIMATION = 111 => "Animation",
    MONO_BEHAVIOUR = 114 => "MonoBehaviour",
    MONO_SCRIPT = 115 => "MonoScript",
    MONO_MANAGER = 116 => "MonoManager",
    TEXTURE_3D = 117 => "Texture3D",
    FLARE_LAYER = 124 => "FlareLayer",
    NAV_MESH_PROJECT_SETTINGS = 126 => "NavMeshProjectSettings",
    FONT = 128 => "Font",
    PLAYER_SETTINGS = 129 => "PlayerSettings",
    SPHERE_COLLIDER = 135 => "SphereCollider",
    CAPSULE_COLLIDER = 136 => "CapsuleCollider",
    SKINNED_MESH_RENDERER = 137 => "SkinnedMeshRenderer",
    BUILD_SETTINGS = 141 => "BuildSettings",
    ASSET_BUNDLE = 142 => "AssetBundle",
    RESOURCE_MANAGER = 147 => "ResourceManager",
    PRELOAD_DATA = 150 => "PreloadData",
    LIGHTMAP_SETTINGS = 157 => "LightmapSettings",
    TEXTURE_2D_ARRAY = 187 => "Texture2DArray",
    NAV_MESH_SETTINGS = 196 => "NavMeshSettings",
    PARTICLE_SYSTEM = 198 => "ParticleSystem",
    PARTICLE_SYSTEM_RENDERER = 199 => "ParticleSystemRenderer",
    LOD_GROUP = 205 => "LODGroup",
    SORTING_GROUP = 210 => "SortingGroup",
    SPRITE_RENDERER = 212 => "SpriteRenderer",
    SPRITE = 213 => "Sprite",
    ANIMATOR_OVERRIDE_CONTROLLER = 221 => "AnimatorOverrideController",
    CANVAS_RENDERER = 222 => "CanvasRenderer",
    CANVAS = 223 => "Canvas",
    RECT_TRANSFORM = 224 => "RectTransform",
    CANVAS_GROUP = 225 => "CanvasGroup",
    LIGHT_PROBES = 258 => "LightProbes",
    PLAYABLE_DIRECTOR = 320 => "PlayableDirector",
    VIDEO_CLIP = 329 => "VideoClip",
    SPRITE_ATLAS = 687_078_895 => "SpriteAtlas",
    LIGHTING_SETTINGS = 850_595_691 => "LightingSettings",
}

impl fmt::Display for ClassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => write!(f, "class {}", self.0),
        }
    }
}
