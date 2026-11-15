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
}
impl Ops {
    fn new(device: &wgpu::Device, name: &'static str, log: &Log) -> Self {
        Self {
            name,
            texture: color_texture(device, name, SIZE, FORMAT),
            log: log.clone(),
            saw_timing: Cell::new(None),
            fails: false,
        }
    }
}
impl FrameOperations for Ops {
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
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
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
