//! The geometry executor runs the authored list, not the `PassSlot` enum order, and a role
//! may appear twice. Each feature draws only in the entry it names.
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_gpu::{device::DeviceRequirements, test_support::TestGpu};
use worldrunner_render_post::*;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SIZE: [u32; 2] = [8, 8];

/// Fills the pass it draws in with one colour and records which role the executor named.
struct Fill {
    name: &'static str,
    pipeline: wgpu::RenderPipeline,
    draws: Rc<RefCell<Vec<(&'static str, PassSlot)>>>,
}
impl Fill {
    fn new(
        device: &wgpu::Device,
        name: &'static str,
        rgb: [f64; 3],
        draws: &Rc<RefCell<Vec<(&'static str, PassSlot)>>>,
    ) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fill"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
override R: f32 = 0.0;
override G: f32 = 0.0;
override B: f32 = 0.0;
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(R, G, B, 1.0); }
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
                constants: &[("R", rgb[0]), ("G", rgb[1]), ("B", rgb[2])],
            },
        );
        Self {
            name,
            pipeline,
            draws: draws.clone(),
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
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.draws.borrow_mut().push((self.name, context.slot));
        pass.set_pipeline(&self.pipeline);
        pass.draw(0..3, 0..1);
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

#[test]
fn passes_run_in_authored_order_and_a_role_can_run_twice() {
    let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
    let (device, queue) = (gpu.device, gpu.queue);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let config = FrameConfig::new(SIZE, FORMAT, 1);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::new(&device, FORMAT),
        SceneDepth::Independent,
    )
    .unwrap();
    // The enum orders DepthNormalPrepass before Opaque. The author lists Opaque twice and
    // puts the prepass between the two, so only the authored list can produce this order.
    let first = PassEntry::new(PassSlot::Opaque);
    let middle = PassEntry::new(PassSlot::DepthNormalPrepass);
    let last = PassEntry::nth(PassSlot::Opaque, 1);
    let entries = [first, middle, last];
    let resources = [
        ResourceRef::new("first target"),
        ResourceRef::new("middle target"),
        ResourceRef::new("last target"),
    ];
    let textures: Vec<_> = resources
        .iter()
        .map(|resource| color_texture(&device, resource.name, SIZE, FORMAT))
        .collect();
    let views: Vec<_> = resources
        .iter()
        .zip(&textures)
        .map(|(resource, texture)| AttachmentView::new(*resource, texture).unwrap())
        .collect();
    let passes: Vec<_> = entries
        .iter()
        .zip(resources)
        .map(|(entry, resource)| clearing(*entry, resource))
        .collect();

    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut red = Fill::new(&device, "first", [1.0, 0.0, 0.0], &draws);
    let mut blue = Fill::new(&device, "middle", [0.0, 0.0, 1.0], &draws);
    let mut green = Fill::new(&device, "last", [0.0, 1.0, 0.0], &draws);
    let mut features = [
        BorrowedFeature {
            declaration: Fill::declaration(first),
            prepared: &mut red,
        },
        BorrowedFeature {
            declaration: Fill::declaration(middle),
            prepared: &mut blue,
        },
        BorrowedFeature {
            declaration: Fill::declaration(last),
            prepared: &mut green,
        },
    ];

    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(&device)
        .unwrap();
    chain
        .encode_geometry(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
                operations: &[],
            },
            &mut features,
        )
        .unwrap();
    let mut readbacks: Vec<_> = textures
        .iter()
        .map(|texture| {
            let mut readback = Readback::new(&device, config, 4096).unwrap();
            readback.encode(&mut encoder, texture).unwrap();
            readback
        })
        .collect();
    let submission = encoder.finish().submit().unwrap();

    assert_eq!(
        *draws.borrow(),
        [
            ("first", PassSlot::Opaque),
            ("middle", PassSlot::DepthNormalPrepass),
            ("last", PassSlot::Opaque),
        ],
        "each feature drew once, in the authored position of the entry it names"
    );
    for (readback, expected) in
        readbacks
            .iter_mut()
            .zip([[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]])
    {
        let pixels = readback
            .read_encoded(&device, Some(submission.clone()))
            .unwrap();
        assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
    }
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn a_repeated_entry_is_refused_before_any_feature_prepares_or_draws() {
    let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
    let (device, queue) = (gpu.device, gpu.queue);
    let config = FrameConfig::new(SIZE, FORMAT, 1);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::new(&device, FORMAT),
        SceneDepth::Independent,
    )
    .unwrap();
    let entry = PassEntry::nth(PassSlot::Opaque, 1);
    let resources = [ResourceRef::new("a"), ResourceRef::new("b")];
    let textures: Vec<_> = resources
        .iter()
        .map(|resource| color_texture(&device, resource.name, SIZE, FORMAT))
        .collect();
    let views: Vec<_> = resources
        .iter()
        .zip(&textures)
        .map(|(resource, texture)| AttachmentView::new(*resource, texture).unwrap())
        .collect();
    let passes = [clearing(entry, resources[0]), clearing(entry, resources[1])];
    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut fill = Fill::new(&device, "never", [1.0, 0.0, 0.0], &draws);
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(&device)
        .unwrap();
    assert_eq!(
        chain.encode_geometry(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
                operations: &[],
            },
            &mut [BorrowedFeature {
                declaration: Fill::declaration(entry),
                prepared: &mut fill,
            }],
        ),
        Err(FrameError::DuplicatePass(entry))
    );
    assert!(draws.borrow().is_empty());
}

#[test]
fn a_prefix_that_encoded_one_entry_of_a_role_does_not_skip_the_next_entry_of_that_role() {
    let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
    let (device, queue) = (gpu.device, gpu.queue);
    let config = FrameConfig::new(SIZE, FORMAT, 1);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::new(&device, FORMAT),
        SceneDepth::Independent,
    )
    .unwrap();
    let first = PassEntry::new(PassSlot::Opaque);
    let second = PassEntry::nth(PassSlot::Opaque, 1);
    let resources = [ResourceRef::new("a"), ResourceRef::new("b")];
    let textures: Vec<_> = resources
        .iter()
        .map(|resource| color_texture(&device, resource.name, SIZE, FORMAT))
        .collect();
    let views: Vec<_> = resources
        .iter()
        .zip(&textures)
        .map(|(resource, texture)| AttachmentView::new(*resource, texture).unwrap())
        .collect();
    let passes = [
        clearing(first, resources[0]),
        clearing(second, resources[1]),
    ];
    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut in_prefix = Fill::new(&device, "prefix", [1.0, 0.0, 0.0], &draws);
    let mut skipped = Fill::new(&device, "skipped", [1.0, 0.0, 0.0], &draws);
    let mut in_suffix = Fill::new(&device, "suffix", [0.0, 1.0, 0.0], &draws);
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(&device)
        .unwrap();
    chain
        .encode_geometry_prefix(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &passes[..1],
                views: &views[..1],
                requested: &[],
                operations: &[],
            },
            &mut [BorrowedFeature {
                declaration: Fill::declaration(first),
                prepared: &mut in_prefix,
            }],
        )
        .unwrap();
    chain
        .encode_geometry(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
                operations: &[],
            },
            &mut [
                BorrowedFeature {
                    declaration: Fill::declaration(first),
                    prepared: &mut skipped,
                },
                BorrowedFeature {
                    declaration: Fill::declaration(second),
                    prepared: &mut in_suffix,
                },
            ],
        )
        .unwrap();
    assert_eq!(
        *draws.borrow(),
        [("prefix", PassSlot::Opaque), ("suffix", PassSlot::Opaque)],
        "the prefix completed entry 0 of Opaque only"
    );
}

/// Declared in two entries of one role; records the context each draw receives.
struct Spy {
    seen: Rc<RefCell<Vec<(PassSlot, PassEntry)>>>,
}
impl PreparedFeature for Spy {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, context: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        self.seen.borrow_mut().push((context.slot, context.entry));
    }
}

#[test]
fn a_feature_declared_in_two_entries_of_one_role_tells_them_apart() {
    let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
    let (device, queue) = (gpu.device, gpu.queue);
    let config = FrameConfig::new(SIZE, FORMAT, 1);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::new(&device, FORMAT),
        SceneDepth::Independent,
    )
    .unwrap();
    let first = PassEntry::new(PassSlot::Opaque);
    let second = PassEntry::nth(PassSlot::Opaque, 1);
    let resources = [ResourceRef::new("a"), ResourceRef::new("b")];
    let textures: Vec<_> = resources
        .iter()
        .map(|resource| color_texture(&device, resource.name, SIZE, FORMAT))
        .collect();
    let views: Vec<_> = resources
        .iter()
        .zip(&textures)
        .map(|(resource, texture)| AttachmentView::new(*resource, texture).unwrap())
        .collect();
    let passes = [
        clearing(first, resources[0]),
        clearing(second, resources[1]),
    ];
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut spy = Spy { seen: seen.clone() };
    let declaration = FeatureDeclaration {
        passes: [first, second]
            .map(|entry| {
                let mut pass = FeaturePass::nth(entry.slot(), entry.occurrence());
                pass.color_format = Some(FORMAT);
                pass
            })
            .to_vec(),
        effects: Vec::new(),
    };
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(&device)
        .unwrap();
    chain
        .encode_geometry(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
                operations: &[],
            },
            &mut [BorrowedFeature {
                declaration,
                prepared: &mut spy,
            }],
        )
        .unwrap();
    assert_eq!(
        *seen.borrow(),
        [(PassSlot::Opaque, first), (PassSlot::Opaque, second)],
        "one draw per entry; the role is shared and the entry differs"
    );
    assert_eq!(second.occurrence(), 1);
    assert_eq!(DrawContext::new(second).entry, second);
    assert_eq!(DrawContext::new(second).slot, PassSlot::Opaque);
}
