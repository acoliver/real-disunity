use super::*;
use crate::{SceneFadeCoverage, SceneSpecularCubeFormat, SceneSpecularPreparation, TextureSampler};

#[test]
fn released_texels_cannot_describe_an_unaddressable_source_payload() {
    let descriptor = SceneTextureDescriptor::Texels(SceneTexelDescriptor {
        format: TexelFormat::Rgba16Float,
        color_space: TextureColorSpace::Linear,
        width: 1 << 30,
        height: 1 << 30,
        mips: 1,
        logical_bytes: 1 << 63,
    });
    assert!(
        single_release(descriptor).is_err(),
        "an exact mip layout must still fit an addressable source allocation"
    );
}

fn released(descriptor: SceneTextureDescriptor) -> SceneTexture {
    SceneTexture::Released(descriptor)
}

fn texel_descriptor() -> SceneTexelDescriptor {
    SceneTexelDescriptor {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 2,
        logical_bytes: 20,
    }
}

fn dds_descriptor() -> SceneDdsDescriptor {
    SceneDdsDescriptor {
        format: SceneDdsFormat::Bc7Rgba,
        color_space: TextureColorSpace::Linear,
        width: 8,
        height: 8,
        mips: 4,
        logical_bytes: 148 + 112,
    }
}

fn cube_descriptor() -> SceneCubeDescriptor {
    SceneCubeDescriptor {
        format: SceneSpecularCubeFormat::Bc6hRgbUfloat,
        size: 4,
        mips: 3,
        logical_bytes: 288,
        preparation: None,
    }
}

fn material_parts(albedo: u32) -> SceneMaterialParts {
    SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 0,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    }
}

fn material(albedo: u32) -> SceneMaterial {
    SceneMaterial::new(material_parts(albedo)).unwrap()
}

#[test]
fn well_formed_descriptors_validate_inside_a_table() {
    let table = SceneResources::new(
        vec![
            released(SceneTextureDescriptor::Texels(texel_descriptor())),
            released(SceneTextureDescriptor::Dds(dds_descriptor())),
            released(SceneTextureDescriptor::SpecularCube(cube_descriptor())),
        ],
        vec![material(0), material(1)],
    )
    .unwrap();
    assert_eq!(table.logical_texture_bytes(), 20 + 260 + 288);
    assert_eq!(table.retained_texture_source_bytes(), 0);
    assert_eq!(table.textures()[1].color_space(), TextureColorSpace::Linear);
    assert_eq!(table.textures()[2].color_space(), TextureColorSpace::Linear);
}

#[test]
fn forged_descriptors_are_rejected() {
    let texels = |edit: fn(&mut SceneTexelDescriptor)| {
        let mut descriptor = texel_descriptor();
        edit(&mut descriptor);
        SceneTextureDescriptor::Texels(descriptor)
    };
    let dds = |edit: fn(&mut SceneDdsDescriptor)| {
        let mut descriptor = dds_descriptor();
        edit(&mut descriptor);
        SceneTextureDescriptor::Dds(descriptor)
    };
    let cube = |edit: fn(&mut SceneCubeDescriptor)| {
        let mut descriptor = cube_descriptor();
        edit(&mut descriptor);
        SceneTextureDescriptor::SpecularCube(descriptor)
    };
    let forged = [
        texels(|d| d.width = 0),
        texels(|d| d.mips = 3),
        texels(|d| d.mips = 0),
        texels(|d| d.logical_bytes = 19),
        dds(|d| d.height = 0),
        dds(|d| d.mips = 5),
        dds(|d| d.mips = 0),
        dds(|d| d.logical_bytes = 259 - 148),
        dds(|d| d.logical_bytes = 0),
        dds(|d| {
            d.format = SceneDdsFormat::Bc5Rg;
            d.color_space = TextureColorSpace::Srgb;
        }),
        cube(|d| d.size = 3),
        cube(|d| d.size = 2048),
        cube(|d| d.mips = 4),
        cube(|d| d.mips = 2),
        cube(|d| d.logical_bytes = 287),
        cube(|d| d.size = 1),
        cube(|d| {
            d.preparation = Some(SceneSpecularPreparation {
                convention: crate::SceneSpecularConvention::default(),
                cube_size: 3,
                mip_count: 1,
                samples: 1,
                source_lod_tail: 1,
            });
        }),
    ];
    for descriptor in forged {
        let result = SceneResources::new(vec![released(descriptor)], Vec::new());
        assert!(result.is_err(), "{descriptor:?} must not validate");
    }
}

#[test]
fn a_released_cube_is_not_a_two_dimensional_material_texture() {
    let result = SceneResources::new(
        vec![released(SceneTextureDescriptor::SpecularCube(
            cube_descriptor(),
        ))],
        vec![material(0)],
    );
    assert!(matches!(
        result,
        Err(SceneValidationError::UnsupportedInput {
            part: "material albedo"
        })
    ));
}

#[test]
fn material_handles_are_still_checked_against_a_released_table() {
    let result = SceneResources::new(
        vec![released(SceneTextureDescriptor::Texels(texel_descriptor()))],
        vec![material(1)],
    );
    assert!(matches!(
        result,
        Err(SceneValidationError::HandleOutOfRange { handle: 1, .. })
    ));
}

#[test]
fn released_threshold_grids_follow_the_source_threshold_rule() {
    let coverage = SceneFadeCoverage::Threshold {
        texture: TextureHandle::from_raw(1),
        jitter_pixels: [0, 0],
    };
    let grid = |format, color_space, width, height, mips, logical_bytes| {
        released(SceneTextureDescriptor::Texels(SceneTexelDescriptor {
            format,
            color_space,
            width,
            height,
            mips,
            logical_bytes,
        }))
    };
    let table = |threshold: SceneTexture| {
        SceneResources::new(
            vec![
                released(SceneTextureDescriptor::Texels(texel_descriptor())),
                threshold,
            ],
            vec![material(0).with_fade_coverage(coverage)],
        )
    };
    assert!(
        table(grid(
            TexelFormat::R8Unorm,
            TextureColorSpace::Linear,
            2,
            2,
            1,
            4
        ))
        .is_ok()
    );
    assert!(
        table(grid(
            TexelFormat::R8Unorm,
            TextureColorSpace::Srgb,
            2,
            2,
            1,
            4
        ))
        .is_err()
    );
    assert!(
        table(grid(
            TexelFormat::Rg8Unorm,
            TextureColorSpace::Linear,
            2,
            2,
            1,
            8
        ))
        .is_err()
    );
    assert!(
        table(grid(
            TexelFormat::R8Unorm,
            TextureColorSpace::Linear,
            2,
            1,
            1,
            2
        ))
        .is_err()
    );
}

#[test]
fn full_and_released_entries_account_for_logical_and_retained_bytes_separately() {
    let full = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 2,
        bytes: vec![0; 20].into(),
    };
    let table = SceneResources::new(
        vec![
            full.clone(),
            full.clone().with_sampler(TextureSampler::default()),
            released(SceneTextureDescriptor::Dds(dds_descriptor())),
        ],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(table.logical_texture_bytes(), 20 + 20 + 260);
    assert_eq!(table.retained_texture_source_bytes(), 40);
}

#[test]
fn replacing_textures_keeps_materials_and_revalidates_every_reference() {
    let base = SceneResources::new(
        vec![released(SceneTextureDescriptor::Texels(texel_descriptor()))],
        vec![material(0)],
    )
    .unwrap();
    let replaced = base
        .with_textures(vec![released(
            SceneTextureDescriptor::Dds(dds_descriptor()),
        )])
        .unwrap();
    assert_eq!(replaced.materials(), base.materials());
    assert_eq!(replaced.textures().len(), 1);
    assert!(base.with_textures(Vec::new()).is_err());
    assert_eq!(
        base.textures().len(),
        1,
        "failure leaves the receiver unchanged"
    );
}

fn dds_released(format: SceneDdsFormat, color_space: TextureColorSpace) -> SceneTexture {
    released(SceneTextureDescriptor::Dds(SceneDdsDescriptor {
        format,
        color_space,
        width: 4,
        height: 4,
        mips: 1,
        logical_bytes: 148 + format.block_bytes(),
    }))
}

#[test]
fn released_scalar_maps_must_store_the_selected_channels() {
    let with_map = |map: SceneTexture| {
        let material = SceneMaterial::new(SceneMaterialParts {
            roughness_metallic: Some(TextureHandle::from_raw(1)),
            ..material_parts(0)
        })
        .unwrap();
        SceneResources::new(
            vec![
                released(SceneTextureDescriptor::Texels(texel_descriptor())),
                map,
            ],
            vec![material],
        )
    };
    assert!(
        with_map(dds_released(
            SceneDdsFormat::Bc7Rgba,
            TextureColorSpace::Linear
        ))
        .is_ok()
    );
    assert!(matches!(
        with_map(dds_released(
            SceneDdsFormat::Bc4R,
            TextureColorSpace::Linear
        )),
        Err(SceneValidationError::MissingTextureChannel {
            part: "material roughness",
            ..
        })
    ));
    assert!(matches!(
        with_map(dds_released(
            SceneDdsFormat::Bc5Rg,
            TextureColorSpace::Linear
        )),
        Err(SceneValidationError::MissingTextureChannel {
            part: "material metalness",
            ..
        })
    ));
}

#[test]
fn released_textures_serve_the_environment_cube_and_lut() {
    let lut = |format, mips, logical_bytes| {
        released(SceneTextureDescriptor::Texels(SceneTexelDescriptor {
            format,
            color_space: TextureColorSpace::Linear,
            width: 2,
            height: 2,
            mips,
            logical_bytes,
        }))
    };
    let environment = crate::SceneEnvironment::new(
        TextureHandle::from_raw(0),
        TextureHandle::from_raw(1),
        crate::SceneSpecularConvention::default(),
        crate::SceneEnvironmentConsumers::Reflections,
    );
    let validate = |lut: SceneTexture| {
        let cube = released(SceneTextureDescriptor::SpecularCube(cube_descriptor()));
        let resources = SceneResources::new(vec![cube, lut], Vec::new()).unwrap();
        environment.validate(&resources, crate::SceneSpecularConvention::default())
    };
    assert!(validate(lut(TexelFormat::Rg16Float, 1, 16)).is_ok());
    assert!(validate(lut(TexelFormat::Rgba8Unorm, 1, 16)).is_err());
    assert!(validate(lut(TexelFormat::Rg16Float, 2, 20)).is_err());
    let not_a_cube = SceneResources::new(
        vec![
            released(SceneTextureDescriptor::Texels(texel_descriptor())),
            lut(TexelFormat::Rg16Float, 1, 16),
        ],
        Vec::new(),
    )
    .unwrap();
    assert!(
        environment
            .validate(&not_a_cube, crate::SceneSpecularConvention::default())
            .is_err()
    );
}

#[test]
fn released_textures_report_terrain_channel_counts_like_their_sources() {
    use crate::scene::terrain_channels::channel_count;
    let released_texels = |format| {
        released(SceneTextureDescriptor::Texels(SceneTexelDescriptor {
            format,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            logical_bytes: format.bytes_per_texel(),
        }))
    };
    assert_eq!(channel_count(&released_texels(TexelFormat::R8Unorm)), Ok(1));
    assert_eq!(
        channel_count(&released_texels(TexelFormat::Rg8Unorm)),
        Ok(2)
    );
    assert_eq!(
        channel_count(&released_texels(TexelFormat::Bgra8Unorm)),
        Ok(4)
    );
    assert!(channel_count(&released_texels(TexelFormat::Rg16Float)).is_err());
    let bc5 = dds_released(SceneDdsFormat::Bc5Rg, TextureColorSpace::Linear);
    assert_eq!(channel_count(&bc5), Ok(2));
    let bc4 = dds_released(SceneDdsFormat::Bc4R, TextureColorSpace::Linear);
    assert_eq!(channel_count(&bc4), Ok(1));
    let cube = released(SceneTextureDescriptor::SpecularCube(cube_descriptor()));
    assert!(channel_count(&cube).is_err());
}

fn dx10_container(dxgi_code: u32) -> Vec<u8> {
    let mut bytes = vec![0; 148 + 16];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[84..88].copy_from_slice(b"DX10");
    for (offset, value) in [
        (4, 124_u32),
        (12, 4),
        (16, 4),
        (28, 1),
        (76, 32),
        (80, 4),
        (128, dxgi_code),
        (132, 3),
        (140, 1),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn dds_source(dxgi_code: u32, color_space: TextureColorSpace) -> SceneTexture {
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space,
        bytes: dx10_container(dxgi_code).into(),
    }
}

#[test]
fn terrain_layers_reject_bc7_as_a_source_and_as_a_released_descriptor() {
    use crate::scene::terrain_channels::channel_count;
    assert_eq!(
        channel_count(&dds_source(83, TextureColorSpace::Linear)),
        Ok(2),
        "the container fixture is admitted when its format is a supported terrain format"
    );
    for (code, color_space) in [
        (98, TextureColorSpace::Linear),
        (99, TextureColorSpace::Srgb),
    ] {
        assert!(
            channel_count(&dds_source(code, color_space)).is_err(),
            "source BC7 code {code} must not be a terrain layer texture"
        );
        let released = dds_released(SceneDdsFormat::Bc7Rgba, color_space);
        assert!(
            channel_count(&released).is_err(),
            "released BC7 {color_space:?} must not be a terrain layer texture"
        );
    }
}

fn dds_with_logical_bytes(logical_bytes: u64) -> SceneTextureDescriptor {
    SceneTextureDescriptor::Dds(SceneDdsDescriptor {
        logical_bytes,
        ..dds_descriptor()
    })
}

fn single_release(
    descriptor: SceneTextureDescriptor,
) -> Result<SceneResources, SceneValidationError> {
    SceneResources::new(vec![released(descriptor)], Vec::new())
}

#[test]
fn released_dds_descriptors_count_the_header_in_their_extent() {
    const HEADER: u64 = 148;
    const CHAIN: u64 = 112;
    assert!(single_release(dds_with_logical_bytes(HEADER + CHAIN)).is_ok());
    for logical in [CHAIN, HEADER, HEADER + CHAIN - 1] {
        assert!(
            single_release(dds_with_logical_bytes(logical)).is_err(),
            "{logical} bytes cannot hold the 148-byte header and the 112-byte mip chain"
        );
    }
}

#[test]
fn released_dds_descriptors_preserve_addressable_trailing_bytes_only() {
    const CANONICAL: u64 = 148 + 112;
    for logical in [CANONICAL + 1, CANONICAL + 8] {
        assert!(single_release(dds_with_logical_bytes(logical)).is_ok());
    }
    assert!(single_release(dds_with_logical_bytes(u64::MAX)).is_err());
}

#[test]
fn released_dds_material_channels_preserve_trailing_byte_admission() {
    let validate = |map| {
        let material = SceneMaterial::new(SceneMaterialParts {
            roughness_metallic: Some(TextureHandle::from_raw(1)),
            ..material_parts(0)
        })
        .unwrap();
        SceneResources::new(
            vec![
                released(SceneTextureDescriptor::Texels(texel_descriptor())),
                map,
            ],
            vec![material],
        )
    };
    let mut source = dx10_container(98);
    source.extend([0; 8]);
    let encoded = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: source.into(),
    };
    let descriptor = SceneTextureDescriptor::Dds(SceneDdsDescriptor {
        format: SceneDdsFormat::Bc7Rgba,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 4,
        mips: 1,
        logical_bytes: 148 + 16 + 8,
    });
    for map in [encoded, released(descriptor)] {
        assert!(validate(map).is_ok());
    }
}

fn released_texels(format: TexelFormat, color_space: TextureColorSpace) -> SceneTextureDescriptor {
    SceneTextureDescriptor::Texels(SceneTexelDescriptor {
        format,
        color_space,
        width: 1,
        height: 1,
        mips: 1,
        logical_bytes: format.bytes_per_texel(),
    })
}

#[test]
fn released_texels_name_only_color_spaces_the_gpu_format_can_store() {
    for format in [
        TexelFormat::R8Unorm,
        TexelFormat::Rg8Unorm,
        TexelFormat::Rg16Float,
        TexelFormat::Rgba16Float,
    ] {
        assert!(
            single_release(released_texels(format, TextureColorSpace::Linear)).is_ok(),
            "{format:?} linear is a GPU format"
        );
        assert!(
            single_release(released_texels(format, TextureColorSpace::Srgb)).is_err(),
            "{format:?} has no sRGB GPU format"
        );
    }
    for format in [TexelFormat::Rgba8Unorm, TexelFormat::Bgra8Unorm] {
        for color_space in [TextureColorSpace::Linear, TextureColorSpace::Srgb] {
            assert!(
                single_release(released_texels(format, color_space)).is_ok(),
                "{format:?} {color_space:?} is a GPU format"
            );
        }
    }
}

/// A grid of 2^29 by 2^29 half-float RGBA texels with a full mip chain, described only.
fn largest_exact_texels() -> SceneTexture {
    const EDGE: u32 = 1 << 29;
    const MIPS: u32 = 30;
    let logical_bytes =
        texel_bytes(TexelFormat::Rgba16Float, EDGE, EDGE, MIPS, "fixture chain").unwrap();
    assert_eq!(logical_bytes, 3_074_457_345_618_258_600);
    released(SceneTextureDescriptor::Texels(SceneTexelDescriptor {
        format: TexelFormat::Rgba16Float,
        color_space: TextureColorSpace::Linear,
        width: EDGE,
        height: EDGE,
        mips: MIPS,
        logical_bytes,
    }))
}

#[test]
fn a_texture_table_whose_logical_bytes_overflow_u64_is_rejected() {
    let fits = SceneResources::new(vec![largest_exact_texels(); 6], Vec::new())
        .expect("six chains sum to u64::MAX - 15");
    assert_eq!(fits.logical_texture_bytes(), u64::MAX - 15);
    let overflow = SceneResources::new(vec![largest_exact_texels(); 7], Vec::new());
    assert!(
        matches!(
            overflow,
            Err(SceneValidationError::ArithmeticOverflow { .. })
        ),
        "seven chains exceed u64::MAX: {overflow:?}"
    );
}

#[test]
fn an_overflowing_aggregate_leaves_the_receiving_table_unchanged() {
    let base = SceneResources::new(vec![largest_exact_texels()], Vec::new()).unwrap();
    let before = base.clone();

    assert!(
        base.with_textures(vec![largest_exact_texels(); 7]).is_err(),
        "replacement tables are checked as a whole"
    );
    assert_eq!(base, before);

    let mut appended = base.clone();
    assert!(
        appended
            .append(vec![largest_exact_texels(); 6], Vec::new())
            .is_err(),
        "appending the seventh chain overflows the aggregate"
    );
    assert_eq!(appended, before);
    assert_eq!(
        appended.logical_texture_bytes(),
        before.logical_texture_bytes()
    );
}

#[test]
fn replacing_one_texture_rejects_overflow_before_copying_the_table() {
    let mut textures = vec![largest_exact_texels(); 6];
    textures.push(released(released_texels(
        TexelFormat::R8Unorm,
        TextureColorSpace::Linear,
    )));
    let mut resources = SceneResources::new(textures, Vec::new()).unwrap();
    let before = resources.clone();
    let table = resources.textures().as_ptr();
    assert!(
        resources
            .replace_texture(6, largest_exact_texels())
            .is_err()
    );
    assert_eq!(resources, before);
    assert_eq!(resources.textures().as_ptr(), table);
    assert_eq!(resources.logical_texture_bytes(), u64::MAX - 14);
}

#[test]
fn successful_replacement_and_shared_materials_preserve_exact_totals() {
    let texture = |format| released(released_texels(format, TextureColorSpace::Linear));
    let original = SceneResources::new(
        vec![
            texture(TexelFormat::Rgba8Unorm),
            texture(TexelFormat::R8Unorm),
        ],
        Vec::new(),
    )
    .unwrap();
    let mut resources = original.clone().with_materials(Vec::new()).unwrap();
    assert_eq!(resources.logical_texture_bytes(), 5);
    assert_eq!(resources.textures().as_ptr(), original.textures().as_ptr());
    resources
        .replace_texture(1, texture(TexelFormat::Rgba16Float))
        .unwrap();
    assert_eq!(resources.logical_texture_bytes(), 12);
    assert_eq!(resources.retained_texture_source_bytes(), 0);
    assert_eq!(original.logical_texture_bytes(), 5);
    assert_eq!(
        resources.logical_texture_bytes(),
        resources
            .textures()
            .iter()
            .map(SceneTexture::logical_source_bytes)
            .sum::<u64>()
    );
}

#[test]
fn failed_material_validation_restores_the_replacement_and_its_total() {
    let texture = |format| released(released_texels(format, TextureColorSpace::Linear));
    let material = SceneMaterial::new(SceneMaterialParts {
        roughness_metallic: Some(TextureHandle::from_raw(1)),
        ..material_parts(0)
    })
    .unwrap();
    let mut resources =
        SceneResources::new(vec![texture(TexelFormat::Rgba8Unorm); 2], vec![material]).unwrap();
    let before = resources.clone();
    assert!(
        resources
            .replace_texture(1, texture(TexelFormat::R8Unorm))
            .is_err()
    );
    assert_eq!(resources, before);
    assert_eq!(resources.logical_texture_bytes(), 8);
}
