use super::geometry::{fixture, float_bytes, mesh};
use crate::bridge_support::*;
use worldrunner_render_post::{BufferReadback, contracts::*};

#[test]
fn continuation_upload_drives_dynamic_mesh_binding() {
    let (device, authority) = gpu();
    let mut registry = ResourceRegistry::default();
    let (effective, draw_op, output) = continuation_draw(&device, &mut registry);
    let (seed, prefix) = seed_input(&device, &mut registry);
    let upload = |bytes| {
        TypedOperation::new(
            id("continuation dynamic upload"),
            TypedWork::Upload {
                destination: effective.clone(),
                bytes,
            },
        )
    };
    let capture = registry
        .compile_capture(
            &device,
            &[prefix],
            &[upload(vec![0; 16]), draw_op.clone()],
            &[effective.allocation()],
        )
        .unwrap();
    let mut readback = BufferReadback::new(&device, 4, 4).unwrap();
    authority
        .begin_capture(&device, capture.writes().clone(), (), |encoder, _, ()| {
            capture.prefix().encode_checked(&registry, encoder, None)?;
            readback.encode(encoder, &seed).unwrap();
            Ok::<_, ContractError>(())
        })
        .unwrap()
        .submit()
        .unwrap()
        .resume(
            &device,
            |device, submission| {
                let bytes = readback.read_encoded(device, Some(submission)).unwrap();
                let green = f32::from(
                    u16::try_from(u32::from_le_bytes(bytes.try_into().unwrap())).unwrap(),
                );
                Ok::<_, ContractError>(float_bytes(&[0., green, 0., 1.]))
            },
            |encoder, _, (), bytes| {
                let plan = registry
                    .compile_operations(&device, &[upload(bytes.clone()), draw_op.clone()])?;
                plan.encode_checked(&registry, encoder, None)
            },
        )
        .unwrap()
        .submit()
        .unwrap()
        .finish(&device, |_, _| Ok::<_, ContractError>(()))
        .unwrap();
    assert_eq!(read(&device, &authority, &output), [0, 255, 0, 255]);
}

fn continuation_draw(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
) -> (BufferView, TypedOperation, wgpu::Texture) {
    let fixture = fixture(device, registry);
    let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
    let base = registry
        .buffer_view(fixture.uniform.allocation(), 0..16, 1)
        .unwrap();
    let effective = registry
        .buffer_view(base.allocation(), alignment..alignment + 16, 1)
        .unwrap();
    let binding = BufferBinding {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        view: base,
        kind: BufferBindingKind::Uniform,
        dynamic_offset: Some(u32::try_from(alignment).unwrap()),
    };
    let bindings = CheckedBindings::build_with_buffers(device, registry, &[], &[binding]).unwrap();
    let output = texture(device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let target = view(registry, &output, 0, 1);
    let raster = pass("continuation mesh", target);
    let mut draw_op = TypedOperation::new(
        raster.id,
        TypedWork::Draws {
            pass: raster,
            draws: vec![mesh(device, fixture.geometry, bindings, false).unwrap()],
        },
    );
    draw_op.buffers = fixture.declarations;
    *draw_op.buffers.last_mut().unwrap() =
        BufferUse::read(effective.clone(), BufferUseKind::Uniform);
    (effective, draw_op, output)
}

fn seed_input(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
) -> (wgpu::Buffer, TypedOperation) {
    let seed = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let key = registry
        .import_buffer(&seed, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let prefix = TypedOperation::new(
        id("continuation input"),
        TypedWork::Upload {
            destination: registry.buffer_view(key, 0..4, 1).unwrap(),
            bytes: 1u32.to_le_bytes().to_vec(),
        },
    );
    (seed, prefix)
}
