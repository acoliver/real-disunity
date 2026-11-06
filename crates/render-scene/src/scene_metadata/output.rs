//! The screen metadata a geometry pipeline set writes, and the shader text that selects it.

/// Present in the object source of pipelines that write the normal and the reflection material
/// together, which is how `ScenePipelines` knows to build two colour targets for them.
pub(crate) const MERGED_MARKER: &str = "struct SceneMetadataOutput";

/// Which screen metadata one set of geometry pipelines writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetadataOutput {
    /// The view-space normal, which SSAO reads from the depth and normal prepass.
    Normal,
    /// The normal at location 0 and the reflection material at location 1, so one draw feeds
    /// both consumers.
    Merged,
    /// The reflection material alone. Only tests build it, as the reference for [`Self::Merged`].
    #[cfg(test)]
    Material,
}

const OBJECT_NORMAL: &str = "vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0)";
const OBJECT_MATERIAL: &str =
    "vec4<f32>(scene_material_f0(albedo.rgb, metalness, DIELECTRIC_F0), roughness)";
const TERRAIN_NORMAL: &str =
    "vec4<f32>(normalize((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0)";
const TERRAIN_MATERIAL: &str = "vec4<f32>(scene_material_f0(projected.albedo, projected.properties.y, DIELECTRIC_F0), projected.properties.x)";

const TERRAIN_RETURN: &str =
    "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), 1.0);";
const TERRAIN_SIGNATURE: &str =
    "fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32>";
const MERGED_FIELDS: &str = "{ @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, }";

const GEOMETRY: &str = include_str!("geometry.wgsl");
const ENTRIES: &str = include_str!("entries.wgsl");
const MERGED_ENTRIES: &str = include_str!("merged_entries.wgsl");

fn fragment(kind: &str, expression: &str, entries: &str) -> String {
    let body = GEOMETRY
        .replace("METADATA_TYPE", kind)
        .replace("METADATA_OUTPUT", expression);
    format!("{body}\n{entries}")
}

/// The object fragment stage with a single colour output computed by `expression`.
pub(crate) fn single_fragment(expression: &str) -> String {
    fragment("vec4<f32>", expression, ENTRIES)
}

impl MetadataOutput {
    /// The object fragment stage appended to the stock object source.
    pub(crate) fn object_fragment(self) -> String {
        match self {
            Self::Normal => single_fragment(OBJECT_NORMAL),
            Self::Merged => fragment(
                "SceneMetadataOutput",
                &format!("SceneMetadataOutput({OBJECT_NORMAL}, {OBJECT_MATERIAL})"),
                MERGED_ENTRIES,
            ),
            #[cfg(test)]
            Self::Material => single_fragment(OBJECT_MATERIAL),
        }
    }

    /// The terrain source whose fragment stage writes this metadata instead of a lit colour.
    pub(crate) fn terrain_source(self, stock: &str) -> String {
        let (value, merged) = match self {
            Self::Normal => (TERRAIN_NORMAL.to_owned(), false),
            Self::Merged => (
                format!("SceneMetadataOutput({TERRAIN_NORMAL}, {TERRAIN_MATERIAL})"),
                true,
            ),
            #[cfg(test)]
            Self::Material => (TERRAIN_MATERIAL.to_owned(), false),
        };
        assert_eq!(stock.matches(TERRAIN_RETURN).count(), 1);
        let mut source = stock.replace(TERRAIN_RETURN, &format!("return {value};"));
        if merged {
            assert_eq!(source.matches(TERRAIN_SIGNATURE).count(), 1);
            source = source.replace(
                TERRAIN_SIGNATURE,
                "fn fs_terrain(input: TerrainVertexOutput) -> SceneMetadataOutput",
            );
            source = format!("{source}\n{MERGED_MARKER} {MERGED_FIELDS}\n");
        }
        source
    }
}
