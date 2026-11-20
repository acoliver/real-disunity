use super::{GameResourceError, GameTextureDeclaration, texture_descriptor};

fn declaration(
    format: wgpu::TextureFormat,
    size: [u32; 3],
    dimension: wgpu::TextureViewDimension,
    texels: Option<&[u8]>,
) -> GameTextureDeclaration<'_> {
    GameTextureDeclaration {
        label: "probe",
        format,
        size,
        dimension,
        texels,
    }
}

fn reason(result: Result<wgpu::TextureDescriptor<'_>, GameResourceError>) -> String {
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
    use wgpu::TextureViewDimension as View;
    let rgba = wgpu::TextureFormat::Rgba8Unorm;
    let plain =
        texture_descriptor(&declaration(rgba, [4, 4, 1], View::D2, Some(&[0; 64]))).unwrap();
    assert_eq!(plain.dimension, wgpu::TextureDimension::D2);
    assert_eq!(
        plain.usage,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST
    );
    let volume = texture_descriptor(&declaration(
        wgpu::TextureFormat::Rg8Unorm,
        [1, 1, 1],
        View::D3,
        Some(&[1, 2]),
    ))
    .unwrap();
    assert_eq!(volume.dimension, wgpu::TextureDimension::D3);
    // A zeroed depth array is only sampled.
    let depth = texture_descriptor(&declaration(
        wgpu::TextureFormat::Depth32Float,
        [4, 4, 2],
        View::D2Array,
        None,
    ))
    .unwrap();
    assert_eq!(depth.size.depth_or_array_layers, 2);
    assert_eq!(depth.usage, wgpu::TextureUsages::TEXTURE_BINDING);
    // Block formats count whole blocks.
    let bc = texture_descriptor(&declaration(
        wgpu::TextureFormat::Bc1RgbaUnorm,
        [8, 4, 1],
        View::D2,
        Some(&[0; 16]),
    ))
    .unwrap();
    assert_eq!(bc.size.width, 8);
}

#[test]
fn a_texture_declaration_that_cannot_be_created_is_refused_by_name() {
    use wgpu::TextureViewDimension as View;
    let rgba = wgpu::TextureFormat::Rgba8Unorm;
    assert!(
        reason(texture_descriptor(&declaration(
            rgba,
            [4, 4, 1],
            View::D2,
            Some(&[0; 63])
        )))
        .contains("63 bytes")
    );
    assert!(
        reason(texture_descriptor(&declaration(
            rgba,
            [4, 4, 2],
            View::D2,
            None
        )))
        .contains("D2 (one layer)")
    );
    assert!(
        reason(texture_descriptor(&declaration(
            rgba,
            [4, 4, 6],
            View::Cube,
            None
        )))
        .contains("D2Array or D3")
    );
    assert!(
        reason(texture_descriptor(&declaration(
            rgba,
            [0, 4, 1],
            View::D2,
            None
        )))
        .contains("empty")
    );
    assert!(
        reason(texture_descriptor(&declaration(
            wgpu::TextureFormat::Bc1RgbaUnorm,
            [6, 4, 1],
            View::D2,
            None,
        )))
        .contains("whole blocks")
    );
    assert!(
        reason(texture_descriptor(&declaration(
            wgpu::TextureFormat::Depth32Float,
            [1, 1, 1],
            View::D2,
            Some(&[0; 4]),
        )))
        .contains("without texels")
    );
}
