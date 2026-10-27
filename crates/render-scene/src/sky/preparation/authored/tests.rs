use super::*;
use crate::device::GpuContext;

fn settings() -> EnvironmentPreparation {
    EnvironmentPreparation {
        convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        cube_size: 8,
        mip_count: 4,
        specular_intensity_micro: 1_000_000,
        lut_size: 4,
        samples: 32,
    }
}
fn cube() -> SceneSpecularCube {
    let mut bytes = vec![];
    for face in 0_u8..6 {
        for (mip, count) in [(0_u8, 64), (1, 16), (2, 16), (3, 16)] {
            bytes.extend(vec![face * 16 + mip; count]);
        }
    }
    SceneSpecularCube::new(8, bytes.into_boxed_slice()).unwrap()
}
#[test]
fn authored_cube_budget_and_publication_fail_before_binding() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let device = context.device();
    let cube = cube();
    let required = allocations(&cube, settings())
        .unwrap()
        .validate(&device.limits())
        .unwrap();
    let mut maps = None;
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            assert!(matches!(
                encode(
                    device,
                    encoder,
                    &cube,
                    settings(),
                    AllocationBudget(required - 1)
                ),
                Err(PreparationError::Budget)
            ));
            maps = Some(encode(
                device,
                encoder,
                &cube,
                settings(),
                AllocationBudget(required),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    assert!(maps.as_ref().unwrap().views().is_err());
    prepared.cancel();
    assert!(maps.unwrap().views().is_err());
    let mismatch = EnvironmentPreparation {
        mip_count: 3,
        ..settings()
    };
    assert!(allocations(&cube, mismatch).is_err());
}
#[test]
fn gpu_authored_cube_retains_every_face_and_mip_byte() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let device = context.device();
    let cube = cube();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 24 * 512,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut maps = None;
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            let environment = encode(
                device,
                encoder,
                &cube,
                settings(),
                AllocationBudget(1_000_000),
            )?;
            for face in 0..6 {
                for mip in 0..4 {
                    let side = (8 >> mip).max(4);
                    encoder.copy_texture_to_buffer(
                        wgpu::TexelCopyTextureInfo {
                            texture: &environment.cube,
                            mip_level: mip,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: face,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyBufferInfo {
                            buffer: &buffer,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: u64::from(face * 4 + mip) * 512,
                                bytes_per_row: Some(256),
                                rows_per_image: None,
                            },
                        },
                        wgpu::Extent3d {
                            width: side,
                            height: side,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
            maps = Some(environment);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    assert!(maps.unwrap().views().is_ok());
    let data = crate::sky::scene_tests::read(device, &buffer);
    assert_subresources(&data);
}
fn assert_subresources(data: &[u8]) {
    for face in 0_u8..6 {
        for mip in 0_u8..4 {
            let side = (8_u8 >> mip).max(4);
            for row in 0..side / 4 {
                let offset = usize::from(face * 4 + mip) * 512 + usize::from(row) * 256;
                assert_eq!(
                    &data[offset..offset + usize::from(side / 4) * 16],
                    vec![face * 16 + mip; usize::from(side / 4) * 16]
                );
            }
        }
    }
}
