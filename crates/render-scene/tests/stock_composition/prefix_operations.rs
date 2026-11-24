//! A public stock-prefix contribution can record retained texture work before its first pass, and
//! cannot read the frame's snapshots.
use super::{
    Contribution, Frame, Sampler, Seen,
    callers::Probe,
    fixture::{SIZE, view},
    prepass_depth_of, unwrap_preparation,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc,
};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FrameError, FrameOperations, FramePlan, OperationsGroup,
    OperationsOutput, PassEntry, PassSlot, ResourceRef, TimingState, color_texture,
};
use worldrunner_render_scene::{
    compositor::{PrefixContribution, PrefixScene, StockDepthPrefix, prepare_stock_scene},
    frame::FrameUniform,
    wgpu,
};

struct ClearRetained {
    texture: wgpu::Texture,
    output: OperationsOutput,
    calls: Cell<usize>,
}
impl FrameOperations for ClearRetained {
    fn produces(&self) -> &[OperationsOutput] {
        std::slice::from_ref(&self.output)
    }

    fn encode_operations(
        &self,
        _: Option<&TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        self.calls.set(self.calls.get() + 1);
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("synthetic retained red clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        }));
        Ok(())
    }
}

fn encode_suffix(
    frame: &mut Frame,
    encoder: &mut wgpu::CommandEncoder,
    uniform: &FrameUniform,
    sampler: &mut Sampler,
    retained: &AttachmentView,
) -> Result<(), worldrunner_render_post::FrameError> {
    let device = frame.context.device();
    let mut stock = prepare_stock_scene(
        &mut frame.renderer,
        device,
        encoder,
        (&mut frame.gpu, &frame.pipelines),
        (&frame.color, &frame.depth),
        *uniform,
    )?;
    let plan = stock.plan();
    let (mut passes, mut views) = (plan.passes.to_vec(), plan.views.to_vec());
    let (pass, sampler_view) = Sampler::pass(&frame.caller);
    passes.push(pass);
    views.extend([sampler_view, retained.clone()]);
    let (declaration, stock_frame) = (stock.declaration(), stock.frame());
    let offscreen = stock.take_offscreen_operations();
    frame.renderer.encode_frame(
        device,
        encoder,
        (&passes, &views),
        offscreen.as_ref().map(|work| work as _),
        &mut [
            BorrowedFeature {
                declaration,
                prepared: &mut stock,
            },
            BorrowedFeature {
                declaration: sampler.declaration(),
                prepared: sampler,
            },
        ],
        &(stock_frame, None),
    )
}

fn encode_composed(
    frame: &mut Frame,
    encoder: &mut wgpu::CommandEncoder,
    readback: &wgpu::Buffer,
    inputs: (
        &StockDepthPrefix,
        &FramePlan<'_>,
        &ClearRetained,
        &mut Sampler,
        &AttachmentView,
        &FrameUniform,
    ),
) -> Result<(), worldrunner_render_post::FrameError> {
    let (prefix, contribution, operation, sampler, retained, uniform) = inputs;
    let device = frame.context.device();
    prefix.encode(
        &mut frame.renderer,
        device,
        encoder,
        PrefixScene {
            gpu: &mut frame.gpu,
            pipelines: &frame.pipelines,
        },
        *uniform,
        PrefixContribution {
            plan: contribution,
            features: &mut [],
        },
    )?;
    assert_eq!(
        operation.calls.get(),
        1,
        "the prefix encoded its operation once"
    );
    encode_suffix(frame, encoder, uniform, sampler, retained)?;
    assert_eq!(
        operation.calls.get(),
        1,
        "the suffix did not repeat its operation"
    );
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &frame.caller,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(SIZE[1]),
            },
        },
        wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
    );
    Ok(())
}

fn red_pixel(bytes: &[u8]) -> [f32; 3] {
    [
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    ]
}

fn assert_red_output(frame: &Frame, readback: &wgpu::Buffer) {
    let (mapped, result) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |status| {
            mapped.send(status).unwrap();
        });
    frame.settle();
    result.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    assert_eq!(
        red_pixel(&bytes).map(f32::to_bits),
        [1.0_f32, 0.0, 0.0].map(f32::to_bits)
    );
}

fn retained_operation(device: &wgpu::Device) -> (ClearRetained, AttachmentView) {
    let resource = ResourceRef::new("synthetic retained operation colour");
    let format = wgpu::TextureFormat::Rgba16Float;
    let texture = color_texture(device, "synthetic retained red", SIZE, format);
    let retained = AttachmentView::new(resource, &texture)
        .unwrap()
        .with_retained_contents();
    let operation = ClearRetained {
        texture,
        output: OperationsOutput::new(resource, format, SIZE),
        calls: Cell::new(0),
    };
    (operation, retained)
}

fn output_readback(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("same-encoder output readback"),
        size: u64::from(SIZE[1] * 1024),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

#[test]
fn gpu_stock_prefix_encodes_contribution_operations_once_and_sampler_reads_the_retained_output() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut frame = Frame::new();
    let device = frame.context.device().clone();
    let (operation, retained) = retained_operation(&device);
    let resource = retained.resource();
    let group = OperationsGroup {
        before: Some(PassEntry::new(PassSlot::DepthNormalPrepass)),
        work: &operation,
    };
    let contribution = FramePlan {
        passes: &[],
        views: std::slice::from_ref(&retained),
        requested: &[],
        operations: std::slice::from_ref(&group),
    };
    let prefix = frame
        .renderer
        .stock_depth_prefix(
            &device,
            PrefixScene {
                gpu: &mut frame.gpu,
                pipelines: &frame.pipelines,
            },
        )
        .unwrap();
    let seen = Rc::new(std::cell::RefCell::new(Seen::default()));
    let mut sampler = Sampler::new(&device, [prepass_depth_of(&prefix.plan()), resource], seen);
    let uniform = frame.uniform(&view(0));
    let authority = frame.context.submission().clone();
    let serial = authority.submitted_serial();
    let readback = output_readback(&device);
    let prepared = authority
        .prepare(&device, |encoder, _| {
            encode_composed(
                &mut frame,
                encoder,
                &readback,
                (
                    &prefix,
                    &contribution,
                    &operation,
                    &mut sampler,
                    &retained,
                    &uniform,
                ),
            )
        })
        .map_err(unwrap_preparation)
        .unwrap();
    assert_eq!(
        authority.submitted_serial(),
        serial,
        "recording does not submit"
    );
    assert_eq!(operation.calls.get(), 1, "the composed frame encoded once");
    prepared.submit().unwrap();
    assert_eq!(authority.submitted_serial(), serial + 1);
    assert_eq!(
        operation.calls.get(),
        1,
        "suffix composition did not repeat it"
    );
    assert_red_output(&frame, &readback);
}

/// A pass of the stock depth prefix cannot read the frame's snapshots: the prefix completes the
/// depth and normals before the frame that writes the snapshots is planned, so a read of one is
/// refused by the snapshot's name, the post-water depth and its device depth included.
#[test]
fn gpu_a_prefix_contribution_that_reads_a_frame_snapshot_is_refused_by_its_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    for name in [
        "opaque snapshot color",
        "opaque snapshot depth",
        "opaque snapshot device depth",
        "post-water depth",
        "post-water device depth",
    ] {
        let mut frame = Frame::new();
        let device = frame.context.device().clone();
        let contribution = Contribution::new(&device);
        let completed = Rc::new(RefCell::new(Vec::new()));
        let mut reader = Probe {
            reads: ResourceRef::new(name),
            completed: completed.clone(),
        };
        let refused = frame
            .context
            .submission()
            .clone()
            .prepare(&device, |encoder, _| {
                let uniform = frame.uniform(&view(0));
                let prefix = frame.renderer.stock_depth_prefix(
                    &device,
                    PrefixScene {
                        gpu: &mut frame.gpu,
                        pipelines: &frame.pipelines,
                    },
                )?;
                prefix.encode(
                    &mut frame.renderer,
                    &device,
                    encoder,
                    PrefixScene {
                        gpu: &mut frame.gpu,
                        pipelines: &frame.pipelines,
                    },
                    uniform,
                    PrefixContribution {
                        plan: &contribution.plan(),
                        features: &mut [BorrowedFeature {
                            declaration: reader.declaration(&contribution.target),
                            prepared: &mut reader,
                        }],
                    },
                )
            })
            .map_err(unwrap_preparation)
            .expect_err("a snapshot read in the prefix is refused");
        assert_eq!(
            refused,
            FrameError::MissingProducer(ResourceRef::new(name)),
            "{name}"
        );
        assert!(completed.borrow().is_empty(), "{name}");
    }
}
