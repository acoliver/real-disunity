//! A caller outside the crate composes its own feature with the stock scene: it prepares the
//! stock depth prefix and the stock scene feature through the public seam, adds a pass of its
//! own that samples the stock outputs, and encodes the frame with the existing frame encoder.
//! The scene is real stock preparation on a synthetic one-mesh scene.
#[path = "stock_composition/callers.rs"]
mod callers;
#[path = "stock_composition/fixture.rs"]
mod fixture;
#[path = "stock_composition/prefix_operations.rs"]
mod prefix_operations;

use callers::{CALLER_FORMAT, Probe, SAMPLER_ENTRY, Sampler, Seen};
use fixture::{SIZE, scene, view};
use std::{cell::RefCell, rc::Rc, time::Duration};
use worldrunner_render_gpu::submission::{
    PrepareError,
    capture::{CaptureError, CaptureObserver, CaptureOutcome, CaptureWrites, WriteKey},
};
use worldrunner_render_plan::SceneEffects;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FrameConfig, FrameError, FramePlan, PassDeclaration,
    PassEntry, PassSlot, Readback, ResourceRef, ToneMap, color_texture,
};
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::{
    SceneView,
    compositor::{
        FeatureTarget, FrameRenderer, PrefixContribution, PrefixScene, StockDepthPrefix,
        prepare_stock_scene,
    },
    device::GpuContext,
    draw::{PassDrawStatistics, SceneGpu},
    frame::FrameUniform,
    offscreen::OffscreenTarget,
    renderer::ScenePipelines,
    wgpu,
};

const NEAR_METRES: f32 = 0.1;
const FAR_METRES: f32 = 20.0;

struct Frame {
    context: GpuContext,
    renderer: FrameRenderer,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    color: AttachmentView,
    depth: AttachmentView,
    caller: wgpu::Texture,
    effects: SceneEffects,
}
impl Frame {
    fn new() -> Self {
        let source = &scene();
        let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty())
            .expect("stock composition acceptance requires a real adapter; never skip");
        let renderer = FrameRenderer::hdr(
            &context,
            SIZE,
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            source.effects(),
        )
        .unwrap();
        let pipelines =
            ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1)
                .unwrap();
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            PreparedScene::new(source).unwrap(),
            &pipelines,
            None,
        )
        .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let caller = color_texture(context.device(), "caller colour", SIZE, CALLER_FORMAT);
        Self {
            context,
            renderer,
            gpu,
            pipelines,
            color,
            depth,
            caller,
            effects: source.effects().unwrap().clone(),
        }
    }

    fn uniform(&self, view: &SceneView) -> FrameUniform {
        FrameUniform::new(view, Some(&self.effects)).unwrap()
    }

    /// Waits for every submission and capture to finish, so the next frame is admitted.
    fn settle(&self) {
        let device = self.context.device();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        let authority = self.context.submission();
        authority
            .await_completion_record(
                device,
                authority.submitted_serial(),
                std::time::Instant::now() + Duration::from_secs(30),
            )
            .unwrap();
        authority.drain_captures(device).unwrap();
    }

    fn read<const N: usize>(&self, texture: &wgpu::Texture) -> Vec<[u8; N]> {
        let config = FrameConfig::new([texture.width(), texture.height()], texture.format(), 1);
        let mut readback = Readback::new(self.context.device(), config, 1 << 20).unwrap();
        let bytes = readback
            .read(self.context.device(), self.context.submission(), texture)
            .unwrap();
        bytes
            .chunks_exact(N)
            .map(|texel| texel.try_into().unwrap())
            .collect()
    }

    /// The depth aspect of a depth texture, one `f32` per texel, copied exactly.
    fn read_depth(&self, texture: &wgpu::Texture) -> Vec<f32> {
        let device = self.context.device();
        let [width, height] = [texture.width(), texture.height()];
        let pitch = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth readback"),
            size: u64::from(pitch * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.context
            .submission()
            .submit_commands([encoder.finish()])
            .unwrap();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |mapped| mapped.unwrap());
        self.settle();
        let bytes = buffer.slice(..).get_mapped_range().unwrap();
        (0..height as usize)
            .flat_map(|row| {
                let start = row * pitch as usize;
                bytes[start..start + width as usize * 4]
                    .chunks_exact(4)
                    .map(f32_of)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The stock scene and the caller's sampler in one frame: the prefix, then the suffix.
    fn encode_composed(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &SceneView,
        sampler: &mut Sampler,
    ) -> Result<(), FrameError> {
        let device = self.context.device();
        let uniform = self.uniform(view);
        let prefix = self.renderer.stock_depth_prefix(
            device,
            PrefixScene {
                gpu: &mut self.gpu,
                pipelines: &self.pipelines,
            },
        )?;
        prefix.encode(
            &mut self.renderer,
            device,
            encoder,
            PrefixScene {
                gpu: &mut self.gpu,
                pipelines: &self.pipelines,
            },
            uniform,
            PrefixContribution {
                plan: &empty_plan(),
                features: &mut [],
            },
        )?;
        self.encode_suffix(encoder, &uniform, sampler)
    }

    fn encode_suffix(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &FrameUniform,
        sampler: &mut Sampler,
    ) -> Result<(), FrameError> {
        let device = self.context.device();
        let mut stock = prepare_stock_scene(
            &mut self.renderer,
            device,
            encoder,
            (&mut self.gpu, &self.pipelines),
            (&self.color, &self.depth),
            *uniform,
        )?;
        let plan = stock.plan();
        let (mut passes, mut views) = (plan.passes.to_vec(), plan.views.to_vec());
        let (pass, view) = Sampler::pass(&self.caller);
        passes.push(pass);
        views.push(view);
        let (declaration, frame) = (stock.declaration(), stock.frame());
        let offscreen = stock.take_offscreen_operations();
        self.renderer.encode_frame(
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
            &(frame, None),
        )
    }

    fn render_composed(&mut self, view: &SceneView, sampler: &mut Sampler) {
        let device = self.context.device().clone();
        let authority = self.context.submission().clone();
        let prepared = authority
            .prepare(&device, |encoder, _| {
                self.encode_composed(encoder, view, sampler)
            })
            .map_err(unwrap_preparation)
            .unwrap();
        prepared.submit().unwrap();
        self.settle();
    }

    /// The write contract of a capture whose prefix is the stock depth prefix and whose suffix
    /// is the stock scene with the caller's pass.
    fn capture_writes(&mut self, prefix: &StockDepthPrefix) -> CaptureWrites {
        let device = self.context.device();
        let scene_keys = self.gpu.capture_write_keys(device).unwrap();
        let plan = prefix.plan();
        let written: Vec<&wgpu::Texture> = plan.views.iter().map(AttachmentView::texture).collect();
        let metadata = self.renderer.capture_texture_keys(&written).unwrap();
        let mut suffix = scene_keys.clone();
        suffix.extend(self.renderer.capture_write_keys());
        suffix.extend(self.renderer.capture_effect_keys(device).unwrap());
        suffix.extend(
            self.renderer
                .capture_texture_keys(&[self.color.texture(), self.depth.texture(), &self.caller])
                .unwrap(),
        );
        let scene_depth = self
            .renderer
            .capture_texture_keys(&[self.depth.texture()])
            .unwrap()[0];
        // The suffix reuses what the prefix completed, except the depth the opaque pass loads.
        suffix.retain(|key| *key == scene_depth || !metadata.contains(key));
        let mut first = scene_keys;
        first.extend(metadata);
        let owned: Vec<WriteKey> = first
            .iter()
            .copied()
            .filter(|key| suffix.contains(key))
            .collect();
        CaptureWrites::new(first, suffix, owned).unwrap()
    }

    /// One frame as a same-frame capture: the prefix submits alone, then the suffix is prepared
    /// and submitted. Returns the suffix's refusal, with the capture's observer.
    fn capture_composed(
        &mut self,
        view: &SceneView,
        sampler: &mut Sampler,
    ) -> (Result<(), FrameError>, CaptureObserver) {
        let device = self.context.device().clone();
        let authority = self.context.submission().clone();
        let uniform = self.uniform(view);
        let prefix = self
            .renderer
            .stock_depth_prefix(
                &device,
                PrefixScene {
                    gpu: &mut self.gpu,
                    pipelines: &self.pipelines,
                },
            )
            .unwrap();
        let writes = self.capture_writes(&prefix);
        let first = authority
            .begin_capture(&device, writes, (), |encoder, _, ()| {
                prefix.encode(
                    &mut self.renderer,
                    &device,
                    encoder,
                    PrefixScene {
                        gpu: &mut self.gpu,
                        pipelines: &self.pipelines,
                    },
                    uniform,
                    PrefixContribution {
                        plan: &empty_plan(),
                        features: &mut [],
                    },
                )
            })
            .unwrap_or_else(|error| panic!("prefix was not prepared: {error:?}"));
        let continuation = first.submit().unwrap();
        let observer = continuation.observer();
        let resumed = continuation.resume(
            &device,
            |_, _| Ok::<_, FrameError>(()),
            |encoder, _, (), ()| self.encode_suffix(encoder, &uniform, sampler),
        );
        let outcome = match resumed {
            Ok(suffix) => {
                suffix
                    .submit()
                    .unwrap()
                    .finish(&device, |_, _| Ok::<_, FrameError>(()))
                    .unwrap();
                Ok(())
            }
            Err(CaptureError::Preparation(error)) => Err(error),
            Err(other) => panic!("unexpected capture failure: {other:?}"),
        };
        (outcome, observer)
    }

    /// The texture behind the prefix plan's prepass depth attachment: the original device depth.
    fn prepass_depth_texture(&mut self) -> wgpu::Texture {
        let prefix = self
            .renderer
            .stock_depth_prefix(
                self.context.device(),
                PrefixScene {
                    gpu: &mut self.gpu,
                    pipelines: &self.pipelines,
                },
            )
            .unwrap();
        let plan = prefix.plan();
        let resource = prepass_depth_of(&plan);
        plan.views
            .iter()
            .find(|view| view.resource() == resource)
            .expect("the prefix attaches its prepass depth")
            .texture()
            .clone()
    }

    fn sampler(&mut self, seen: &Rc<RefCell<Seen>>) -> Sampler {
        let prefix = self
            .renderer
            .stock_depth_prefix(
                self.context.device(),
                PrefixScene {
                    gpu: &mut self.gpu,
                    pipelines: &self.pipelines,
                },
            )
            .unwrap();
        let reads = [
            prepass_depth_of(&prefix.plan()),
            ResourceRef::new("scene color"),
        ];
        Sampler::new(self.context.device(), reads, seen.clone())
    }
}

fn prepass_depth_of(plan: &FramePlan<'_>) -> ResourceRef {
    plan.passes
        .iter()
        .find(|pass| pass.entry() == PassEntry::new(PassSlot::DepthNormalPrepass))
        .and_then(|pass| pass.depth.as_ref())
        .expect("the prefix prepass attaches the device depth")
        .resource
}

fn empty_plan() -> FramePlan<'static> {
    FramePlan {
        passes: &[],
        views: &[],
        requested: &[],
        operations: &[],
    }
}

fn unwrap_preparation(error: PrepareError<FrameError>) -> FrameError {
    match error {
        PrepareError::Preparation(error) => error,
        other => panic!("frame was not admitted: {other:?}"),
    }
}

fn f16(texel: &[u8]) -> f32 {
    half::f16::from_le_bytes([texel[0], texel[1]]).to_f32()
}
fn f32_of(bytes: &[u8]) -> f32 {
    f32::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn gpu_caller_feature_samples_the_stock_device_depth_and_lit_colour_in_a_later_entry() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut frame = Frame::new();
    let before = frame.gpu.statistics().passes;
    assert_eq!(before, PassDrawStatistics::default());

    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut sampler = frame.sampler(&seen);
    frame.render_composed(&view(0), &mut sampler);

    // Backing texture identity: the caller's sampled views are the stock allocations.
    {
        let seen = seen.borrow();
        assert_eq!(
            seen.colour.as_ref().unwrap(),
            frame.renderer.targets().scene().texture(),
            "the lit colour is the stock scene target"
        );
        assert_eq!(
            seen.depth.as_ref().unwrap(),
            &frame.prepass_depth_texture(),
            "the device depth is the prepass depth attachment"
        );
        assert_eq!(
            seen.completed,
            [
                (PassEntry::new(PassSlot::DepthNormalPrepass), true),
                (PassEntry::nth(PassSlot::DepthNormalPrepass, 1), false),
                (PassEntry::new(PassSlot::DepthPyramid(0)), true),
                (PassEntry::new(PassSlot::Opaque), false),
                (SAMPLER_ENTRY, false),
            ],
            "the prefix completes occurrence 0 of its roles and nothing later"
        );
    }

    // Sampled pixels match the stock outputs.
    let caller_pixels: Vec<[u8; 16]> = frame.read(&frame.caller);
    let lit: Vec<[u8; 8]> = frame.read(frame.renderer.targets().scene().texture());
    let linear_target = frame
        .renderer
        .feature_target(FeatureTarget::Depth)
        .expect("the frame owns screen-space metadata")
        .clone();
    let linear: Vec<[u8; 4]> = frame.read(&linear_target);
    let device_depth_texture = frame.prepass_depth_texture();
    let stock_depth = frame.read_depth(&device_depth_texture);
    assert_eq!(caller_pixels.len(), lit.len());
    assert_eq!(caller_pixels.len(), stock_depth.len());
    let (mut covered, mut bright) = (0, 0);
    for (index, (sample, stock)) in caller_pixels.iter().zip(&lit).enumerate() {
        for channel in 0..3 {
            assert_eq!(
                f32_of(&sample[channel * 4..channel * 4 + 4]).to_bits(),
                f16(&stock[channel * 2..channel * 2 + 2]).to_bits(),
                "lit colour, pixel {index}, channel {channel}"
            );
        }
        let device_depth = f32_of(&sample[12..16]);
        assert_eq!(
            device_depth.to_bits(),
            stock_depth[index].to_bits(),
            "device depth, pixel {index}"
        );
        if device_depth < 1.0 {
            covered += 1;
            let expected =
                NEAR_METRES * FAR_METRES / (FAR_METRES - device_depth * (FAR_METRES - NEAR_METRES));
            let stock_linear = f32_of(&linear[index]);
            assert!(
                (expected - stock_linear).abs() <= 1e-3 * stock_linear,
                "pixel {index}: device depth {device_depth} is {expected} m, stock linear \
                 depth is {stock_linear} m"
            );
        }
        bright += usize::from(f16(&stock[..2]) > 0.05);
    }
    assert!(covered > 0, "the quad covers part of the frame");
    assert!(bright > 0, "the stock colour is lit");

    let after = frame.gpu.statistics().passes;
    assert!(after.prepass_objects.meshes.calls > 0, "{after:?}");
    assert!(after.opaque_objects.meshes.calls > 0, "{after:?}");
    assert_ne!(after, before);
}

#[test]
fn gpu_refused_suffix_leaves_the_completed_frame_and_a_fresh_frame_succeeds() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut frame = Frame::new();
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut sampler = frame.sampler(&seen);
    let authority = frame.context.submission().clone();

    // A completed frame, from the camera at x = 0.
    frame.render_composed(&view(0), &mut sampler);
    let completed = frame.gpu.statistics();
    assert!(completed.passes.opaque_objects.meshes.calls > 0);
    assert!(completed.passes.prepass_objects.meshes.calls > 0);
    let colour =
        |frame: &Frame| -> Vec<[u8; 8]> { frame.read(frame.renderer.targets().scene().texture()) };
    let completed_colour = colour(&frame);

    // The prefix of a frame from another camera submits; the caller refuses the suffix.
    sampler.refuse.set(true);
    let serial = authority.submitted_serial();
    let (refused, observer) = frame.capture_composed(&view(1_500), &mut sampler);
    assert!(
        matches!(
            refused,
            Err(FrameError::FeaturePreparation {
                feature: "caller sampler",
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(observer.outcome(), Some(CaptureOutcome::PreparationFailed));
    assert_eq!(
        authority.submitted_serial(),
        serial + 1,
        "only the prefix submitted"
    );
    frame.settle();
    assert_eq!(
        frame.gpu.statistics(),
        completed,
        "a refused suffix leaves the completed frame's statistics"
    );
    assert_eq!(
        colour(&frame),
        completed_colour,
        "the scene colour was not redrawn"
    );

    // A fresh frame is admitted and completes.
    sampler.refuse.set(false);
    frame.render_composed(&view(1_500), &mut sampler);
    let fresh = frame.gpu.statistics();
    assert!(fresh.passes.opaque_objects.meshes.calls > 0);
    assert_ne!(
        colour(&frame),
        completed_colour,
        "the fresh frame drew its own camera"
    );

    // The same capture, completed, publishes the statistics of its own frame.
    sampler.refuse.set(false);
    let (completed_capture, observer) = frame.capture_composed(&view(0), &mut sampler);
    assert_eq!(completed_capture, Ok(()));
    assert_eq!(observer.outcome(), Some(CaptureOutcome::Complete));
    frame.settle();
    assert_eq!(
        colour(&frame),
        completed_colour,
        "the capture drew the camera at x = 0"
    );
    assert_eq!(frame.gpu.statistics().passes, completed.passes);
}
/// A caller pass that joins the stock prefix, with the colour target it clears.
struct Contribution {
    target: wgpu::Texture,
    pass: PassDeclaration,
    view: AttachmentView,
}
impl Contribution {
    fn new(device: &wgpu::Device) -> Self {
        let target = color_texture(
            device,
            "caller prefix colour",
            SIZE,
            wgpu::TextureFormat::Rgba16Float,
        );
        let (pass, view) = Probe::pass(&target);
        Self { target, pass, view }
    }

    fn plan(&self) -> FramePlan<'_> {
        FramePlan {
            passes: std::slice::from_ref(&self.pass),
            views: std::slice::from_ref(&self.view),
            requested: &[],
            operations: &[],
        }
    }
}

#[test]
fn gpu_a_prefix_contribution_with_an_unproduced_read_is_refused_before_anything_is_encoded() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut frame = Frame::new();
    let device = frame.context.device().clone();
    let contribution = Contribution::new(&device);
    let completed = Rc::new(RefCell::new(Vec::new()));
    let mut unproduced = Probe {
        reads: ResourceRef::new("nothing produces this"),
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
                        declaration: unproduced.declaration(&contribution.target),
                        prepared: &mut unproduced,
                    }],
                },
            )
        })
        .map_err(unwrap_preparation)
        .expect_err("an unproduced read is refused");
    assert_eq!(
        refused,
        FrameError::MissingProducer(ResourceRef::new("nothing produces this"))
    );
    assert!(completed.borrow().is_empty());
}

#[test]
fn gpu_a_prefix_contribution_is_encoded_with_the_prefix_and_found_completed_by_the_suffix() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut frame = Frame::new();
    let device = frame.context.device().clone();
    let contribution = Contribution::new(&device);
    let completed = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::new(RefCell::new(Seen::default()));
    // A read of the prepass depth is produced earlier in the same prefix.
    let mut probe = Probe {
        reads: frame.sampler(&seen).reads[0],
        completed: completed.clone(),
    };
    let prepared = frame
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
                        declaration: probe.declaration(&contribution.target),
                        prepared: &mut probe,
                    }],
                },
            )?;
            let mut stock = prepare_stock_scene(
                &mut frame.renderer,
                &device,
                encoder,
                (&mut frame.gpu, &frame.pipelines),
                (&frame.color, &frame.depth),
                uniform,
            )?;
            let plan = stock.plan();
            let (mut passes, mut views) = (plan.passes.to_vec(), plan.views.to_vec());
            passes.push(contribution.pass.clone());
            views.push(contribution.view.clone());
            let (declaration, stock_frame) = (stock.declaration(), stock.frame());
            let offscreen = stock.take_offscreen_operations();
            frame.renderer.encode_frame(
                &device,
                encoder,
                (&passes, &views),
                offscreen.as_ref().map(|work| work as _),
                &mut [
                    BorrowedFeature {
                        declaration,
                        prepared: &mut stock,
                    },
                    BorrowedFeature {
                        declaration: probe.declaration(&contribution.target),
                        prepared: &mut probe,
                    },
                ],
                &(stock_frame, None),
            )
        })
        .map_err(unwrap_preparation)
        .unwrap();
    prepared.submit().unwrap();
    frame.settle();
    assert_eq!(
        *completed.borrow(),
        [false, true],
        "prepared in the prefix before it ran, then found completed by the suffix"
    );
    assert!(frame.gpu.statistics().passes.opaque_objects.meshes.calls > 0);
}
