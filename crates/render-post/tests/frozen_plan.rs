//! A frozen plan is refused by name before any feature prepares when it breaks an order the
//! stock passes need, and encodes as given when the order is merely not the stock one.
use std::{cell::Cell, cell::RefCell, rc::Rc};
use worldrunner_render_gpu::{device::DeviceRequirements, test_support::TestGpu};
use worldrunner_render_post::*;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SIZE: [u32; 2] = [8, 8];

/// Counts its preparations and records the entry of every pass it is asked to draw in.
#[derive(Default)]
struct Counting {
    prepared: Rc<Cell<u32>>,
    draws: Rc<RefCell<Vec<PassEntry>>>,
}
impl PreparedFeature for Counting {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.prepared.set(self.prepared.get() + 1);
        Ok(())
    }
    fn draw(&self, context: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        self.draws.borrow_mut().push(context.entry);
    }
}

struct Rig {
    gpu: TestGpu,
    chain: PostChain<CopyOutput>,
    textures: Vec<wgpu::Texture>,
}

/// What encoding a frozen plan did.
struct Outcome {
    result: Result<(), FrameError>,
    prepared: u32,
    draws: Vec<PassEntry>,
}

impl Rig {
    fn new() -> Self {
        let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
        let chain = PostChain::new(
            &gpu.device,
            FrameConfig::new(SIZE, FORMAT, 1),
            Resources::default(),
            CopyOutput::new(&gpu.device, FORMAT),
        )
        .unwrap();
        Self {
            gpu,
            chain,
            textures: Vec::new(),
        }
    }

    fn view(&mut self, name: &'static str, format: wgpu::TextureFormat) -> AttachmentView {
        let texture = color_texture(&self.gpu.device, name, SIZE, format);
        let view = AttachmentView::new(ResourceRef::new(name), &texture).unwrap();
        self.textures.push(texture);
        view
    }

    /// A colour target and the attachment that clears it.
    fn colour(&mut self, name: &'static str) -> (AttachmentDeclaration, AttachmentView) {
        let view = self.view(name, FORMAT);
        (clear(view.resource(), FORMAT), view)
    }

    /// A depth target and the attachment that clears it.
    fn depth(&mut self, name: &'static str) -> (AttachmentDeclaration, AttachmentView) {
        let view = self.view(name, DEPTH);
        let mut depth = clear(view.resource(), DEPTH);
        depth.load = AttachmentLoad::ClearDepth(1.0);
        (depth, view)
    }

    /// Encodes the plan, with one counting feature that declares `declaration`.
    fn encode(
        &mut self,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        declaration: FeatureDeclaration,
    ) -> Outcome {
        let counting = Counting::default();
        let (prepared, draws) = (counting.prepared.clone(), counting.draws.clone());
        let mut feature = counting;
        let mut features = [BorrowedFeature {
            declaration,
            prepared: &mut feature,
        }];
        let mut encoder =
            worldrunner_render_gpu::submission::SubmissionAuthority::new(self.gpu.queue.clone())
                .begin(&self.gpu.device)
                .unwrap();
        let result = self.chain.encode_frozen(
            &self.gpu.device,
            &mut encoder,
            &FramePlan {
                passes,
                views,
                requested: &[],
                operations: &[],
            },
            &mut features,
        );
        if result.is_ok() {
            encoder.finish().submit().unwrap();
        }
        Outcome {
            result,
            prepared: prepared.get(),
            draws: draws.take(),
        }
    }
}

fn clear(resource: ResourceRef, format: wgpu::TextureFormat) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource,
        format,
        size: SIZE,
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    }
}

fn pass(entry: PassEntry, color: &AttachmentDeclaration) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(entry.slot(), entry.occurrence());
    pass.color = Some(color.clone());
    pass
}

fn drawing(entries: &[(PassEntry, &[ResourceRef])]) -> FeatureDeclaration {
    FeatureDeclaration {
        passes: entries
            .iter()
            .map(|(entry, reads)| {
                let mut pass = FeaturePass::nth(entry.slot(), entry.occurrence());
                pass.reads = reads.to_vec();
                pass
            })
            .collect(),
        effects: Vec::new(),
    }
}

fn entry(slot: PassSlot) -> PassEntry {
    PassEntry::new(slot)
}

fn refused(outcome: &Outcome, error: FrameError) {
    assert_eq!(outcome.result, Err(error));
    assert_eq!(outcome.prepared, 0, "a feature prepared before the refusal");
    assert!(
        outcome.draws.is_empty(),
        "a feature drew before the refusal"
    );
}

#[test]
fn a_read_before_its_writer_is_refused_before_any_feature_prepares() {
    let mut rig = Rig::new();
    let (first, first_view) = rig.colour("first target");
    let (second, second_view) = rig.colour("second target");
    let (reader, writer) = (entry(PassSlot::Opaque), entry(PassSlot::TransparentWater));
    let passes = [pass(reader, &first), pass(writer, &second)];
    let views = [first_view, second_view];
    // The first entry reads what only the second writes.
    let declaration = drawing(&[(reader, &[second.resource]), (writer, &[])]);
    let outcome = rig.encode(&passes, &views, declaration);
    refused(&outcome, FrameError::MissingProducer(second.resource));
}

#[test]
fn a_read_of_a_retained_earlier_frame_view_is_admitted() {
    let mut rig = Rig::new();
    let (first, first_view) = rig.colour("first target");
    let (history, history_view) = rig.colour("history");
    let reader = entry(PassSlot::Opaque);
    let views = [first_view, history_view.with_retained_contents()];
    let declaration = drawing(&[(reader, &[history.resource])]);
    let outcome = rig.encode(&[pass(reader, &first)], &views, declaration);
    outcome.result.unwrap();
    assert_eq!((outcome.prepared, outcome.draws), (1, vec![reader]));
}

#[test]
fn an_entry_that_differs_from_its_feature_declaration_is_refused_before_any_feature_prepares() {
    let mut rig = Rig::new();
    let (color, view) = rig.colour("target");
    let opaque = entry(PassSlot::Opaque);
    let mut declaration = drawing(&[(opaque, &[])]);
    declaration.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
    let outcome = rig.encode(&[pass(opaque, &color)], &[view], declaration);
    refused(&outcome, FrameError::AttachmentContract(opaque));
}

#[test]
fn two_writers_of_one_target_in_one_pass_are_refused_before_any_feature_prepares() {
    let mut rig = Rig::new();
    let (color, view) = rig.colour("target");
    let opaque = entry(PassSlot::Opaque);
    let mut twice = pass(opaque, &color);
    twice.additional_colors.push(color.clone());
    let outcome = rig.encode(&[twice], &[view], drawing(&[(opaque, &[])]));
    refused(&outcome, FrameError::AttachmentHazard(color.resource));
}

/// Every listed role needs the roles before it; each plan is otherwise valid.
fn ordered(rig: &mut Rig, slots: &[PassSlot]) -> (Vec<PassDeclaration>, Vec<AttachmentView>) {
    let names = ["a", "b", "c", "d", "e", "f"];
    let (mut passes, mut views) = (Vec::new(), Vec::new());
    for (slot, name) in slots.iter().zip(names) {
        let (color, view) = rig.colour(name);
        passes.push(pass(entry(*slot), &color));
        views.push(view);
    }
    (passes, views)
}

#[test]
fn the_stock_role_orders_are_refused_before_any_feature_prepares() {
    use PassSlot::*;
    // (the listed roles, the role listed too early, the role it must follow)
    let cases = [
        (
            vec![OpaqueLighting, OpaqueGbuffer],
            OpaqueLighting,
            OpaqueGbuffer,
        ),
        (vec![Opaque, OpaqueLighting], Opaque, OpaqueLighting),
        (vec![ReflectionTrace, Opaque], ReflectionTrace, Opaque),
        (
            vec![ScreenSpaceAfterTransparent, OpaqueLighting, Opaque],
            ScreenSpaceAfterTransparent,
            OpaqueLighting,
        ),
    ];
    for (slots, early, needs) in cases {
        let mut rig = Rig::new();
        let (passes, views) = ordered(&mut rig, &slots);
        let declaration = drawing(
            &slots
                .iter()
                .map(|slot| (entry(*slot), &[][..]))
                .collect::<Vec<_>>(),
        );
        let outcome = rig.encode(&passes, &views, declaration);
        refused(
            &outcome,
            FrameError::StockOrder {
                pass: entry(early),
                needs: entry(needs),
            },
        );
    }
}

#[test]
fn opaque_depth_read_before_it_is_complete_is_refused_before_any_feature_prepares() {
    let mut rig = Rig::new();
    let (depth, depth_view) = rig.depth("scene depth");
    let (writer_color, writer_view) = rig.colour("writer color");
    let (reader_color, reader_view) = rig.colour("reader color");
    let (opaque_color, opaque_view) = rig.colour("opaque color");
    // A caller pass clears the depth and another reads it, and only then does Opaque draw with it.
    // The read has a producer, so frame validation admits it; the depth is not Opaque's yet.
    let writer = entry(PassSlot::Celestial);
    let reader = entry(PassSlot::AfterOpaqueDepth);
    let opaque = entry(PassSlot::Opaque);
    let mut first = pass(writer, &writer_color);
    first.depth = Some(depth.clone());
    let mut main = pass(opaque, &opaque_color);
    main.depth = Some(AttachmentDeclaration {
        load: AttachmentLoad::Load,
        ..depth.clone()
    });
    let passes = [first, pass(reader, &reader_color), main];
    let views = [depth_view, writer_view, reader_view, opaque_view];
    let declaration = drawing(&[(writer, &[]), (reader, &[depth.resource]), (opaque, &[])]);
    let outcome = rig.encode(&passes, &views, declaration);
    refused(
        &outcome,
        FrameError::IncompleteDepth {
            pass: reader,
            resource: depth.resource,
        },
    );
}

#[test]
fn opaque_depth_read_after_a_prepass_or_after_opaque_is_admitted() {
    let mut rig = Rig::new();
    let (depth, depth_view) = rig.depth("scene depth");
    let (normals, normals_view) = rig.colour("normals");
    let (early_color, early_view) = rig.colour("early color");
    let (opaque_color, opaque_view) = rig.colour("opaque color");
    let (late_color, late_view) = rig.colour("late color");
    // The depth is complete after the prepass, so a pass between the prepass and Opaque may read
    // it, and Opaque loads it. After Opaque it is complete too.
    let prepass = entry(PassSlot::DepthNormalPrepass);
    let early = entry(PassSlot::OpaqueLighting);
    let opaque = entry(PassSlot::Opaque);
    let late = entry(PassSlot::AfterOpaqueDepth);
    let mut first = pass(prepass, &normals);
    first.depth = Some(depth.clone());
    let mut main = pass(opaque, &opaque_color);
    main.depth = Some(AttachmentDeclaration {
        load: AttachmentLoad::Load,
        ..depth.clone()
    });
    let passes = [
        first,
        pass(early, &early_color),
        main,
        pass(late, &late_color),
    ];
    let views = [depth_view, normals_view, early_view, opaque_view, late_view];
    let reads = [depth.resource];
    let declaration = drawing(&[
        (prepass, &[]),
        (early, &reads),
        (opaque, &[]),
        (late, &reads),
    ]);
    let outcome = rig.encode(&passes, &views, declaration);
    outcome.result.unwrap();
    assert_eq!(outcome.draws, [prepass, early, opaque, late]);
}

/// The passes a G-buffer producer needs: a prepass that completes the depth, then `OpaqueGbuffer`
/// testing it read-only.
fn gbuffer_plan(rig: &mut Rig, with_prepass: bool) -> (Vec<PassDeclaration>, Vec<AttachmentView>) {
    let (depth, depth_view) = rig.depth("scene depth");
    let (normals, normals_view) = rig.colour("normals");
    let (material, material_view) = rig.colour("g0");
    let mut passes = Vec::new();
    let mut views = Vec::new();
    if with_prepass {
        let mut prepass = pass(entry(PassSlot::DepthNormalPrepass), &normals);
        prepass.depth = Some(depth.clone());
        passes.push(prepass);
        views.extend([depth_view, normals_view]);
    } else {
        // Without a prepass the depth is only a view an earlier frame left behind.
        views.extend([depth_view.with_retained_contents(), normals_view]);
    }
    let mut producer = pass(entry(PassSlot::OpaqueGbuffer), &material);
    producer.depth = Some(AttachmentDeclaration {
        load: AttachmentLoad::Load,
        read_only: true,
        ..depth
    });
    passes.push(producer);
    views.push(material_view);
    (passes, views)
}

fn producing() -> FeatureDeclaration {
    let mut declaration = drawing(&[(entry(PassSlot::OpaqueGbuffer), &[])]);
    declaration.passes[0].depth_format = Some(DEPTH);
    declaration
}

#[test]
fn a_gbuffer_producer_without_a_prepass_is_refused_before_any_feature_prepares() {
    let mut rig = Rig::new();
    let (passes, views) = gbuffer_plan(&mut rig, false);
    let outcome = rig.encode(&passes, &views, producing());
    refused(
        &outcome,
        FrameError::AttachmentContract(entry(PassSlot::OpaqueGbuffer)),
    );
}

#[test]
fn a_gbuffer_producer_after_a_prepass_is_admitted() {
    let mut rig = Rig::new();
    let (passes, views) = gbuffer_plan(&mut rig, true);
    let mut declaration = producing();
    declaration
        .passes
        .push(FeaturePass::new(PassSlot::DepthNormalPrepass));
    let outcome = rig.encode(&passes, &views, declaration);
    outcome.result.unwrap();
    assert_eq!(outcome.prepared, 1);
    assert_eq!(
        outcome.draws,
        [
            entry(PassSlot::DepthNormalPrepass),
            entry(PassSlot::OpaqueGbuffer)
        ]
    );
}

/// A registered feature builds its resources and prepares only for a plan that was admitted.
struct Registered {
    built: Rc<Cell<u32>>,
    prepared: Rc<Cell<u32>>,
}
impl FrameFeature for Registered {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration::new(PassSlot::Opaque)
    }
    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        self.built.set(self.built.get() + 1);
        Ok(Box::new(Counting {
            prepared: self.prepared.clone(),
            draws: Rc::default(),
        }))
    }
}

#[test]
fn a_refusal_neither_builds_nor_prepares_a_registered_feature() {
    let mut rig = Rig::new();
    let (built, prepared) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    rig.chain
        .register(
            &rig.gpu.device,
            Box::new(Registered {
                built: built.clone(),
                prepared: prepared.clone(),
            }),
        )
        .unwrap();
    // Opaque is listed before the lighting pass it must follow.
    let (passes, views) = ordered(&mut rig, &[PassSlot::Opaque, PassSlot::OpaqueLighting]);
    let outcome = rig.encode(&passes, &views, drawing(&[]));
    assert_eq!(
        outcome.result,
        Err(FrameError::StockOrder {
            pass: entry(PassSlot::Opaque),
            needs: entry(PassSlot::OpaqueLighting),
        })
    );
    assert_eq!((built.get(), prepared.get()), (0, 0));
}
