//! The stock shadow cascades lent to a game's features: what a feature reads in the frame that
//! wrote them, and when the handle refuses.
use super::cascades::source_with;
use super::refresh::depth_layers;
use super::scheduler::uniforms;
use super::*;
use crate::compositor::{
    CascadeSelection, ColorOutput, FrameInputError, FrameShadowHandle, FrameShadows, GameBuild,
    GameFeatureFactory, GamePassSpec, GamePasses, GameTargets, TargetDeclaration, TargetExtent,
};
use crate::offscreen::OffscreenTarget;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedFeature,
};

const SAMPLED: &str = "sampled cascades";
/// Texels of the cascade layer between two samples of the sampling pass, so a 4 x 4 target
/// covers a 512 x 512 layer.
const STRIDE: u32 = 128;

const SAMPLE_WGSL: &str = "
@group(0) @binding(0) var cascades: texture_depth_2d_array;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(pixel.xy) * 128;
    return vec4<f32>(textureLoad(cascades, texel, 0, 0), 0.0, 0.0, 1.0);
}";

type Seen = Arc<Mutex<Vec<Result<FrameShadows, FrameInputError>>>>;

/// A game that reads the lent shadows in every `prepare`. With `sample` it also draws a pass that
/// samples the first cascade layer into a 4 x 4 target.
struct Probe {
    seen: Seen,
    sample: bool,
}

thread_local! {
    /// The handle the last build received. The handle is single-threaded and the factory is not,
    /// so the factory leaves it here for the test that installed it.
    static BUILT: RefCell<Option<FrameShadowHandle>> = const { RefCell::new(None) };
}

impl Probe {
    fn install(
        scheduler: &mut crate::compositor::FrameRenderer,
        context: &GpuContext,
        sample: bool,
    ) -> (Seen, FrameShadowHandle) {
        let seen = Seen::default();
        GamePasses::new()
            .with(Arc::new(Probe {
                seen: seen.clone(),
                sample,
            }))
            .install(scheduler, context, None)
            .unwrap();
        let handle = BUILT
            .with(|built| built.borrow_mut().take())
            .expect("built");
        (seen, handle)
    }
}

impl GameFeatureFactory for Probe {
    fn name(&self) -> &'static str {
        "shadow probe"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        if !self.sample {
            return Vec::new();
        }
        vec![
            TargetDeclaration::new(
                SAMPLED,
                wgpu::TextureFormat::R32Float,
                TargetExtent::Fixed([4, 4]),
            )
            .with_usage(wgpu::TextureUsages::COPY_SRC),
        ]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        BUILT.with(|built| *built.borrow_mut() = Some(context.shadows.clone()));
        let device = context.context.device();
        let sampler = self.sample.then(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cascade sampling"),
                source: wgpu::ShaderSource::Wgsl(SAMPLE_WGSL.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("cascade sampling"),
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
                    targets: &[Some(wgpu::TextureFormat::R32Float.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        Ok(Box::new(ProbeFeature {
            seen: self.seen.clone(),
            shadows: context.shadows.clone(),
            targets: context.targets.clone(),
            device: device.clone(),
            sampler,
        }))
    }
}

struct ProbeFeature {
    seen: Seen,
    shadows: FrameShadowHandle,
    targets: GameTargets,
    device: wgpu::Device,
    sampler: Option<wgpu::RenderPipeline>,
}

impl FrameFeature for ProbeFeature {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let mut declared = Vec::new();
        if self.sampler.is_some() {
            let layer = TargetDeclaration::new(
                SAMPLED,
                wgpu::TextureFormat::R32Float,
                TargetExtent::Fixed([4, 4]),
            )
            .layer(0);
            let spec =
                GamePassSpec::new(GamePlace::BeforeOpaque, 0).writing([ColorOutput::Target(layer)]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(ProbeFrame {
            seen: self.seen.clone(),
            shadows: self.shadows.clone(),
            device: self.device.clone(),
            pipeline: self.sampler.clone(),
            group: None,
        }))
    }
}

struct ProbeFrame {
    seen: Seen,
    shadows: FrameShadowHandle,
    device: wgpu::Device,
    pipeline: Option<wgpu::RenderPipeline>,
    group: Option<wgpu::BindGroup>,
}

impl PreparedFeature for ProbeFrame {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let lent = self.shadows.current();
        if let (Some(pipeline), Ok(lent)) = (&self.pipeline, &lent) {
            self.group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cascade sampling"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&lent.depth),
                }],
            }));
        }
        self.seen.lock().unwrap().push(lent);
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        if let (Some(pipeline), Some(group)) = (&self.pipeline, &self.group) {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// Four cascades with a blend band, over a scene with one caster.
fn cascaded() -> RenderScene {
    source_with(
        &scene(&[[0, 0, 1_000]], 600_000, None),
        SceneShadowCascades::new(vec![650, 1_200, 2_000, 3_500], 100_000).unwrap(),
    )
}

fn shadowed(context: &GpuContext) -> (ShadowedScene, OffscreenTarget) {
    let renderer = super::shadowed(context, &cascaded(), settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    (renderer, target)
}

/// Exact equality: the lent values are the ones the stock uniform was encoded from.
fn bits(value: f32) -> u32 {
    value.to_bits()
}

/// The receiver uniform as the shaders read it, one word per float.
struct Words(Vec<f32>);

impl Words {
    fn of(bytes: &[u8]) -> Self {
        Self(
            bytes
                .chunks_exact(4)
                .map(|word| f32::from_le_bytes(word.try_into().unwrap()))
                .collect(),
        )
    }

    fn matrix(&self, cascade: usize) -> glam::Mat4 {
        glam::Mat4::from_cols_array(&self.0[cascade * 16..][..16].try_into().unwrap())
    }
}

/// The normal bias and the shadow strength the stock receivers apply, from the uniform's words.
fn assert_receiver_terms(lent: &FrameShadows, stock: &Words) {
    assert_eq!(bits(lent.normal_bias_meters), bits(stock.0[65]));
    assert_eq!(
        bits(lent.normal_bias_meters),
        bits(settings().normal_bias_meters)
    );
    // The receivers apply `1 - ` the reduction the uniform stores.
    assert_eq!(bits(lent.strength), bits(1.0 - stock.0[67]));
}

#[test]
fn gpu_a_game_feature_reads_the_cascades_the_stock_receivers_use() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, target) = shadowed(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let (seen, _) = Probe::install(&mut scheduler, &context, false);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "the feature prepared once");
    let lent = seen[0]
        .as_ref()
        .expect("a shadowed frame lends its cascades");
    let stock = Words::of(&uniforms(&context, &renderer)[1]);
    let declared = cascaded()
        .effects()
        .unwrap()
        .shadows()
        .extents_millimeters()
        .len();
    assert_eq!(lent.cascades.len(), declared);
    assert_eq!(
        bits(f32::from(u8::try_from(declared).unwrap())),
        bits(stock.0[68])
    );
    for (index, cascade) in lent.cascades.iter().enumerate() {
        assert_eq!(cascade.view_projection, stock.matrix(index), "{index}");
        assert_eq!(
            bits(cascade.split_distance),
            bits(stock.0[72 + index]),
            "{index}"
        );
        assert_eq!(
            bits(cascade.depth_bias),
            bits(stock.0[80 + index]),
            "{index}"
        );
    }
    assert_ne!(
        lent.cascades[0].view_projection, lent.cascades[3].view_projection,
        "the cascades differ, so the comparison above is not vacuous"
    );
    assert_eq!(bits(lent.cascades[0].split_distance), bits(0.65));
    let selection = match bits(stock.0[70]) {
        word if word == bits(0.0) => CascadeSelection::ByCoverage,
        word if word == bits(1.0) => CascadeSelection::ByCameraDistance,
        _ => CascadeSelection::Recurrence,
    };
    assert_eq!(lent.selection, selection);
    assert_eq!(bits(lent.near), bits(stock.0[71]));
    assert_eq!(
        lent.view_depth.to_array().map(f32::to_bits),
        [76, 77, 78, 79].map(|word| bits(stock.0[word]))
    );
    assert_eq!(bits(lent.blend), bits(stock.0[69]));
    assert!(lent.blend > 0.0, "the fixture blends");
    assert_eq!(bits(lent.multiplier), bits(stock.0[64]));
    assert_eq!(bits(lent.fade_out), bits(stock.0[84]));
    assert_receiver_terms(lent, &stock);
    assert_eq!(lent.resolution, settings().resolution);
    assert_eq!(lent.depth, *renderer.map.depth.receiver_view());
    assert_eq!(lent.sampler, renderer.map.sampler);
}

/// The shadow strength the receivers apply when the scene weakens its shadows, as
/// `gpu_retained_sampling_update_rekeys_caster_slope` in `sampling.rs` sets it, so the lent
/// strength is compared at a value other than full strength.
#[test]
fn gpu_a_game_feature_reads_the_weakened_shadow_strength_the_stock_receivers_apply() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let cascades = SceneShadowCascades::new(vec![650, 1_200, 2_000, 3_500], 100_000)
        .unwrap()
        .with_sampling(SceneShadowSampling {
            strength_micro: 800_000,
            ..SceneShadowSampling::default()
        })
        .unwrap();
    let source = source_with(&scene(&[[0, 0, 1_000]], 600_000, None), cascades);
    let mut renderer = super::shadowed(&context, &source, settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let (seen, _) = Probe::install(&mut scheduler, &context, false);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();

    let seen = seen.lock().unwrap();
    let lent = seen[0]
        .as_ref()
        .expect("a shadowed frame lends its cascades");
    let stock = Words::of(&uniforms(&context, &renderer)[1]);
    assert_receiver_terms(lent, &stock);
    assert!(
        (lent.strength - 0.8).abs() < 1e-6,
        "the receivers blend sunlight at 0.8: {}",
        lent.strength
    );
}

#[test]
fn gpu_a_same_frame_fitted_capture_lends_the_fitted_cascades() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = super::shadowed(&context, &cascaded(), settings()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: super::sdsm::policy(),
            }),
        )
        .unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let (seen, _) = Probe::install(&mut scheduler, &context, false);
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(renderer.last_depth_fit().is_some(), "the capture fitted");

    let seen = seen.lock().unwrap();
    let lent = seen
        .last()
        .and_then(|lent| lent.as_ref().ok())
        .expect("the suffix lends the cascades it fitted");
    let stock = Words::of(&uniforms(&context, &renderer)[1]);
    assert_eq!(lent.cascades.len(), 4);
    for (index, cascade) in lent.cascades.iter().enumerate() {
        assert_eq!(cascade.view_projection, stock.matrix(index), "{index}");
        assert_eq!(
            bits(cascade.split_distance),
            bits(stock.0[72 + index]),
            "{index}"
        );
    }
    assert_ne!(
        bits(lent.cascades[0].split_distance),
        bits(0.65),
        "the splits are the fitted ones, not the declared ones"
    );
}

#[test]
fn gpu_the_handle_lends_nothing_between_frames_and_never_an_earlier_frames_cascades() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, target) = shadowed(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let (seen, handle) = Probe::install(&mut scheduler, &context, false);
    assert_eq!(handle.current().unwrap_err(), FrameInputError::OutsideFrame);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(seen.lock().unwrap()[0].is_ok());
    assert_eq!(
        handle.current().unwrap_err(),
        FrameInputError::OutsideFrame,
        "the frame is over"
    );

    // A transaction that publishes nothing is not the frame the cascades were published in.
    let mut verdict = None;
    context
        .submission()
        .prepare(context.device(), |_, _| {
            verdict = Some(handle.current().err());
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap()
        .cancel();
    assert_eq!(verdict, Some(Some(FrameInputError::NotPrepared)));
    assert_eq!(handle.current().unwrap_err(), FrameInputError::OutsideFrame);

    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert!(
        seen.lock().unwrap()[1].is_ok(),
        "the next frame lends its own"
    );
}

#[test]
fn gpu_a_frame_with_shadows_off_lends_nothing_and_still_renders() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, target) = shadowed(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let (seen, _) = Probe::install(&mut scheduler, &context, false);
    renderer.set_shadows_enabled(false);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(matches!(
        seen.lock().unwrap().as_slice(),
        [Err(FrameInputError::NotPrepared)]
    ));
    renderer.set_shadows_enabled(true);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(seen.lock().unwrap()[1].is_ok());
}

#[test]
fn gpu_a_game_pass_samples_the_cascades_the_stock_casters_wrote_this_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, target) = shadowed(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let (seen, _) = Probe::install(&mut scheduler, &context, true);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(seen.lock().unwrap()[0].is_ok());

    let stock = &depth_layers(&context, &renderer)[0];
    let side = usize::try_from(settings().resolution).unwrap();
    let expected: Vec<f32> = (0..4_usize)
        .flat_map(|row| (0..4_usize).map(move |column| (row, column)))
        .map(|(row, column)| {
            let stride = usize::try_from(STRIDE).unwrap();
            stock[row * stride * side + column * stride]
        })
        .collect();
    assert!(
        expected.iter().all(|&depth| 0.0 < depth && depth < 1.0)
            && expected
                .windows(2)
                .any(|pair| bits(pair[0]) != bits(pair[1])),
        "the samples are written depths that differ across the layer: {expected:?}"
    );
    let texture = scheduler.game_targets().texture(SAMPLED).unwrap();
    assert_eq!(read_f32(&context, &texture), expected);
}

fn read_f32(context: &GpuContext, texture: &wgpu::Texture) -> Vec<f32> {
    let device = context.device();
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sampled cascades readback"),
        size: 256 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    context
        .submission()
        .submit_commands([encoder.finish()])
        .unwrap();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, |mapped| mapped.unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let mapped = staging.slice(..).get_mapped_range().unwrap();
    (0..4_usize)
        .flat_map(|row| {
            mapped[row * 256..][..16]
                .chunks_exact(4)
                .map(|texel| f32::from_le_bytes(texel.try_into().unwrap()))
                .collect::<Vec<_>>()
        })
        .collect()
}
