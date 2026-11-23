//! A frame plan carries operations groups, each positioned before an entry of the authored pass
//! list (or after every pass), and the executor encodes each at that position.
use std::{cell::Cell, cell::RefCell, rc::Rc};
use worldrunner_render_gpu::{device::DeviceRequirements, test_support::TestGpu};
use worldrunner_render_post::*;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SIZE: [u32; 2] = [8, 8];

type Log = Rc<RefCell<Vec<&'static str>>>;

/// Fills its pass and logs when the executor asks it to draw, which is the order the pass's
/// commands are recorded.
struct Fill {
    name: &'static str,
    pipeline: wgpu::RenderPipeline,
    log: Log,
}
impl Fill {
    fn new(device: &wgpu::Device, name: &'static str, log: &Log) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fill"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }
"
                .into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = fullscreen_pipeline(
            device,
            FullscreenDescriptor {
                label: None,
                layout: &layout,
                module: &module,
                vertex_entry: "vs",
                fragment_entry: "fs",
                format: FORMAT,
                constants: &[],
            },
        );
        Self {
            name,
            pipeline,
            log: log.clone(),
        }
    }
    fn declaration(entry: PassEntry) -> FeatureDeclaration {
        let mut feature = FeaturePass::nth(entry.slot(), entry.occurrence());
        feature.color_format = Some(FORMAT);
        FeatureDeclaration {
            passes: vec![feature],
            effects: Vec::new(),
        }
    }
}
impl PreparedFeature for Fill {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.log.borrow_mut().push(self.name);
        pass.set_pipeline(&self.pipeline);
        pass.draw(0..3, 0..1);
    }
}

/// Operations of its own: clears a texture of its own to red in the frame's encoder.
struct Ops {
    name: &'static str,
    texture: wgpu::Texture,
    log: Log,
    saw_timing: Cell<Option<bool>>,
    fails: bool,
    color: wgpu::Color,
    outputs: Vec<OperationsOutput>,
}
impl Ops {
    fn new(device: &wgpu::Device, name: &'static str, log: &Log) -> Self {
        Self {
            name,
            texture: color_texture(device, name, SIZE, FORMAT),
            log: log.clone(),
            saw_timing: Cell::new(None),
            fails: false,
            color: wgpu::Color::RED,
            outputs: Vec::new(),
        }
    }
}
impl FrameOperations for Ops {
    fn produces(&self) -> &[OperationsOutput] {
        &self.outputs
    }
    fn encode_operations(
        &self,
        timing: Option<&TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        self.log.borrow_mut().push(self.name);
        self.saw_timing.set(Some(timing.is_some()));
        if self.fails {
            return Err(FrameError::FeaturePreparation {
                feature: "test operations",
                reason: self.name.to_owned(),
            });
        }
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        }));
        Ok(())
    }
}

fn clearing(entry: PassEntry, resource: ResourceRef) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(entry.slot(), entry.occurrence());
    pass.color = Some(AttachmentDeclaration {
        resource,
        format: FORMAT,
        size: SIZE,
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    pass
}

/// Two `Opaque` entries, each clearing its own target, and the features that draw in them.
struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    chain: PostChain<CopyOutput>,
    entries: [PassEntry; 2],
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
    log: Log,
    first: Fill,
    second: Fill,
}
impl Rig {
    fn new() -> Self {
        let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
        let (device, queue) = (gpu.device, gpu.queue);
        let chain = PostChain::new(
            &device,
            FrameConfig::new(SIZE, FORMAT, 1),
            Resources::default(),
            CopyOutput::new(&device, FORMAT),
            SceneDepth::Independent,
        )
        .unwrap();
        let entries = [
            PassEntry::new(PassSlot::Opaque),
            PassEntry::nth(PassSlot::Opaque, 1),
        ];
        let resources = [ResourceRef::new("a target"), ResourceRef::new("b target")];
        let views: Vec<_> = resources
            .iter()
            .map(|resource| {
                let texture = color_texture(&device, resource.name, SIZE, FORMAT);
                AttachmentView::new(*resource, &texture).unwrap()
            })
            .collect();
        let passes = entries
            .iter()
            .zip(resources)
            .map(|(entry, resource)| clearing(*entry, resource))
            .collect();
        let log = Log::default();
        Self {
            first: Fill::new(&device, "a", &log),
            second: Fill::new(&device, "b", &log),
            device,
            queue,
            chain,
            entries,
            passes,
            views,
            log,
        }
    }
    fn ops(&self, name: &'static str) -> Ops {
        Ops::new(&self.device, name, &self.log)
    }
    fn encoder(&self) -> worldrunner_render_gpu::submission::RecordingFrame {
        worldrunner_render_gpu::submission::SubmissionAuthority::new(self.queue.clone())
            .begin(&self.device)
            .unwrap()
    }
    /// Encodes both entries with `groups`, returning what the executor said.
    fn run(&mut self, groups: &[OperationsGroup<'_>]) -> Result<(), FrameError> {
        let mut encoder = self.encoder();
        let [first, second] = self.entries;
        self.chain.encode_geometry(
            &self.device,
            &mut encoder,
            &FramePlan {
                passes: &self.passes,
                views: &self.views,
                requested: &[],
                operations: groups,
            },
            &mut [
                BorrowedFeature {
                    declaration: Fill::declaration(first),
                    prepared: &mut self.first,
                },
                BorrowedFeature {
                    declaration: Fill::declaration(second),
                    prepared: &mut self.second,
                },
            ],
        )
    }
    fn logged(&self) -> Vec<&'static str> {
        self.log.borrow().clone()
    }
}

fn before(entry: PassEntry, work: &dyn FrameOperations) -> OperationsGroup<'_> {
    OperationsGroup {
        before: Some(entry),
        work,
    }
}
fn after_all(work: &dyn FrameOperations) -> OperationsGroup<'_> {
    OperationsGroup { before: None, work }
}

#[test]
fn a_group_before_the_first_entry_runs_first_and_its_commands_land_in_the_frame() {
    let mut rig = Rig::new();
    let scope = rig.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let ops = rig.ops("ops");
    let mut encoder = rig.encoder();
    let [first, second] = rig.entries;
    rig.chain
        .encode_geometry(
            &rig.device,
            &mut encoder,
            &FramePlan {
                passes: &rig.passes,
                views: &rig.views,
                requested: &[],
                operations: &[before(first, &ops)],
            },
            &mut [
                BorrowedFeature {
                    declaration: Fill::declaration(first),
                    prepared: &mut rig.first,
                },
                BorrowedFeature {
                    declaration: Fill::declaration(second),
                    prepared: &mut rig.second,
                },
            ],
        )
        .unwrap();
    let config = FrameConfig::new(SIZE, FORMAT, 1);
    let mut readback = Readback::new(&rig.device, config, 4096).unwrap();
    readback.encode(&mut encoder, &ops.texture).unwrap();
    let submission = encoder.finish().submit().unwrap();
    assert_eq!(rig.logged(), ["ops", "a", "b"]);
    assert_eq!(
        ops.saw_timing.get(),
        Some(true),
        "the executor hands the group the frame's timer"
    );
    let pixels = readback
        .read_encoded(&rig.device, Some(submission))
        .unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255])
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn a_group_before_a_later_entry_runs_between_the_two_passes() {
    let mut rig = Rig::new();
    let ops = rig.ops("ops");
    let second = rig.entries[1];
    rig.run(&[before(second, &ops)]).unwrap();
    assert_eq!(rig.logged(), ["a", "ops", "b"]);
}

#[test]
fn a_group_with_no_entry_runs_after_every_pass() {
    let mut rig = Rig::new();
    let ops = rig.ops("ops");
    rig.run(&[after_all(&ops)]).unwrap();
    assert_eq!(rig.logged(), ["a", "b", "ops"]);
}

#[test]
fn groups_run_in_position_order_and_groups_at_one_position_keep_plan_order() {
    let mut rig = Rig::new();
    let [first, second] = rig.entries;
    let (tail, later, head, also_later) = (
        rig.ops("tail"),
        rig.ops("later"),
        rig.ops("head"),
        rig.ops("also later"),
    );
    rig.run(&[
        after_all(&tail),
        before(second, &later),
        before(first, &head),
        before(second, &also_later),
    ])
    .unwrap();
    assert_eq!(
        rig.logged(),
        ["head", "a", "later", "also later", "b", "tail"]
    );
}

#[test]
fn a_group_before_an_entry_the_plan_does_not_author_is_refused_before_anything_runs() {
    let mut rig = Rig::new();
    let ops = rig.ops("ops");
    let unauthored = PassEntry::nth(PassSlot::Opaque, 7);
    assert_eq!(
        rig.run(&[before(unauthored, &ops)]),
        Err(FrameError::MissingPass(unauthored))
    );
    assert!(rig.logged().is_empty(), "no group encoded, no feature drew");
}

#[test]
fn a_plan_with_no_groups_encodes_only_its_passes() {
    let mut rig = Rig::new();
    rig.run(&[]).unwrap();
    assert_eq!(rig.logged(), ["a", "b"]);
}

#[test]
fn a_group_that_fails_abandons_the_frame_at_its_position() {
    let mut rig = Rig::new();
    let mut ops = rig.ops("ops");
    ops.fails = true;
    let second = rig.entries[1];
    assert_eq!(
        rig.run(&[before(second, &ops)]),
        Err(FrameError::FeaturePreparation {
            feature: "test operations",
            reason: "ops".to_owned(),
        })
    );
    assert_eq!(rig.logged(), ["a", "ops"], "the later pass never encoded");
}

#[test]
fn a_group_in_the_prefix_plan_runs_with_the_prefix_and_is_not_repeated_after_it() {
    let mut rig = Rig::new();
    let ops = rig.ops("ops");
    let [first, second] = rig.entries;
    let mut encoder = rig.encoder();
    rig.chain
        .encode_geometry_prefix(
            &rig.device,
            &mut encoder,
            &FramePlan {
                passes: &rig.passes[..1],
                views: &rig.views[..1],
                requested: &[],
                operations: &[before(first, &ops)],
            },
            &mut [BorrowedFeature {
                declaration: Fill::declaration(first),
                prepared: &mut rig.first,
            }],
        )
        .unwrap();
    rig.chain
        .encode_geometry(
            &rig.device,
            &mut encoder,
            &FramePlan {
                passes: &rig.passes,
                views: &rig.views,
                requested: &[],
                operations: &[],
            },
            &mut [
                BorrowedFeature {
                    declaration: Fill::declaration(first),
                    prepared: &mut rig.first,
                },
                BorrowedFeature {
                    declaration: Fill::declaration(second),
                    prepared: &mut rig.second,
                },
            ],
        )
        .unwrap();
    assert_eq!(rig.logged(), ["ops", "a", "b"]);
}

#[test]
fn a_group_positioned_before_a_prefix_entry_runs_before_the_first_pass_the_suffix_encodes() {
    let mut rig = Rig::new();
    let ops = rig.ops("ops");
    let [first, second] = rig.entries;
    let mut encoder = rig.encoder();
    rig.chain
        .encode_geometry_prefix(
            &rig.device,
            &mut encoder,
            &FramePlan {
                passes: &rig.passes[..1],
                views: &rig.views[..1],
                requested: &[],
                operations: &[],
            },
            &mut [BorrowedFeature {
                declaration: Fill::declaration(first),
                prepared: &mut rig.first,
            }],
        )
        .unwrap();
    // The prefix encoded `first`, so the group cannot precede it; it runs before `second`,
    // the first pass the full plan still has to encode.
    rig.chain
        .encode_geometry(
            &rig.device,
            &mut encoder,
            &FramePlan {
                passes: &rig.passes,
                views: &rig.views,
                requested: &[],
                operations: &[before(first, &ops)],
            },
            &mut [
                BorrowedFeature {
                    declaration: Fill::declaration(first),
                    prepared: &mut rig.first,
                },
                BorrowedFeature {
                    declaration: Fill::declaration(second),
                    prepared: &mut rig.second,
                },
            ],
        )
        .unwrap();
    assert_eq!(rig.logged(), ["a", "ops", "b"]);
}

/// Copies the texel it reads from `source` under each of its own, so the pixels of the pass it
/// draws in show what the sampled texture holds.
struct Copies {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    source: ResourceRef,
    bind_group: Option<wgpu::BindGroup>,
}
impl Copies {
    fn new(device: &wgpu::Device, source: ResourceRef) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("copies"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(at.xy), 0);
}
"
                .into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = fullscreen_pipeline(
            device,
            FullscreenDescriptor {
                label: None,
                layout: &pipeline_layout,
                module: &module,
                vertex_entry: "vs",
                fragment_entry: "fs",
                format: FORMAT,
                constants: &[],
            },
        );
        Self {
            pipeline,
            layout,
            source,
            bind_group: None,
        }
    }
    /// A feature of `entry` that declares its read of `source`.
    fn declaration(&self, entry: PassEntry) -> FeatureDeclaration {
        let mut declaration = Fill::declaration(entry);
        declaration.passes[0].reads = vec![self.source];
        declaration
    }
}
impl PreparedFeature for Copies {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let view = context.resource(self.source)?.sampled_view();
        self.bind_group = Some(
            context
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    }],
                }),
        );
        Ok(())
    }
    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, self.bind_group.as_ref(), &[]);
        pass.draw(0..3, 0..1);
    }
}

/// A frame whose second entry reads `ops`, the texture the operations of `ops` clear to red.
struct Reads {
    rig: Rig,
    written: wgpu::Texture,
    copies: Copies,
}
impl Reads {
    /// The frame and the operations that write the texture it reads.
    fn new() -> (Self, Ops) {
        let rig = Rig::new();
        let mut ops = rig.ops("ops");
        let resource = ResourceRef::new("ops");
        ops.outputs = vec![OperationsOutput::new(resource, FORMAT, SIZE)];
        let copies = Copies::new(&rig.device, resource);
        let written = ops.texture.clone();
        (
            Self {
                rig,
                written,
                copies,
            },
            ops,
        )
    }
    /// The views of the plan, with the one the host hands in for the written texture when
    /// `hosted`.
    fn views(&self, hosted: bool) -> Vec<AttachmentView> {
        let mut views = self.rig.views.clone();
        if hosted {
            views.push(
                AttachmentView::new(ResourceRef::new("ops"), &self.written)
                    .unwrap()
                    .with_retained_contents(),
            );
        }
        views
    }
    /// Encodes both entries, the second reading `ops`, and returns the second entry's target.
    fn run(
        &mut self,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        groups: &[OperationsGroup<'_>],
    ) -> Result<Vec<u8>, FrameError> {
        let mut encoder = self.rig.encoder();
        let [first, second] = self.rig.entries;
        self.rig.chain.encode_geometry(
            &self.rig.device,
            &mut encoder,
            &FramePlan {
                passes,
                views,
                requested: &[],
                operations: groups,
            },
            &mut [
                BorrowedFeature {
                    declaration: Fill::declaration(first),
                    prepared: &mut self.rig.first,
                },
                BorrowedFeature {
                    declaration: self.copies.declaration(second),
                    prepared: &mut self.copies,
                },
            ],
        )?;
        let config = FrameConfig::new(SIZE, FORMAT, 1);
        let mut readback = Readback::new(&self.rig.device, config, 4096).unwrap();
        readback
            .encode(&mut encoder, self.rig.views[1].texture())
            .unwrap();
        let submission = encoder.finish().submit().unwrap();
        Ok(readback
            .read_encoded(&self.rig.device, Some(submission))
            .unwrap())
    }
}

#[test]
fn a_pass_after_a_group_samples_the_texture_the_group_wrote() {
    let (mut frame, ops) = Reads::new();
    let scope = frame
        .rig
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let second = frame.rig.entries[1];
    let (passes, views) = (frame.rig.passes.clone(), frame.views(true));
    let pixels = frame.run(&passes, &views, &[before(second, &ops)]).unwrap();
    assert_eq!(frame.rig.logged(), ["a", "ops"]);
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]),
        "the second entry's target holds the red the group cleared"
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn a_read_of_the_groups_texture_with_no_group_or_a_later_group_has_no_producer() {
    let (mut frame, ops) = Reads::new();
    let second = frame.rig.entries[1];
    let missing = Err(FrameError::MissingProducer(ResourceRef::new("ops")));
    let passes = frame.rig.passes.clone();
    assert_eq!(
        frame.run(&passes, &frame.views(false), &[]),
        missing,
        "no group and no texture"
    );
    assert_eq!(
        frame.run(&passes, &frame.views(true), &[after_all(&ops)]),
        missing,
        "the group runs after the reader"
    );
    assert_eq!(
        frame
            .run(&passes, &frame.views(true), &[before(second, &ops)])
            .map(drop),
        Ok(()),
    );
    assert_eq!(
        frame.rig.logged(),
        ["a", "ops"],
        "the refused plans encoded nothing"
    );
}

#[test]
fn a_geometry_pass_that_writes_the_groups_texture_is_refused_by_name() {
    let (mut frame, ops) = Reads::new();
    let second = frame.rig.entries[1];
    let mut passes = frame.rig.passes.clone();
    passes[1].color.as_mut().unwrap().resource = ResourceRef::new("ops");
    let views = vec![frame.rig.views[0].clone(), frame.views(true).remove(2)];
    assert_eq!(
        frame.run(&passes, &views, &[before(second, &ops)]),
        Err(FrameError::OffscreenWrite {
            pass: second,
            resource: ResourceRef::new("ops"),
        })
    );
    assert!(frame.rig.logged().is_empty(), "nothing encoded");
}

#[path = "operations_groups/retry.rs"]
mod retry;
