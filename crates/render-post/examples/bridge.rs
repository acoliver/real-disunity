//! A consumer keeps its shaders while adopting checked execution and submission.
#[cfg(test)]
#[path = "bridge/checks.rs"]
mod checks;
#[path = "bridge/reference.rs"]
mod reference;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::{
    AttachmentLoad, CopyOutput, FrameConfig, Readback, StageId,
    contracts::{
        AllocationKey, CheckedDraw, CheckedView, ColorAttachment, ContractError, RasterId,
        RasterPass, ResourceLifetime, ResourceRegistry, ResourceUse, Semantic, TextureRange,
        TypedOperation, TypedWork, UseKind,
    },
};

fn expected_color(reverse: bool) -> [u8; 4] {
    let destination = [0_u16, 255, 0, 255];
    let source = [255_u16, 0, 0, 255];
    // Source alpha is 1/4; the destination alpha remains opaque.
    reference::quantize(
        std::array::from_fn(|channel| {
            if reverse || channel == 3 {
                destination[channel] * 4
            } else {
                source[channel] + destination[channel] * 3
            }
        }),
        4,
    )
}

fn id(name: &'static str) -> RasterId {
    RasterId::new(StageId::new(name, 0), 0)
}
fn texture(device: &wgpu::Device, samples: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("procedural bridge"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if samples == 1 {
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}
fn view(
    registry: &mut ResourceRegistry,
    texture: &wgpu::Texture,
    lifetime: ResourceLifetime,
) -> CheckedView {
    let key = registry
        .import(texture, lifetime, Semantic::LinearColor, None)
        .unwrap();
    registry
        .view(key, TextureRange::single(0, 0, wgpu::TextureAspect::All), 1)
        .unwrap()
}
fn pass(name: &'static str, target: CheckedView, resolve: Option<CheckedView>) -> RasterPass {
    RasterPass {
        id: id(name),
        colors: vec![Some(ColorAttachment {
            view: target,
            before: None,
            load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
            resolve,
        })],
        depth: None,
        uses: vec![],
        bindings: vec![],
    }
}
fn consumer(device: &wgpu::Device) -> CheckedDraw {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("consumer procedural shader"),
        source: wgpu::ShaderSource::Wgsl("@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> { let p = array(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.)); return vec4<f32>(p[i], 0., 1.); } @fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1., 0., 0., 0.25); }".into()),
    });
    CheckedDraw::new(
        device,
        &wgpu::RenderPipelineDescriptor {
            label: Some("consumer alpha draw"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        },
        vec![],
        0..3,
        0..1,
    )
    .unwrap()
}
fn recipe(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    output: &wgpu::Texture,
    reverse: bool,
) -> (Vec<TypedOperation>, Vec<TypedOperation>, AllocationKey) {
    let imported = texture(device, 1);
    let input = view(registry, &imported, ResourceLifetime::PersistentMutable);
    let mut init = pass("consumer persistent initialization", input.clone(), None);
    init.colors[0].as_mut().unwrap().load = AttachmentLoad::ClearColor(wgpu::Color::GREEN);
    let msaa = view(registry, &texture(device, 4), ResourceLifetime::FrameLocal);
    let resolved = view(registry, &texture(device, 1), ResourceLifetime::FrameLocal);
    let exported = view(registry, &texture(device, 1), ResourceLifetime::FrameLocal);
    let target = view(registry, output, ResourceLifetime::PersistentMutable);
    let mut mixed = pass("mixed providers", msaa.clone(), Some(resolved));
    mixed
        .uses
        .push(ResourceUse::read(input.clone(), UseKind::Sampled));
    let stock = CopyOutput::checked_draw(device, registry, input, 4).unwrap();
    let mut draws = vec![stock, consumer(device)];
    if reverse {
        draws.reverse();
    }
    let mut prefix = vec![
        TypedOperation::new(
            init.id,
            TypedWork::Draws {
                pass: init,
                draws: vec![],
            },
        ),
        TypedOperation::new(mixed.id, TypedWork::Draws { pass: mixed, draws }),
        TypedOperation::new(
            id("explicit resolve"),
            TypedWork::Resolve {
                source: msaa,
                destination: exported.clone(),
            },
        ),
    ];
    let initial = registry
        .view(target.allocation(), target.range().clone(), 0)
        .unwrap();
    let initialize = pass("consumer output initialization", initial, None);
    prefix.push(TypedOperation::new(
        initialize.id,
        TypedWork::Draws {
            pass: initialize,
            draws: vec![],
        },
    ));
    let mut consumer_pass = pass("consumer exported target", target.clone(), None);
    consumer_pass
        .uses
        .push(ResourceUse::read(exported.clone(), UseKind::Sampled));
    let copy = CopyOutput::checked_draw(device, registry, exported, 1).unwrap();
    let suffix = vec![TypedOperation::new(
        consumer_pass.id,
        TypedWork::Draws {
            pass: consumer_pass,
            draws: vec![copy],
        },
    )];
    (prefix, suffix, target.allocation())
}
/// Runs the same declared work through cancellation and capture boundaries.
pub fn run(cancel: bool) {
    run_order(cancel, false);
}

/// Reversing providers demonstrates that packet order is observable.
pub fn run_order(cancel: bool, reverse: bool) {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let authority = SubmissionAuthority::new(queue);
    let output = texture(&device, 1);
    let mut registry = ResourceRegistry::default();
    let (prefix, suffix, owned) = recipe(&device, &mut registry, &output, reverse);
    assert!(registry.compile_operations(&device, &suffix).is_err());
    let capture = registry
        .compile_capture(&device, &prefix, &suffix, &[owned])
        .unwrap();
    assert_eq!(
        capture.prefix().ids().collect::<Vec<_>>(),
        prefix.iter().map(|op| op.id).collect::<Vec<_>>()
    );
    if cancel {
        cancel_prefix(&device, &authority, &registry, &capture, &output);
    }
    let before = authority.submitted_serial();
    let continuation = authority
        .begin_capture(&device, capture.writes().clone(), (), |encoder, _, ()| {
            capture.prefix().encode_checked(&registry, encoder, None)
        })
        .unwrap()
        .submit()
        .unwrap();
    continuation
        .resume(
            &device,
            |_, _| Ok::<_, ContractError>(()),
            |encoder, _, (), ()| capture.suffix().encode_checked(&registry, encoder, None),
        )
        .unwrap()
        .submit()
        .unwrap()
        .finish(&device, |_, _| Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(authority.submitted_serial(), before + 2);
    let expected = expected_color(reverse).repeat(4);
    assert_eq!(read(&device, &authority, &output), expected);
    if let TypedWork::Draws { pass, .. } = &prefix[1].work {
        let resolved = pass.colors[0].as_ref().unwrap().resolve.as_ref().unwrap();
        assert_eq!(
            read(&device, &authority, resolved.view().texture()),
            expected
        );
    }
}
#[allow(dead_code)]
fn main() {
    run(false);
}

fn cancel_prefix(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    registry: &ResourceRegistry,
    capture: &worldrunner_render_post::contracts::CompiledCapture,
    output: &wgpu::Texture,
) {
    let published = Arc::new(AtomicUsize::new(0));
    let edited = published.clone();
    let prepared = authority
        .prepare(device, |encoder, edits| {
            capture.prefix().encode_checked(registry, encoder, None)?;
            edits.stage(move || {
                edited.store(1, Ordering::SeqCst);
            });
            Ok::<_, ContractError>(())
        })
        .unwrap();
    prepared.cancel();
    assert_eq!(authority.submitted_serial(), 0);
    assert_eq!(published.load(Ordering::SeqCst), 0);
    assert_eq!(read(device, authority, output), [0; 16]);
}

fn read(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    Readback::new(
        device,
        FrameConfig::new([2, 2], wgpu::TextureFormat::Rgba8Unorm, 1),
        512,
    )
    .unwrap()
    .read(device, authority, texture)
    .unwrap()
}
