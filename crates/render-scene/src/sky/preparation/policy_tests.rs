use super::*;
use std::num::NonZeroU64;

#[test]
fn filter_sample_count_and_source_tail_match_constant_mip_oracle() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let source = mip_distinct_source(&context);
    write_graded_mips(&context, &source);
    // Uncapped footprints use integer LODs, avoiding implementation-defined sampler
    // interpolation precision. The 128-sample recipe exercises both source caps.
    for (samples, tail) in [(24, 1), (96, 1), (128, 4), (128, 7)] {
        let config = EnvironmentPreparation {
            samples,
            source_lod_tail: tail,
            ..settings()
        };
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let maps = encode(
            context.device(),
            &mut encoder,
            &source,
            config,
            AllocationBudget(1_000_000),
        )
        .unwrap();
        encoder.finish().submit().unwrap();
        let bytes = read_texture(context.device(), context.submission(), &maps.cube, 2);
        let actual = half(u16::from_le_bytes([bytes[0], bytes[1]]));
        // At roughness one, D=1/pi and the reflection PDF is 1/(4*pi),
        // independent of the Hammersley direction. Every footprint has this LOD.
        let lod = (0.5 * (6.0 * 64.0 * 64.0 / f64::from(samples)).log2()).min(f64::from(7 - tail));
        let low = 2.0_f64.powf(lod.floor());
        let expected = low * (1.0 + lod.fract());
        // Linear source mip filtering followed by binary16 output rounding.
        assert!(
            (f64::from(actual) - expected).abs() < expected * 0.001,
            "samples={samples} tail={tail}: {actual} != {expected}, lod={lod}"
        );
    }
    assert!(
        EnvironmentPreparation {
            source_lod_tail: 33,
            ..settings()
        }
        .allocations()
        .is_err()
    );
}

#[test]
fn authored_table_keeps_identity_and_view_cosine_roughness_axes() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    for format in [
        wgpu::TextureFormat::Rg16Float,
        wgpu::TextureFormat::Rgba16Float,
    ] {
        let table = authored_table_format(&context, format);
        assert_eq!(table.identity().get(), 180);
        assert_eq!(table.axes(), BrdfTableAxes::ViewCosineRoughness);
        let source = source_cube(&context);
        for intensity in [0, 500_000, 1_000_000, 2_000_000] {
            let mut encoder = context.submission().begin(context.device()).unwrap();
            let maps = encode_with_authored_brdf(
                context.device(),
                &mut encoder,
                &source,
                EnvironmentPreparation {
                    specular_intensity_micro: intensity,
                    ..settings()
                },
                table.clone(),
                AllocationBudget(1_000_000),
            )
            .unwrap();
            assert!(maps.views().is_err());
            encoder.finish().submit().unwrap();
            assert_eq!(maps.authored_brdf().unwrap().identity(), table.identity());
            if intensity == 1_000_000 {
                assert_eq!(maps.views().unwrap().1, table.view);
            }
            let actual = sample_table(&context, &maps.views().unwrap().1);
            let expected = (0.625 * 0.4 + 0.25) * 2.0 * f64::from(intensity) / 1e6;
            assert!(
                (f64::from(actual) - expected).abs() < 1e-6,
                "intensity={intensity}: {actual} vs {expected}"
            );
        }
    }
}

fn sample_table(context: &crate::device::GpuContext, view: &wgpu::TextureView) -> f32 {
    let device = context.device();
    let pipeline = table_pipeline(device);
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = context.submission().begin(device).unwrap();
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 4);
    encoder.finish().submit().unwrap();
    read_value(device, &readback)
}

fn table_pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            r"
@group(0) @binding(0) var table:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
@group(0) @binding(2) var<storage,read_write> result:array<f32>;
@compute @workgroup_size(1) fn evaluate() {
 let ab=textureSampleLevel(table,linear_sampler,vec2(0.25,0.75),0.0).rg;
 result[0]=(0.4*ab.x+ab.y)*2.0;
}"
            .into(),
        ),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("evaluate"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn read_value(device: &wgpu::Device, readback: &wgpu::Buffer) -> f32 {
    let (send, recv) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    recv.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    f32::from_le_bytes(bytes[..4].try_into().unwrap())
}

fn write_graded_mips(context: &crate::device::GpuContext, source: &wgpu::TextureView) {
    for mip in 0..7 {
        let size = 64 >> mip;
        let value = half::f16::from_f64(2.0_f64.powf(f64::from(mip))).to_le_bytes();
        let texel = [value, value, value, half::f16::ONE.to_le_bytes()].concat();
        context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source.texture(),
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &texel.repeat(usize::try_from(size * size * 6).unwrap()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 8),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
        );
    }
}

fn authored_table_format(
    context: &crate::device::GpuContext,
    format: wgpu::TextureFormat,
) -> AuthoredBrdfTable {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("authored BRDF axes"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let bytes = [0.125_f32, 0.375, 0.625, 0.875]
        .into_iter()
        .flat_map(|a| {
            let channels = if format == wgpu::TextureFormat::Rg16Float {
                2
            } else {
                4
            };
            [a, 0.25, 0.0, 1.0]
                .into_iter()
                .take(channels)
                .flat_map(|v| half::f16::from_f32(v).to_le_bytes())
        })
        .collect::<Vec<_>>();
    context.queue().write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(if format == wgpu::TextureFormat::Rg16Float {
                8
            } else {
                16
            }),
            rows_per_image: Some(2),
        },
        texture.size(),
    );
    let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        array_layer_count: Some(1),
        ..Default::default()
    });
    AuthoredBrdfTable::new(
        NonZeroU64::new(180).unwrap(),
        array_view.texture(),
        settings().convention,
        BrdfTableAxes::ViewCosineRoughness,
    )
    .unwrap()
}
