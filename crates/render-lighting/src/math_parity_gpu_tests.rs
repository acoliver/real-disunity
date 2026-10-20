#![allow(clippy::float_cmp)]

use super::WGSL;

#[test]
#[ignore = "requires the primary GPU adapter; run explicitly for lighting parity"]
fn canonical_lighting_kernels_match_retired_float32_expressions() {
    let values = evaluate(comparison_source());
    assert_parity(&values);
}

fn comparison_source() -> String {
    format!(
        r"{WGSL}
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
fn retired_visibility(no_v: f32, no_l: f32, roughness: f32) -> f32 {{
    let a2 = roughness * roughness * roughness * roughness;
    let v = no_l * sqrt(no_v * no_v * (1.0-a2) + a2);
    let l = no_v * sqrt(no_l * no_l * (1.0-a2) + a2);
    return 0.5 / max(v+l, 1e-7);
}}
fn retired_fresnel(cosine: f32, f0: vec3<f32>) -> vec3<f32> {{
    let t = clamp(1.0-cosine, 0.0, 1.0);
    return f0 + (vec3<f32>(1.0)-f0) * pow(t, 5.0);
}}
fn retired_sh(direction: vec3<f32>, c: array<vec3<f32>, 9>) -> vec3<f32> {{
    let x = direction.x; let y = direction.y; let z = direction.z;
    return c[0] + c[1]*x + c[2]*y + c[3]*z + c[4]*(x*y)
        + c[5]*(x*z) + c[6]*(y*z) + c[7]*(x*x-y*y) + c[8]*(z*z);
}}
@compute @workgroup_size(1) fn compare() {{
    let grazing = vec2<f32>(0.0001, 0.0003);
    let roughness = array<f32, 6>(0.045, 0.1, 0.25, 0.5, 0.9, 1.0);
    for (var i = 0u; i < 6u; i += 1u) {{
        let r = roughness[i];
        output[i] = vec4<f32>(
            rd_visibility(grazing.x, grazing.y, r, 0u),
            retired_visibility(grazing.x, grazing.y, r),
            rd_fresnel_schlick(0.0001, vec3<f32>(0.04, 0.2, 0.8)).x,
            retired_fresnel(0.0001, vec3<f32>(0.04, 0.2, 0.8)).x);
    }}
    let coefficients = array<vec3<f32>, 9>(
        vec3<f32>(1.106, 0.9, 0.8), vec3<f32>(-1.106, -0.9, -0.8),
        vec3<f32>(0.2, -0.1, 0.3), vec3<f32>(-0.2, 0.1, -0.3),
        vec3<f32>(0.4, 0.3, -0.2), vec3<f32>(-0.4, -0.3, 0.2),
        vec3<f32>(0.1, -0.2, 0.3), vec3<f32>(-0.1, 0.2, -0.3),
        vec3<f32>(0.00001, -0.00002, 0.00003));
    let directions = array<vec3<f32>, 3>(
        normalize(vec3<f32>(0.577, 0.577, 0.577)),
        normalize(vec3<f32>(0.001, 0.999, -0.001)),
        normalize(vec3<f32>(0.707, -0.707, 0.00001)));
    for (var i = 0u; i < 3u; i += 1u) {{
        output[6u+i] = vec4<f32>(rd_irradiance_polynomial(directions[i], coefficients), 0.0);
        output[9u+i] = vec4<f32>(retired_sh(directions[i], coefficients), 0.0);
    }}
}}
"
    )
}

fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("lighting parity requires the primary GPU adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("lighting parity requires a GPU device");
    eprintln!(
        "canonical lighting parity adapter: {:?}",
        adapter.get_info()
    );

    (device, queue)
}

fn evaluate(source: String) -> Vec<f32> {
    let (device, queue) = device();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canonical lighting retired-expression parity"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("canonical lighting parity"),
        layout: None,
        module: &module,
        entry_point: Some("compare"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bytes = 12 * 16;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lighting parity output"),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lighting parity readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lighting parity bindings"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
    queue.submit([encoder.finish()]);
    read_values(&device, &readback)
}

fn read_values(device: &wgpu::Device, readback: &wgpu::Buffer) -> Vec<f32> {
    let (send, receive) = std::sync::mpsc::sync_channel(1);
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
    receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let values: Vec<f32> = mapped
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    drop(mapped);
    readback.unmap();
    values
}

fn assert_parity(values: &[f32]) {
    let mut max_abs = 0.0_f32;
    let mut max_relative = 0.0_f32;
    for (current, retired) in (0..6)
        .flat_map(|i| {
            [
                (values[i * 4], values[i * 4 + 1]),
                (values[i * 4 + 2], values[i * 4 + 3]),
            ]
        })
        .chain((0..9).map(|i| (values[24 + i], values[36 + i])))
    {
        let error = (current - retired).abs();
        max_abs = max_abs.max(error);
        max_relative = max_relative.max(error / current.abs().max(retired.abs()).max(1e-30));
        assert!(
            error <= 2e-6 + 2e-6 * current.abs().max(retired.abs()),
            "retired/current lighting mismatch: {retired:?} vs {current:?}, abs={error}"
        );
    }
    eprintln!(
        "retired/current max abs error={max_abs:.9e}, max relative error={max_relative:.9e}; bound=2e-6 absolute + 2e-6 relative"
    );
}
