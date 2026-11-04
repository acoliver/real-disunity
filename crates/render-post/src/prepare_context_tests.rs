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
