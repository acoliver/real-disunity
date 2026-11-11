//! The layout of `ComputeShader` objects: the kernels a compute shader declares and, for each
//! platform, the compiled variants of every kernel with the resources they bind.
#[path = "support/compute_bytes.rs"]
mod compute_bytes;
#[path = "support/decode.rs"]
mod decode;
mod support;

use compute_bytes::{
    Binding, BufferLayout, GLSL, Kernel, Member, Platform, SPIRV, Variant, compute_shader_bytes,
};
use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    compute_shader::{ComputeBinding, ComputeShader, ComputeSlot},
    write::ObjectWriter,
};

/// The header of a SPIR-V module with no instructions: magic, version, generator, ID bound and
/// the reserved word, little-endian.
const MODULE: [u8; 20] = [
    0x03, 0x02, 0x23, 0x07, 0x00, 0x00, 0x01, 0x00, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0,
];

fn decoded(bytes: Vec<u8>) -> Result<ComputeShader, SerializedError> {
    decode_one::<ComputeShader>(ClassId::COMPUTE_SHADER, bytes, false)
}

fn one_kernel(name: &str, size: [u32; 3]) -> Platform {
    let variant = Variant::new(&MODULE, size);
    Platform::new(SPIRV, 0, vec![Kernel::single(name, variant)])
}

/// The variant of the terrain read-back's shape: two sampled textures with separate samplers, a
/// writable buffer and a constant buffer in set 1.
fn read_back_variant() -> Variant {
    let mut variant = Variant::new(&MODULE, [8, 8, 1]);
    variant.constant_buffers = vec![Binding::constant_buffer("CGlobals", 1, 0)];
    variant.textures = vec![
        Binding::texture("_Heights", 0, Some(3), 2),
        Binding::texture("_Clutter", 1, Some(4), 2),
    ];
    variant.writable = vec![Binding::writable("_Output", 2)];
    variant
}

fn read_back() -> Vec<u8> {
    let mut vulkan = Platform::new(
        SPIRV,
        0,
        vec![Kernel::single("TerrainReadBackCS", read_back_variant())],
    );
    vulkan.buffers = vec![BufferLayout {
        name: "CGlobals".into(),
        size: 8,
        members: vec![
            Member {
                name: "_OutputHeight".into(),
                words: [1, 4, 0, 1, 1],
            },
            Member {
                name: "_OutputWidth".into(),
                words: [1, 0, 0, 1, 1],
            },
        ],
    }];
    let glsl = Platform::new(
        GLSL,
        10,
        vec![Kernel::single(
            "TerrainReadBackCS",
            Variant::new(b"#version 430", [8, 8, 1]),
        )],
    );
    compute_shader_bytes("TerrainReadBackShader", &[glsl, vulkan], false)
}

#[test]
fn reads_every_platform_with_its_renderer_and_level() {
    let shader = decoded(read_back()).unwrap();
    assert_eq!(shader.name, "TerrainReadBackShader");
    let seen: Vec<_> = shader
        .platforms
        .iter()
        .map(|platform| (platform.renderer, platform.level, platform.kernels.len()))
        .collect();
    assert_eq!(seen, [(GLSL, 10, 1), (SPIRV, 0, 1)]);
    assert!(shader.platforms.iter().all(|platform| platform.resolved));
    assert_eq!(shader.platform(SPIRV).unwrap().renderer, SPIRV);
    assert!(shader.platform(4).is_none());
}

#[test]
fn reads_the_resources_a_variant_binds_with_their_names_and_places() {
    let shader = decoded(read_back()).unwrap();
    let kernel = &shader.platform(SPIRV).unwrap().kernels[0];
    assert_eq!(kernel.name, "TerrainReadBackCS");
    let variant = &kernel.variants[0];
    assert_eq!(&variant.code[..], &MODULE);
    assert_eq!(variant.thread_group_size, [8, 8, 1]);

    let places = |list: &[ComputeBinding]| {
        list.iter()
            .map(|b| (b.name.clone(), b.slot.set(), b.slot.binding(), b.sampler))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        places(&variant.constant_buffers),
        [("CGlobals".to_owned(), 1, 0, None)]
    );
    assert_eq!(
        places(&variant.textures),
        [
            ("_Heights".to_owned(), 0, 0, Some(3)),
            ("_Clutter".to_owned(), 0, 1, Some(4)),
        ]
    );
    assert_eq!(
        places(&variant.writable),
        [("_Output".to_owned(), 0, 2, None)]
    );
    assert!(variant.buffers.is_empty() && variant.samplers.is_empty());
    assert!(variant.textures.iter().all(|t| t.dimension == Some(2)));
    assert!(variant.textures.iter().all(|t| t.slot.is_texture()));
    assert!(variant.writable[0].slot.is_writable());
    assert!(!variant.writable[0].slot.is_image());
}

#[test]
fn reads_the_constant_buffer_layouts_of_a_platform() {
    let shader = decoded(read_back()).unwrap();
    let buffers = &shader.platform(SPIRV).unwrap().constant_buffers;
    assert_eq!(buffers.len(), 1);
    assert_eq!((buffers[0].name.as_str(), buffers[0].size), ("CGlobals", 8));
    let members: Vec<_> = buffers[0]
        .members
        .iter()
        .map(|m| {
            (
                m.name.as_str(),
                m.kind,
                m.offset,
                m.array_size,
                m.rows,
                m.columns,
            )
        })
        .collect();
    assert_eq!(
        members,
        [
            ("_OutputHeight", 1, 4, 0, 1, 1),
            ("_OutputWidth", 1, 0, 0, 1, 1),
        ]
    );
    assert!(shader.platform(GLSL).unwrap().constant_buffers.is_empty());
}

#[test]
fn reads_built_in_samplers_buffers_and_storage_images() {
    let mut variant = Variant::new(&MODULE, [4, 4, 4]);
    variant.samplers = vec![(0x54, 4), (0x55, 2)];
    variant.buffers = vec![Binding::buffer("_Input", 2)];
    variant.writable = vec![Binding::image("_Volume", 5, 3)];
    let platform = Platform::new(SPIRV, 0, vec![Kernel::single("K", variant)]);
    let shader = decoded(compute_shader_bytes("Mixed", &[platform], false)).unwrap();
    let variant = &shader.platforms[0].kernels[0].variants[0];
    let samplers: Vec<_> = variant
        .samplers
        .iter()
        .map(|s| (s.state, s.bind_point))
        .collect();
    assert_eq!(samplers, [(0x54, 4), (0x55, 2)]);
    assert_eq!(variant.buffers[0].slot, ComputeSlot(2));
    let image = &variant.writable[0];
    assert_eq!((image.slot.binding(), image.dimension), (5, Some(3)));
    assert!(image.slot.is_image() && image.slot.is_writable() && image.slot.is_texture());
}

#[test]
fn keeps_the_kernels_in_the_order_the_asset_lists_them() {
    let kernels = ["KGather", "KClear", "KMerge"]
        .into_iter()
        .zip([[16, 16, 1], [16, 1, 1], [4, 4, 4]])
        .map(|(name, size)| Kernel::single(name, Variant::new(&MODULE, size)))
        .collect();
    let bytes = compute_shader_bytes("Histogram", &[Platform::new(SPIRV, 0, kernels)], false);
    let shader = decoded(bytes).unwrap();
    let seen: Vec<_> = shader
        .kernels()
        .iter()
        .map(|kernel| (kernel.name.as_str(), kernel.thread_group_size()))
        .collect();
    assert_eq!(
        seen,
        [
            ("KGather", Some([16, 16, 1])),
            ("KClear", Some([16, 1, 1])),
            ("KMerge", Some([4, 4, 4])),
        ]
    );
}

/// A kernel of four variants: tile 8 and 16, each with and without a flip.
fn keyworded() -> Kernel {
    let sizes = [[1, 1, 1], [2, 1, 1], [3, 1, 1], [4, 1, 1]];
    let mut variants: Vec<Variant> = sizes
        .into_iter()
        .map(|size| Variant::new(&MODULE, size))
        .collect();
    // The list after a variant that is not the kernel's last: empty, or one empty name.
    variants[1].following = vec![String::new()];
    let combo = |keywords: &str, index| (keywords.to_owned(), index);
    Kernel {
        name: "Tiled".into(),
        header: vec![0],
        variants,
        keyword_variants: vec![
            combo("TILE_8", 0),
            combo("TILE_8 FLIP", 1),
            combo("TILE_16", 2),
            combo("TILE_16 FLIP", 3),
        ],
        keyword_names: ["TILE_8", "TILE_16", "FLIP"].map(String::from).to_vec(),
    }
}

#[test]
fn reads_past_every_variant_of_a_kernel_to_find_the_next_one() {
    let platform = Platform::new(
        SPIRV,
        0,
        vec![
            keyworded(),
            Kernel::single("After", Variant::new(&MODULE, [32, 1, 1])),
        ],
    );
    let shader = decoded(compute_shader_bytes("Variants", &[platform], false)).unwrap();
    let kernels = shader.kernels();
    assert_eq!(kernels.len(), 2);
    let sizes: Vec<_> = kernels[0]
        .variants
        .iter()
        .map(|v| v.thread_group_size[0])
        .collect();
    assert_eq!(sizes, [1, 2, 3, 4]);
    assert_eq!(kernels[0].variants[1].following, [""]);
    assert!(kernels[0].variants[0].following.is_empty());
    assert!(kernels[0].variants[3].following.is_empty());
    assert_eq!(kernels[0].keyword_names, ["TILE_8", "TILE_16", "FLIP"]);
    assert_eq!(kernels[0].keyword_variants.len(), 4);
    assert_eq!(kernels[1].name, "After");
    assert_eq!(kernels[1].thread_group_size(), Some([32, 1, 1]));
    // The size of the kernel is its first variant's.
    assert_eq!(kernels[0].thread_group_size(), Some([1, 1, 1]));
}

#[test]
fn chooses_the_variant_for_the_enabled_keywords_in_the_order_the_kernel_names_them() {
    let platform = Platform::new(SPIRV, 0, vec![keyworded()]);
    let shader = decoded(compute_shader_bytes("Tiled", &[platform], false)).unwrap();
    let kernel = &shader.kernels()[0];
    let chosen = |keywords: &[&str]| {
        kernel
            .variant_for(keywords)
            .map(|variant| variant.thread_group_size[0])
    };
    assert_eq!(chosen(&["TILE_8"]), Some(1));
    assert_eq!(chosen(&["FLIP", "TILE_8"]), Some(2));
    assert_eq!(chosen(&["TILE_16"]), Some(3));
    assert_eq!(chosen(&["FLIP", "TILE_16"]), Some(4));
    // No combination without a tile size, and no such keyword.
    assert_eq!(chosen(&[]), None);
    assert_eq!(chosen(&["TILE_8", "OTHER"]), None);
}

#[test]
fn a_kernel_without_keywords_has_its_one_variant_for_no_keywords() {
    let shader = decoded(read_back()).unwrap();
    let kernel = &shader.platform(SPIRV).unwrap().kernels[0];
    assert_eq!(kernel.variant_for(&[]), Some(&kernel.variants[0]));
    assert_eq!(kernel.variant_for(&["ANY"]), None);
}

#[test]
fn a_kernel_that_states_no_size_has_none() {
    let shader = decoded(compute_shader_bytes(
        "Unsized",
        &[one_kernel("Main", [0, 0, 0])],
        false,
    ));
    assert_eq!(shader.unwrap().kernels()[0].thread_group_size(), None);
}

#[test]
fn a_shader_with_no_platforms_has_no_kernels() {
    let shader = decoded(compute_shader_bytes("Empty", &[], false)).unwrap();
    assert!(shader.platforms.is_empty());
    assert!(shader.kernels().is_empty());
}

#[test]
fn a_shader_with_only_a_vulkan_platform_reads() {
    let bytes = compute_shader_bytes("OnlyVulkan", &[one_kernel("Main", [2, 2, 1])], false);
    let shader = decoded(bytes).unwrap();
    assert_eq!(shader.platforms.len(), 1);
    assert_eq!(shader.kernels()[0].thread_group_size(), Some([2, 2, 1]));
}

#[test]
fn truncating_the_object_anywhere_is_an_error() {
    let bytes = read_back();
    for length in 0..bytes.len() {
        assert!(decoded(bytes[..length].to_vec()).is_err(), "{length}");
    }
}

#[test]
fn bytes_after_the_last_platform_are_an_error() {
    let mut bytes = read_back();
    bytes.extend([0; 4]);
    assert!(matches!(
        decoded(bytes),
        Err(SerializedError::TrailingBytes { .. })
    ));
}

#[test]
fn a_huge_count_is_refused_before_anything_is_allocated() {
    // Platforms, kernels, variants and a code length that the object cannot hold.
    let platforms = |w: &mut ObjectWriter| {
        w.string("Huge").unwrap();
        w.u32(1).u32(SPIRV).u32(0);
    };
    let mut w = ObjectWriter::new(false);
    w.string("Huge").unwrap();
    w.u32(u32::MAX / 2);
    assert!(matches!(
        decoded(w.finish()),
        Err(SerializedError::InvalidCount { .. })
    ));

    let mut w = ObjectWriter::new(false);
    platforms(&mut w);
    w.u32(u32::MAX / 2);
    assert!(matches!(
        decoded(w.finish()),
        Err(SerializedError::InvalidCount { .. })
    ));

    let mut w = ObjectWriter::new(false);
    platforms(&mut w);
    w.u32(1).string("Main").unwrap();
    w.u32(u32::MAX / 2);
    assert!(matches!(
        decoded(w.finish()),
        Err(SerializedError::InvalidCount { .. })
    ));
}

#[test]
fn a_thread_group_size_that_is_not_three_values_is_not_understood() {
    let mut bytes = compute_shader_bytes("Odd", &[one_kernel("Main", [8, 8, 1])], false);
    // The group size is the array [3, 8, 8, 1] before the requirements word 50241: make it two.
    let at = bytes
        .windows(4)
        .position(|word| word == 50241_u32.to_le_bytes())
        .unwrap()
        - 16;
    bytes[at..at + 4].copy_from_slice(&2_u32.to_le_bytes());
    let result = decoded(bytes);
    assert!(
        matches!(result, Err(SerializedError::InvalidComputeShader { .. })),
        "{result:?}"
    );
}

#[test]
fn a_kernel_without_variants_is_not_understood() {
    let kernel = Kernel {
        variants: Vec::new(),
        ..Kernel::single("Bare", Variant::new(&MODULE, [1, 1, 1]))
    };
    let platform = Platform::new(SPIRV, 0, vec![kernel]);
    let result = decoded(compute_shader_bytes("Bare", &[platform], false));
    assert!(
        matches!(result, Err(SerializedError::InvalidComputeShader { .. })),
        "{result:?}"
    );
}

#[test]
fn a_big_endian_shader_reads_the_same() {
    let platform = one_kernel("Main", [4, 2, 1]);
    let bytes = compute_shader_bytes("Big", &[platform], true);
    let shader = decode_one::<ComputeShader>(ClassId::COMPUTE_SHADER, bytes, true).unwrap();
    assert_eq!(shader.kernels()[0].thread_group_size(), Some([4, 2, 1]));
    assert_eq!(&shader.kernels()[0].variants[0].code[..], &MODULE);
}

#[test]
fn reads_a_shader_laid_out_word_by_word() {
    // One platform with one kernel with one variant that binds nothing, as 32-bit words:
    // the name, the platform count, the platform's renderer and level, the kernel count and name,
    // the variant count, the header words, five empty binding lists, the code, the group size,
    // the requirements, the empty keyword list, then the kernel's table and the platform's tail.
    let mut w = ObjectWriter::new(false);
    w.string("Raw").unwrap();
    w.u32(1).u32(21).u32(0).u32(1);
    w.string("K").unwrap();
    w.u32(1).u32(1).u32(0);
    for _ in 0..5 {
        w.u32(0);
    }
    w.byte_array(&MODULE).unwrap();
    w.u32(3).u32(2).u32(3).u32(4);
    w.u32(50241).u32(0);
    w.u32(1).string("").unwrap().u32(0);
    w.u32(0).u32(0).u32(0);
    w.u32(0).u32(1);
    let shader = decoded(w.finish()).unwrap();
    let kernel = &shader.platforms[0].kernels[0];
    assert_eq!(kernel.thread_group_size(), Some([2, 3, 4]));
    assert_eq!(kernel.variants[0].requirements, 50241);
    assert_eq!(kernel.keyword_variants, [(String::new(), 0)]);
    assert_eq!(kernel.header, [0]);
}

#[test]
fn a_raw_module_gives_its_words() {
    let shader = decoded(read_back()).unwrap();
    let variant = &shader.platform(SPIRV).unwrap().kernels[0].variants[0];
    assert_eq!(
        variant.module_words().unwrap(),
        [0x0723_0203, 0x0001_0000, 0, 1, 0]
    );
    let text = &shader.platform(GLSL).unwrap().kernels[0].variants[0];
    assert!(text.module_words().is_err());
}
