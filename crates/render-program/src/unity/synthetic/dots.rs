//! A synthetic shader of the shape of a game's `DOTS_INSTANCING_ON` variant: a read-only raw buffer
//! `unity_DOTSInstanceData` at set 0, binding 5, a metadata block whose words are byte offsets
//! into it, and a block of visible-instance entries indexed by the instance index. The vertex
//! stage reads one matrix of four packed float3 columns, 48 bytes, at the metadata offset plus 48
//! times the visible entry. The plain variant draws the whole target in [`MARKER`].

use super::shaders::{no_cull, variant};
use super::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, VariantSpec, binding, constant_buffer,
    matrix_member, shader, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, ResourceBinding, ResourceKind, StructLayout, VariantParameters,
};

/// The keyword whose variant reads the batch.
pub const KEYWORD: &str = "DOTS_INSTANCING_ON";
/// The colour the plain variant draws.
pub const MARKER: [u8; 4] = [0, 255, 255, 255];
/// The per-instance property the synthetic shader reads its matrix by.
pub const METADATA: &str = "unity_ObjectToWorld";
/// The bytes in front of the first matrix, which the metadata word skips.
pub const HEADER: usize = 16;

/// What a variant's fragment stage draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fragment {
    /// The colour of the instance, or the marker for the plain variant.
    Colour,
    /// `unity_LightData.z` in the red channel.
    Light,
    /// The fourth component of the spherical harmonic `unity_SHAr` in the blue channel: the plain
    /// variant reads the builtin, the DOTS variant the default its globals block holds for it.
    Ambient,
}

const PLAIN: &str = "
struct VOut { @builtin(position) position: vec4<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position, 1.0);
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return vec4<f32>(0.0, 1.0, 1.0, 1.0); }";

/// A plain variant that places the quad by the draw's `unity_ObjectToWorld` and draws
/// `unity_LightData.z` in the red channel.
const LIT_PLAIN: &str = "
struct Draw { object_to_world: mat4x4<f32> }
struct Light { data: vec4<f32> }
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(7) var<uniform> light: Light;
struct VOut { @builtin(position) position: vec4<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>) -> VOut {
    var out: VOut;
    out.position = draw.object_to_world * vec4<f32>(position, 1.0);
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(light.data.z, 0.0, 0.0, 1.0);
}";

/// A plain variant that places the quad by the draw's `unity_ObjectToWorld` and draws the fourth
/// component of `unity_SHAr` in the blue channel.
const AMBIENT_PLAIN: &str = "
struct Draw { object_to_world: mat4x4<f32> }
struct Ambient { sh: vec4<f32> }
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(7) var<uniform> ambient: Ambient;
struct VOut { @builtin(position) position: vec4<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>) -> VOut {
    var out: VOut;
    out.position = draw.object_to_world * vec4<f32>(position, 1.0);
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, ambient.sh.w, 1.0);
}";

/// The binding of the constant buffer that holds `unity_LightData`, or the ambient value an
/// ambient variant reads, in a fragment stage.
const LIGHT_BINDING: u16 = 7;

/// The block of the defaults a DOTS variant reads for a builtin no instance gives, and the member
/// of it that holds the default of `unity_SHAr`.
const GLOBAL_VALUES: &str = "unity_DOTSInstanceGlobalValues";
const DOTS_SH_AR: &str = "unity_DOTS_SHAr";

/// The constant buffer and binding of `unity_LightData`.
fn light_parameters() -> (ConstantBuffer, ResourceBinding) {
    (
        constant_buffer("UnityLight", 16, vec![vector_member("unity_LightData", 0)]),
        binding(
            "UnityLight",
            slot(0, LIGHT_BINDING, FRAGMENT),
            ResourceKind::ConstantBuffer,
        ),
    )
}

/// The parameters of [`LIT_PLAIN`].
fn lit_plain_parameters() -> VariantParameters {
    let (light, light_binding) = light_parameters();
    VariantParameters {
        constant_buffers: vec![
            constant_buffer("Draw", 64, vec![matrix_member("unity_ObjectToWorld", 0)]),
            light,
        ],
        bindings: vec![
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
            light_binding,
        ],
    }
}

/// The parameters of [`AMBIENT_PLAIN`].
fn ambient_plain_parameters() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![
            constant_buffer("Draw", 64, vec![matrix_member("unity_ObjectToWorld", 0)]),
            constant_buffer("UnityAmbient", 16, vec![vector_member("unity_SHAr", 0)]),
        ],
        bindings: vec![
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
            binding(
                "UnityAmbient",
                slot(0, LIGHT_BINDING, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
        ],
    }
}

/// The WGSL of the DOTS variant, whose visible block holds `entries` entries, with the fragment
/// stage `fragment`.
fn dots_wgsl(entries: usize, reads_indirect: bool, fragment: Fragment) -> String {
    let (indirect, skipped) = if reads_indirect {
        (
            "@group(0) @binding(6) var<storage, read> indirect: array<u32>;",
            "f32(indirect[0])",
        )
    } else {
        ("", "0.0")
    };
    let (light, fragment) = match fragment {
        Fragment::Colour => ("", "in.color"),
        Fragment::Light => (
            "struct Light { data: vec4<f32> }
@group(0) @binding(7) var<uniform> light: Light;",
            "vec4<f32>(light.data.z, 0.0, 0.0, 1.0)",
        ),
        Fragment::Ambient => (
            "struct Defaults { sh: vec4<f32> }
@group(0) @binding(7) var<uniform> defaults: Defaults;",
            "vec4<f32>(0.0, 0.0, defaults.sh.w, 1.0)",
        ),
    };
    format!(
        "
struct Entry {{ data: vec4<u32> }}
struct Visible {{ entries: array<Entry, {entries}> }}
struct Words {{ to_world: i32, to_object: i32 }}
@group(1) @binding(2) var<uniform> visible: Visible;
@group(1) @binding(3) var<uniform> words: Words;
@group(0) @binding(5) var<storage, read> instances: array<f32>;
{indirect}
{light}
struct VOut {{
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
}}
fn column(base: u32, k: u32) -> vec3<f32> {{
    let at = base + 3u * k;
    return vec3<f32>(instances[at], instances[at + 1u], instances[at + 2u]);
}}
@vertex fn vs_main(
    @location(0) position: vec3<f32>,
    @builtin(instance_index) i: u32,
) -> VOut {{
    let element = visible.entries[i].data.x;
    let base = (u32(words.to_world) & 0x7fffffffu) / 4u + element * 12u;
    let moved = column(base, 0u) * position.x + column(base, 1u) * position.y
        + column(base, 2u) * position.z + column(base, 3u);
    var out: VOut;
    out.position = vec4<f32>(moved, 1.0);
    out.color = vec4<f32>(f32(element) / 8.0, 0.4, 0.6, 1.0 + {skipped});
    return out;
}}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {{ return {fragment}; }}"
    )
}

fn member(name: &str, kind: i32, columns: i32, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind,
        rows: 1,
        columns,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

/// The parameters of a DOTS variant: the metadata block, and the visible block of `entries`
/// entries of 16 bytes, which a player stores as a struct array.
fn dots_parameters(entries: i32, reads_indirect: bool, fragment: Fragment) -> VariantParameters {
    let metadata = constant_buffer(
        "UnityDOTSInstancing_BuiltinPropertyMetadata",
        8,
        vec![
            member(
                "unity_DOTSInstancingF48_Metadataunity_ObjectToWorld",
                1,
                1,
                0,
            ),
            member(
                "unity_DOTSInstancingF48_Metadataunity_WorldToObject",
                1,
                1,
                4,
            ),
        ],
    );
    let mut visible = constant_buffer(
        "UnityInstancingDOTS_InstanceVisibility",
        entries * 16,
        Vec::new(),
    );
    visible.structs.push(StructLayout {
        name: "unity_DOTSVisibleInstances".to_owned(),
        index: 0,
        array_size: entries,
        size: 16,
        members: vec![member("VisibleData", 1, 4, 0)],
    });
    let mut parameters = VariantParameters {
        constant_buffers: vec![metadata, visible],
        bindings: vec![
            binding(
                "UnityInstancingDOTS_InstanceVisibility",
                slot(1, 2, VERTEX),
                ResourceKind::ConstantBuffer,
            ),
            binding(
                "UnityDOTSInstancing_BuiltinPropertyMetadata",
                slot(1, 3, VERTEX),
                ResourceKind::ConstantBuffer,
            ),
            binding(
                "unity_DOTSInstanceData",
                slot(0, 5, VERTEX),
                ResourceKind::Buffer,
            ),
        ],
    };
    if reads_indirect {
        parameters.bindings.push(binding(
            "unity_DOTSInstancing_IndirectInstanceVisibility",
            slot(0, 6, VERTEX),
            ResourceKind::Buffer,
        ));
    }
    match fragment {
        Fragment::Colour => {}
        Fragment::Light => {
            let (light, light_binding) = light_parameters();
            parameters.constant_buffers.push(light);
            parameters.bindings.push(light_binding);
        }
        Fragment::Ambient => {
            parameters.constant_buffers.push(constant_buffer(
                GLOBAL_VALUES,
                16,
                vec![vector_member(DOTS_SH_AR, 0)],
            ));
            parameters.bindings.push(binding(
                GLOBAL_VALUES,
                slot(0, LIGHT_BINDING, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ));
        }
    }
    parameters
}

fn spec(keywords: &[&'static str], wgsl: &str, parameters: VariantParameters) -> VariantSpec {
    VariantSpec {
        channels: vec![[0, 0]],
        ..variant(keywords, wgsl, "fs_main", parameters)
    }
}

/// A one-pass shader with the `LIGHTMODE` tag `Alpha` of the `plain` variant and a `DOTS` variant.
fn two_variants(plain: VariantSpec, dots: VariantSpec) -> SyntheticShader {
    no_cull(shader(
        &[KEYWORD],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![plain, dots],
        }],
    ))
}

/// A shader whose plain variant draws the whole target in [`MARKER`], and whose DOTS variant has
/// a visible block of `entries` entries. Its pass has the `LIGHTMODE` tag `Alpha`.
pub fn dots_passes(entries: usize, reads_indirect: bool) -> SyntheticShader {
    let plain = spec(
        &[],
        PLAIN,
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: Vec::new(),
        },
    );
    let dots = spec(
        &[KEYWORD],
        &dots_wgsl(entries, reads_indirect, Fragment::Colour),
        dots_parameters(
            i32::try_from(entries).unwrap(),
            reads_indirect,
            Fragment::Colour,
        ),
    );
    two_variants(plain, dots)
}

/// A shader that draws `unity_LightData.z` in the red channel. Its plain variant places the quad
/// by the draw's `unity_ObjectToWorld`; its DOTS variant, with a visible block of `entries`
/// entries, places it as [`dots_passes`] does.
pub fn lit_dots_passes(entries: usize) -> SyntheticShader {
    let plain = spec(&[], LIT_PLAIN, lit_plain_parameters());
    let dots = spec(
        &[KEYWORD],
        &dots_wgsl(entries, false, Fragment::Light),
        dots_parameters(i32::try_from(entries).unwrap(), false, Fragment::Light),
    );
    two_variants(plain, dots)
}

/// A shader that draws the fourth component of `unity_SHAr` in the blue channel. Its plain variant
/// reads the builtin; its DOTS variant, with a visible block of `entries` entries and the metadata
/// of [`dots_passes`], reads the default of its globals block [`GLOBAL_VALUES`].
pub fn ambient_dots_passes(entries: usize) -> SyntheticShader {
    let plain = spec(&[], AMBIENT_PLAIN, ambient_plain_parameters());
    let dots = spec(
        &[KEYWORD],
        &dots_wgsl(entries, false, Fragment::Ambient),
        dots_parameters(i32::try_from(entries).unwrap(), false, Fragment::Ambient),
    );
    two_variants(plain, dots)
}

/// The matrix that moves the quad of clip space into cell `cell` of `cells`: scale `1 / cells`
/// in x and a translation to the cell's center, as four columns of three floats.
pub fn matrix(cell: usize, cells: usize) -> [f32; 12] {
    let cells = f32::from(u16::try_from(cells).unwrap());
    let center = (2.0 * f32::from(u16::try_from(cell).unwrap()) + 1.0) / cells - 1.0;
    [
        1.0 / cells,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        1.0,
        center,
        0.0,
        0.0,
    ]
}

/// The metadata word of the matrices: bit 31 set, then the byte offset of the first.
pub fn word() -> u32 {
    0x8000_0000 | u32::try_from(HEADER).unwrap()
}
