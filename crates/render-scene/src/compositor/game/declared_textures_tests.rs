use super::{GameTextureDeclaration, GameTextureLevel, beyond_limits, unsampleable};
use crate::compositor::GameResourceError;
use wgpu::TextureFormat as Format;
use wgpu::TextureViewDimension as View;

fn declaration(
    format: Format,
    size: [u32; 3],
    dimension: View,
    texels: Option<&[u8]>,
) -> GameTextureDeclaration<'_> {
    chain(format, size, dimension, 1, texels)
}

fn chain(
    format: Format,
    size: [u32; 3],
    dimension: View,
    mip_levels: u32,
    texels: Option<&[u8]>,
) -> GameTextureDeclaration<'_> {
    GameTextureDeclaration {
        label: "probe",
        format,
        size,
        dimension,
        mip_levels,
        texels,
    }
}

fn reason<T: std::fmt::Debug>(result: Result<T, GameResourceError>) -> String {
    match result {
        Err(GameResourceError::Declaration { label, reason }) => {
            assert_eq!(label, "probe");
            reason
        }
        other => panic!("expected a refused declaration, got {other:?}"),
    }
}

#[test]
fn a_declared_texture_takes_its_dimension_and_usage_from_the_declaration() {
    let rgba = Format::Rgba8Unorm;
    let plain = declaration(rgba, [4, 4, 1], View::D2, Some(&[0; 64]));
    let plain = plain.descriptor().unwrap();
    assert_eq!(plain.dimension, wgpu::TextureDimension::D2);
    assert_eq!(plain.mip_level_count, 1);
    assert_eq!(
        plain.usage,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
    );
    // Without texels, a colour or block-compressed texture is still writable by GPU work and
    // readable back.
    let empty = chain(Format::Bc5RgUnorm, [16, 16, 2], View::D2Array, 4, None);
    assert_eq!(
        empty.descriptor().unwrap().usage,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
    );
    let volume = declaration(Format::Rg8Unorm, [1, 1, 1], View::D3, Some(&[1, 2]));
    assert_eq!(
        volume.descriptor().unwrap().dimension,
        wgpu::TextureDimension::D3
    );
    // A zeroed depth array is only sampled.
    let depth = declaration(Format::Depth32Float, [4, 4, 2], View::D2Array, None);
    let depth = depth.descriptor().unwrap();
    assert_eq!(depth.size.depth_or_array_layers, 2);
    assert_eq!(depth.usage, wgpu::TextureUsages::TEXTURE_BINDING);
    // A zeroed volume may be deeper than one slice.
    let deep = declaration(Format::R8Unorm, [2, 2, 4], View::D3, None);
    assert_eq!(deep.descriptor().unwrap().size.depth_or_array_layers, 4);
    // Block formats count whole blocks.
    let bc = declaration(Format::Bc1RgbaUnorm, [8, 4, 1], View::D2, Some(&[0; 16]));
    assert_eq!(bc.descriptor().unwrap().size.width, 8);
}

#[test]
fn a_block_compressed_mip_chain_is_laid_out_in_whole_blocks_down_to_one_texel() {
    // 8 x 8 BC1: two by two blocks of 8 bytes, then one block for 4 x 4, 2 x 2 and 1 x 1.
    let probe = chain(Format::Bc1RgbaUnorm, [8, 8, 1], View::D2, 4, None);
    let level = |mip, width, bytes_per_row, rows, offset, len| GameTextureLevel {
        layer: 0,
        mip,
        width,
        height: width,
        bytes_per_row,
        rows,
        offset,
        len,
    };
    assert_eq!(
        probe.levels().unwrap(),
        [
            level(0, 8, 16, 2, 0, 32),
            level(1, 4, 8, 1, 32, 8),
            level(2, 2, 8, 1, 40, 8),
            level(3, 1, 8, 1, 48, 8),
        ]
    );
    let texels = [0; 56];
    let whole = chain(Format::Bc1RgbaUnorm, [8, 8, 1], View::D2, 4, Some(&texels));
    let descriptor = whole.descriptor().unwrap();
    assert_eq!(descriptor.mip_level_count, 4);
    assert_eq!(
        worldrunner_render_gpu::census::texture_backing_bytes(&descriptor),
        56,
        "the budget counts every level"
    );
    let short = chain(
        Format::Bc1RgbaUnorm,
        [8, 8, 1],
        View::D2,
        4,
        Some(&texels[1..]),
    );
    assert!(reason(short.descriptor()).contains("55 bytes, the 4 levels 56"));
}

#[test]
fn an_array_lays_out_each_layer_chain_after_the_last() {
    let probe = chain(Format::Rgba8Unorm, [4, 4, 2], View::D2Array, 3, None);
    let levels = probe.levels().unwrap();
    let placed: Vec<_> = levels
        .iter()
        .map(|level| (level.layer, level.mip, level.bytes_per_row, level.offset))
        .collect();
    assert_eq!(
        placed,
        [
            (0, 0, 16, 0),
            (0, 1, 8, 64),
            (0, 2, 4, 80),
            (1, 0, 16, 84),
            (1, 1, 8, 148),
            (1, 2, 4, 164),
        ]
    );
    let texels = [0; 168];
    let whole = chain(
        Format::Rgba8Unorm,
        [4, 4, 2],
        View::D2Array,
        3,
        Some(&texels),
    );
    assert_eq!(whole.descriptor().unwrap().mip_level_count, 3);
}

#[test]
fn a_texture_declaration_that_cannot_be_created_is_refused_by_name() {
    let rgba = Format::Rgba8Unorm;
    let refused = |probe: GameTextureDeclaration<'_>| reason(probe.descriptor());
    assert!(refused(declaration(rgba, [4, 4, 1], View::D2, Some(&[0; 63]))).contains("63 bytes"));
    assert!(refused(declaration(rgba, [4, 4, 2], View::D2, None)).contains("D2 (one layer)"));
    assert!(refused(declaration(rgba, [4, 4, 6], View::Cube, None)).contains("D2Array or D3"));
    assert!(refused(declaration(rgba, [0, 4, 1], View::D2, None)).contains("empty"));
    assert!(
        refused(declaration(Format::Bc1RgbaUnorm, [6, 4, 1], View::D2, None))
            .contains("whole blocks")
    );
    assert!(
        refused(declaration(
            Format::Depth32Float,
            [1, 1, 1],
            View::D2,
            Some(&[0; 4])
        ))
        .contains("without texels")
    );
    assert!(refused(chain(rgba, [4, 4, 1], View::D2, 0, None)).contains("1 to 3"));
    assert!(refused(chain(rgba, [4, 2, 1], View::D2, 4, None)).contains("1 to 3"));
    assert!(
        refused(declaration(
            Format::R8Unorm,
            [2, 2, 2],
            View::D3,
            Some(&[0; 8])
        ))
        .contains("deeper than one slice")
    );
    assert!(
        refused(declaration(Format::Bc4RUnorm, [4, 4, 1], View::D3, None))
            .contains("not block-compressed")
    );
    assert!(
        refused(declaration(Format::Depth32Float, [2, 2, 2], View::D3, None))
            .contains("depth texture is not 3D")
    );
    // The layout itself is refused the same way.
    assert!(
        reason(declaration(Format::Depth32Float, [1, 1, 1], View::D2, None).levels())
            .contains("without texels")
    );
}

#[test]
fn a_texture_past_the_device_limits_is_refused_by_name() {
    let limits = wgpu::Limits::default();
    let past = |size, dimension| {
        let probe = declaration(Format::R8Unorm, size, dimension, None);
        reason(beyond_limits(&limits, &probe.descriptor().unwrap()))
    };
    let wide = limits.max_texture_dimension_2d + 1;
    assert!(past([wide, 1, 1], View::D2).contains("texture limit"));
    let layers = limits.max_texture_array_layers + 1;
    assert!(past([1, 1, layers], View::D2Array).contains("more layers"));
    // A volume's depth is bounded by the 3D limit, not by the array layers.
    let deep = limits.max_texture_dimension_3d;
    assert!(deep > limits.max_texture_array_layers);
    let fits = declaration(Format::R8Unorm, [1, 1, deep], View::D3, None);
    assert!(beyond_limits(&limits, &fits.descriptor().unwrap()).is_ok());
    assert!(past([1, 1, deep + 1], View::D3).contains("texture limit"));
}

#[test]
fn a_format_the_device_cannot_sample_is_named() {
    let bc = Format::Bc5RgUnorm;
    let missing = unsampleable(bc, wgpu::Features::empty()).unwrap();
    assert!(missing.contains("Bc5RgUnorm"), "{missing}");
    assert!(missing.contains("TEXTURE_COMPRESSION_BC"), "{missing}");
    assert_eq!(
        unsampleable(bc, wgpu::Features::TEXTURE_COMPRESSION_BC),
        None
    );
    for format in [
        Format::Rgba8Unorm,
        Format::Rg32Uint,
        Format::R16Uint,
        Format::Depth32Float,
    ] {
        assert_eq!(
            unsampleable(format, wgpu::Features::empty()),
            None,
            "{format:?}"
        );
    }
}

#[test]
fn shared_declarations_agree_on_everything_they_declare() {
    let texels = [7; 16];
    let other = [8; 16];
    let base = declaration(Format::Rgba8Unorm, [2, 2, 1], View::D2, Some(&texels));
    let same_bytes = texels;
    let again = declaration(Format::Rgba8Unorm, [2, 2, 1], View::D2, Some(&same_bytes));
    assert_eq!(base.identity(), again.identity());
    let changed = [
        declaration(Format::Rgba8Unorm, [2, 2, 1], View::D2, Some(&other)),
        declaration(Format::Rgba8Unorm, [2, 2, 1], View::D2, None),
        declaration(Format::Rgba8UnormSrgb, [2, 2, 1], View::D2, Some(&texels)),
        declaration(Format::Rgba8Unorm, [2, 2, 1], View::D2Array, Some(&texels)),
        chain(Format::Rgba8Unorm, [2, 2, 1], View::D2, 2, None),
    ];
    for probe in changed {
        assert_ne!(base.identity(), probe.identity(), "{probe:?}");
    }
}

#[test]
fn a_shared_block_chain_is_counted_once_by_the_census_and_by_each_declaring_factory() {
    use crate::compositor::game::resources::{GameResources, SharedTextures};
    use worldrunner_render_gpu::census::ResourceClass;
    use worldrunner_render_gpu::configuration::AllocationBudget;
    use worldrunner_render_gpu::submission::SubmissionAuthority;

    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..wgpu::DeviceDescriptor::default()
    });
    let authority = SubmissionAuthority::new(queue);
    let shared = SharedTextures::default();
    let factory = |budget| {
        GameResources::new(
            &device,
            &authority,
            AllocationBudget(budget),
            shared.clone(),
        )
    };
    // Two layers of 12 x 20 BC3 down to 1 x 1: 15, 6, 2, 1 and 1 blocks of 16 bytes a layer, the
    // last three levels smaller than a block.
    let probe = chain(Format::Bc3RgbaUnorm, [12, 20, 2], View::D2Array, 5, None);
    let bytes: u64 = probe.levels().unwrap().iter().map(|level| level.len).sum();
    assert_eq!(bytes, 2 * (15 + 6 + 2 + 1 + 1) * 16);
    let texels = vec![5; usize::try_from(bytes).unwrap()];
    let declared = GameTextureDeclaration {
        label: "grid",
        texels: Some(&texels),
        ..probe
    };
    let in_census = || authority.census().snapshot().get(ResourceClass::Texture);
    let before = in_census();
    let (first, second, small) = (factory(1 << 20), factory(1 << 20), factory(bytes - 1));
    let handle = first.declare_shared_texture(&declared).unwrap();
    assert_eq!(
        in_census() - before,
        bytes,
        "the census counts every level of every layer"
    );
    assert_eq!(
        (first.bytes(), first.runtime().live_bytes()),
        (bytes, bytes)
    );
    second.declare_shared_texture(&declared).unwrap();
    assert_eq!(
        in_census() - before,
        bytes,
        "a second declaring factory adds no texture"
    );
    assert_eq!(
        (second.bytes(), second.runtime().live_bytes()),
        (bytes, bytes)
    );
    let refused = small.declare_shared_texture(&declared);
    assert!(
        matches!(refused, Err(GameResourceError::Budget { bytes: asked, .. }) if asked == bytes),
        "{refused:?}"
    );
    assert_eq!(
        (small.bytes(), small.runtime().live_bytes()),
        (0, 0),
        "a refusal holds nothing"
    );
    assert_eq!(first.declare_shared_texture(&declared), Ok(handle));
    assert_eq!(
        first.runtime().live_bytes(),
        bytes,
        "declaring again holds nothing more"
    );
    drop(first);
    assert_eq!(
        in_census() - before,
        bytes,
        "the texture outlives the factory that made it"
    );
    assert_eq!(second.runtime().live_bytes(), bytes);
    drop((second, small, factory));
    drop(shared);
    assert_eq!(in_census(), before, "the last holder releases it once");
}
