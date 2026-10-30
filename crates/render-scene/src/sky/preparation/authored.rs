//! Authored reflection cubes use the existing environment installation path.
use super::{EnvironmentPreparation, Integration, PreparationError, PreparedEnvironmentMaps};
use wgpu::util::DeviceExt;
use worldrunner_render_gpu::{
    configuration::{AllocationBudget, AllocationSet},
    submission::Publications,
};
use worldrunner_render_plan::SceneSpecularCube;

/// Filtering sample count and source LOD policy do not apply to an authored cube.
/// Its prefiltered mip chain is uploaded byte-for-byte; samples still govern generated BRDF integration.
pub fn allocations(
    cube: &SceneSpecularCube,
    settings: EnvironmentPreparation,
) -> Result<AllocationSet, PreparationError> {
    settings.allocations()?;
    if cube.format() != worldrunner_render_plan::SceneSpecularCubeFormat::Bc6hRgbUfloat {
        return Err(PreparationError::Source);
    }
    if cube.size() != settings.cube_size || cube.mip_count() != settings.mip_count {
        return Err(PreparationError::Settings);
    }
    let upload_bytes: u64 = (0..cube.mip_count())
        .map(|mip| {
            let blocks = (cube.size() >> mip).div_ceil(4);
            u64::from((blocks * 16).div_ceil(256) * 256) * u64::from(blocks) * 6
        })
        .sum();
    let mut set = AllocationSet::default();
    set.texture(
        "authored reflections",
        [cube.size(), cube.size(), 6],
        wgpu::TextureFormat::Bc6hRgbUfloat,
        [cube.mip_count(), 1],
        1,
    );
    set.texture(
        "integrated BRDF",
        [settings.lut_size, settings.lut_size, 1],
        wgpu::TextureFormat::Rgba16Float,
        [1, 1],
        1,
    );
    set.buffer("authored reflection staging", upload_bytes, 1);
    set.cpu("authored reflection staging", upload_bytes);
    set.buffer("environment job uniforms", 32, 1);
    Ok(set)
}

pub fn encode(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    cube: &SceneSpecularCube,
    settings: EnvironmentPreparation,
    budget: AllocationBudget,
) -> Result<PreparedEnvironmentMaps, PreparationError> {
    let transaction = Publications::for_encoder(encoder)?;
    let required = allocations(cube, settings)?
        .validate(&device.limits())
        .map_err(|_| PreparationError::Budget)?;
    if required > budget.0 {
        return Err(PreparationError::Budget);
    }
    if !device
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    {
        return Err(PreparationError::Source);
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("authored reflections"),
        size: wgpu::Extent3d {
            width: cube.size(),
            height: cube.size(),
            depth_or_array_layers: 6,
        },
        mip_level_count: cube.mip_count(),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bc6hRgbUfloat,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    upload(device, encoder, &texture, cube);
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let lut = super::texture(device, settings.lut_size, 1, 1);
    Integration::new(device, &view, settings.specular_intensity_micro).dispatch(
        encoder,
        &lut,
        settings,
        settings.mip_count,
    );
    Ok(PreparedEnvironmentMaps {
        cube: texture,
        lut,
        transaction: Some(transaction),
        convention: settings.convention,
        authored_brdf: None,
        baked_specular_intensity_micro: settings.specular_intensity_micro,
        specular_intensity_micro: 1_000_000,
    })
}

/// Install authored reflection mips and authored BRDF coefficients in the same transaction.
pub fn encode_with_authored_brdf(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    cube: &SceneSpecularCube,
    settings: EnvironmentPreparation,
    table: super::AuthoredBrdfTable,
    budget: AllocationBudget,
) -> Result<PreparedEnvironmentMaps, PreparationError> {
    let settings = EnvironmentPreparation {
        lut_size: table.view.texture().width(),
        ..settings
    };
    let maps = encode(device, encoder, cube, settings, budget)?;
    super::authored_brdf::install(device, encoder, maps, table)
}

fn upload(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    cube: &SceneSpecularCube,
) {
    let mut bytes = vec![];
    let mut copies = vec![];
    for face in 0..6 {
        for mip in 0..cube.mip_count() {
            let side = (cube.size() >> mip).max(4);
            let row_bytes = side / 4 * 16;
            let stride = row_bytes.div_ceil(256) * 256;
            let offset = u64::try_from(bytes.len()).expect("bounded upload");
            for row in cube
                .subresource(face, mip)
                .expect("bounded cube index")
                .chunks_exact(row_bytes as usize)
            {
                bytes.extend_from_slice(row);
                bytes.resize(bytes.len() + (stride - row_bytes) as usize, 0);
            }
            copies.push((face, mip, side, stride, offset));
        }
    }
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("authored reflection upload"),
        contents: &bytes,
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    for (face, mip, side, stride, offset) in copies {
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: mip,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: face,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests;
