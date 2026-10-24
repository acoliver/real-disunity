use crate::contracts::*;
use crate::{BufferReadback, StageId};
use worldrunner_render_gpu::submission::{
    SubmissionAuthority,
    capture::{CaptureError, CaptureWrites},
};

struct Fixture {
    device: wgpu::Device,
    authority: SubmissionAuthority,
    registry: ResourceRegistry,
    buffer: wgpu::Buffer,
    key: AllocationKey,
}
impl Fixture {
    fn new() -> Self {
        let instance = worldrunner_render_gpu::device::headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let authority = SubmissionAuthority::new(queue);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture reduction"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut registry = ResourceRegistry::default();
        let key = registry
            .import_buffer(&buffer, ResourceLifetime::PersistentMutable, None)
            .unwrap();
        Self {
            device,
            authority,
            registry,
            buffer,
            key,
        }
    }
    fn upload(&self, occurrence: u32, value: u32) -> TypedOperation {
        TypedOperation::new(
            RasterId::new(StageId::new("capture fixture", 90), occurrence),
            TypedWork::Upload {
                destination: self
                    .registry
                    .buffer_view(self.key, 0..4, occurrence + 1)
                    .unwrap(),
                bytes: value.to_le_bytes().to_vec(),
            },
        )
    }
}

fn compile_capture(f: &Fixture) -> CompiledCapture {
    let prefix_op = f.upload(0, 123);
    let suffix_op = f.upload(1, 0);
    let capture = f
        .registry
        .compile_capture(
            &f.device,
            std::slice::from_ref(&prefix_op),
            std::slice::from_ref(&suffix_op),
            &[f.key],
        )
        .unwrap();
    assert_eq!(
        capture.prefix().ids().collect::<Vec<_>>(),
        vec![prefix_op.id]
    );
    assert_eq!(
        capture.suffix().ids().collect::<Vec<_>>(),
        vec![suffix_op.id]
    );
    capture
}

#[test]
fn compiled_capture_preserves_ids_and_reads_prefix_before_suffix() {
    let f = Fixture::new();
    let capture = compile_capture(&f);
    let mut readback = BufferReadback::new(&f.device, 4, 4).unwrap();
    let mut final_readback = BufferReadback::new(&f.device, 4, 4).unwrap();
    let prefix = f
        .authority
        .begin_capture(
            &f.device,
            capture.writes().clone(),
            42u64,
            |encoder, _, frame| {
                assert_eq!(*frame, 42);
                capture
                    .prefix()
                    .encode(&f.registry, encoder, None, |_, _| {})
                    .unwrap();
                readback.encode(encoder, &f.buffer).unwrap();
                Ok::<_, &'static str>(())
            },
        )
        .unwrap();
    let suffix = prefix
        .submit()
        .unwrap()
        .resume(
            &f.device,
            |device, submission| {
                readback
                    .read_encoded(device, Some(submission))
                    .map_err(|_| "map")
            },
            |encoder, _, frame, bytes| {
                assert_eq!(*frame, 42);
                assert_eq!(*bytes, 123u32.to_le_bytes());
                let value = u32::from_le_bytes(bytes.as_slice().try_into().unwrap()) + 1;
                let prepared = f
                    .registry
                    .compile_operations(&f.device, &[f.upload(1, value)])
                    .unwrap();
                prepared
                    .encode(&f.registry, encoder, None, |_, _| {})
                    .unwrap();
                final_readback.encode(encoder, &f.buffer).unwrap();
                Ok(())
            },
        )
        .unwrap();
    let bytes = suffix
        .submit()
        .unwrap()
        .finish(&f.device, |device, submission| {
            final_readback.read_encoded(device, Some(submission))
        })
        .unwrap();
    assert_eq!(bytes, 124u32.to_le_bytes());
}

#[test]
fn compiled_capture_rejects_undeclared_writes_and_unowned_overlap() {
    let f = Fixture::new();
    let upload = f.upload(0, 123);
    let capture = f
        .registry
        .compile_capture(&f.device, std::slice::from_ref(&upload), &[], &[])
        .unwrap();
    let undeclared = f.authority.begin_capture(
        &f.device,
        CaptureWrites::new([], [], []).unwrap(),
        (),
        |encoder, _, ()| {
            capture
                .prefix()
                .encode(&f.registry, encoder, None, |_, _| {})
        },
    );
    assert!(matches!(
        undeclared,
        Err(CaptureError::Preparation(ContractError::CaptureWrite(_)))
    ));
    let overlap = f
        .registry
        .compile_capture(&f.device, &[upload], &[f.upload(1, 456)], &[]);
    assert!(matches!(overlap, Err(ContractError::CaptureWrite(_))));
}
