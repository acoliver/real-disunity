use crate::{BufferReadback, PrepareContext};

fn buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("region copy fixture"),
        size,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bytes(device: &wgpu::Device, queue: &wgpu::Queue, source: &wgpu::Buffer) -> Vec<u8> {
    let mut capture = BufferReadback::new(device, source.size(), 4096).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    capture.encode(&mut encoder, source).unwrap();
    let submission = queue.submit([encoder.finish()]);
    capture.read_encoded(device, Some(submission)).unwrap()
}

#[test]
fn a_region_copy_reads_bytes_uploaded_earlier_in_the_same_context() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let source = buffer(&device, 32);
    let destination = buffer(&device, 32);
    let pattern: Vec<u8> = (0..32).collect();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.upload_buffer(&destination, 0, &[0xEE; 32]);
            context.upload_buffer(&source, 0, &pattern);
            context.copy_buffer_region((&source, 8), (&destination, 20), 12);
            context.copy_buffer_region((&source, 0), (&destination, 0), 4);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let mut expected = vec![0xEE; 32];
    expected[20..32].copy_from_slice(&pattern[8..20]);
    expected[..4].copy_from_slice(&pattern[..4]);
    assert_eq!(bytes(&device, &queue, &destination), expected);
}

#[test]
fn a_cancelled_context_leaves_the_destination_unchanged() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let source = buffer(&device, 16);
    let destination = buffer(&device, 16);
    queue.write_buffer(&source, 0, &[7; 16]);
    queue.write_buffer(&destination, 0, &[1; 16]);
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.copy_buffer_region((&source, 0), (&destination, 0), 16);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.cancel();
    assert_eq!(bytes(&device, &queue, &destination), vec![1; 16]);
}

const DOUBLE: &str = "
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<storage, read_write> destination: array<u32>;
@compute @workgroup_size(4)
fn double(@builtin(global_invocation_id) id: vec3<u32>) {
    destination[id.x] = source[id.x] * 2u;
}
";

fn storage(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dispatch fixture"),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[test]
fn a_dispatch_reads_earlier_uploads_and_later_copies_read_its_output() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("double"),
        source: wgpu::ShaderSource::Wgsl(DOUBLE.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("double"),
        layout: None,
        module: &module,
        entry_point: Some("double"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let (source, doubled, copied) = (
        storage(&device, 16),
        storage(&device, 16),
        buffer(&device, 16),
    );
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("double"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: source.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: doubled.as_entire_binding(),
            },
        ],
    });
    let words = [3_u32, 5, 0x4000_0000, 11];
    let pattern: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.upload_buffer(&source, 0, &pattern);
            context.dispatch("double", &pipeline, &group, [1, 1, 1]);
            context.copy_buffer_region((&doubled, 0), (&copied, 0), 16);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let expected: Vec<u8> = words
        .iter()
        .flat_map(|word| (word * 2).to_le_bytes())
        .collect();
    assert_eq!(bytes(&device, &queue, &copied), expected);
}

#[test]
fn steady_frames_reuse_upload_staging_and_land_every_upload_on_the_gpu() {
    use crate::upload_staging_tests::{destination, expected, frames, read};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let target = destination(&device, 40 * 64);
    let created = frames(&device, &authority, &target, (32, 40, 3));
    assert_eq!(read(&device, &queue, &target), expected(32, 40));
    assert!(created[31] <= 3, "{created:?}");
    assert!(
        created[8..].iter().all(|&count| count == created[31]),
        "staging chunks created after each frame: {created:?}"
    );
}
