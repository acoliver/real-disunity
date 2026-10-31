use super::*;
use crate::sky::preparation::{self, EnvironmentPreparation, PreparationError};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{SceneSpecularCube, SceneSpecularPreparation};

fn request() -> SceneSpecularPreparation {
    SceneSpecularPreparation {
        convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        cube_size: 4,
        mip_count: 3,
        samples: 8,
        source_lod_tail: 1,
    }
}
fn cube(compressed: bool) -> SceneSpecularCube {
    if compressed {
        // BC6H unsigned mode 11: one subset, equal 10-bit endpoints, zero indices.
        let mut bits = 3u128;
        for endpoint in 0..6 {
            bits |= 512u128 << (5 + endpoint * 10);
        }
        SceneSpecularCube::new(4, bits.to_le_bytes().repeat(18).into()).unwrap()
    } else {
        let mut bytes = Vec::new();
        for face in 0u16..6 {
            for mip in 0u16..3 {
                let texel = [0x3c00 + face * 128 + mip * 32, 0x3800, 0x3400, 0x3c00];
                bytes.extend(
                    texel
                        .repeat((4usize >> mip).pow(2))
                        .into_iter()
                        .flat_map(u16::to_le_bytes),
                );
            }
        }
        SceneSpecularCube::rgba16_float(4, 3, bytes.into()).unwrap()
    }
}
fn source(cube: SceneSpecularCube) -> RenderScene {
    let base = scene(SceneAlphaMode::Opaque, false, 1);
    let mut textures = base.resources().textures().to_vec();
    textures.push(SceneTexture::SpecularCube(cube));
    let resources = SceneResources::new(textures, base.resources().materials().to_vec()).unwrap();
    base.with_resources(resources, base.geometry().clone(), base.effects().cloned())
        .unwrap()
}
#[test]
fn environment_source_scene_matches_direct_preparation_for_float_and_bc6h() {
    let context = context();
    for compressed in [false, true] {
        let cube = cube(compressed);
        let source_texture = crate::upload::upload_texture(
            context.device(),
            context.queue(),
            &crate::texture::prepare_texture(&SceneTexture::SpecularCube(cube.clone())).unwrap(),
        )
        .unwrap();
        let direct = direct(&context, &source_texture);
        let scene = source(cube.with_preparation(request()).unwrap());
        let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
        let resident = gpu.retained.textures.last().unwrap();
        assert_eq!(resident.format(), wgpu::TextureFormat::Rgba16Float);
        for mip in 0..3 {
            let actual = read(&context, resident, mip);
            let expected = read(&context, &direct, mip);
            for (actual, expected) in actual.chunks_exact(2).zip(expected.chunks_exact(2)) {
                let a = half::f16::from_le_bytes(actual.try_into().unwrap()).to_f32();
                let b = half::f16::from_le_bytes(expected.try_into().unwrap()).to_f32();
                assert!((a - b).abs() <= 0.0005);
            }
            if compressed {
                for texel in actual.chunks_exact(8) {
                    let red = half::f16::from_le_bytes([texel[0], texel[1]]).to_f32();
                    assert!(red > 0.1);
                    assert_eq!(&texel[..2], &texel[2..4]);
                    assert_eq!(&texel[..2], &texel[4..6]);
                    assert_eq!(&texel[6..], &[0, 0x3c]);
                }
            }
        }
    }
}
fn direct(context: &crate::device::GpuContext, source: &wgpu::Texture) -> wgpu::Texture {
    let view = source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut maps = None;
    let frame = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            maps = Some(preparation::encode(
                context.device(),
                encoder,
                &view,
                EnvironmentPreparation::from(request()),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    assert!(maps.as_ref().unwrap().views().is_err());
    frame.cancel();
    assert!(maps.take().unwrap().views().is_err());
    let frame = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            maps = Some(preparation::encode(
                context.device(),
                encoder,
                &view,
                EnvironmentPreparation::from(request()),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    frame.submit().unwrap();
    maps.unwrap().views().unwrap().0.texture().clone()
}
#[test]
fn environment_prefiltered_default_is_byte_identical_and_has_no_job_inventory() {
    let context = context();
    let cube = cube(false);
    let scene = source(cube.clone());
    assert_eq!(
        super::super::source_environment::allocations(&scene)
            .unwrap()
            .validate(&context.device().limits())
            .unwrap(),
        0
    );
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    for mip in 0..3 {
        let bytes = read(&context, gpu.retained.textures.last().unwrap(), mip);
        let expected = (0..6)
            .flat_map(|face| cube.subresource(face, mip).unwrap().iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected);
    }
    let requested = source(cube.with_preparation(request()).unwrap());
    let plan = super::super::source_environment::allocations(&requested).unwrap();
    let generated = EnvironmentPreparation::from(request())
        .allocations()
        .unwrap();
    let limits = context.device().limits();
    // 1008 source bytes plus three levels of aligned staging on six faces,
    // counted once on the GPU and once on the CPU.
    assert_eq!(
        plan.validate(&limits).unwrap() - generated.validate(&limits).unwrap(),
        1008 + 2 * 6 * 256 * 7
    );
}
fn read(context: &crate::device::GpuContext, texture: &wgpu::Texture, mip: u32) -> Vec<u8> {
    let side = texture.width() >> mip;
    let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("cube regression readback"),
        size: u64::from(256 * side * 6),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let frame = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(side),
                    },
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 6,
                },
            );
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    frame.submit().unwrap();
    map_read(context, &buffer, side)
}
fn map_read(context: &crate::device::GpuContext, buffer: &wgpu::Buffer, side: u32) -> Vec<u8> {
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = mapped
        .chunks_exact(256)
        .flat_map(|row| row[..side as usize * 8].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}
