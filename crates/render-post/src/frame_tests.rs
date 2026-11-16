#[path = "frame/frozen_tests.rs"]
mod frozen_contour_tests;

#[test]
fn multiple_color_outputs_validate_shapes_hazards_and_producers() {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    let color = attachment(
        AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        wgpu::StoreOp::Store,
    );
    pass.color = Some(color.clone());
    let mut receiver = color;
    receiver.resource = ResourceRef::new("receiver");
    pass.additional_colors.push(receiver);
    assert!(validate_frame(&[pass.clone()], &[], &[]).is_ok());
    let mut consumer = PassDeclaration::new(PassSlot::TransparentWater);
    consumer.color = pass.color.clone();
    consumer.color.as_mut().unwrap().load = AttachmentLoad::Load;
    let mut feature = FeatureDeclaration::new(PassSlot::TransparentWater);
    feature.passes[0].reads.push(ResourceRef::new("receiver"));
    assert!(validate_frame(&[pass.clone(), consumer], &[feature], &[]).is_ok());
    pass.additional_colors[0].samples = 4;
    assert!(matches!(
        validate_frame(&[pass.clone()], &[], &[]),
        Err(FrameError::AttachmentContract(_))
    ));
    pass.additional_colors[0].samples = 1;
    pass.additional_colors[0].resource = ResourceRef::new("color");
    assert!(matches!(
        validate_frame(&[pass], &[], &[]),
        Err(FrameError::AttachmentHazard(_))
    ));
}

#[test]
fn stock_slots_accept_both_authored_shadow_prepass_orders() {
    let mut shadow = PassDeclaration::new(PassSlot::ShadowCasters(0));
    shadow.color = Some(attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    ));
    let mut prepass = shadow.clone();
    prepass.slot = PassSlot::DepthNormalPrepass;
    prepass.color.as_mut().unwrap().resource = ResourceRef::new("prepass color");
    assert!(validate_frame(&[shadow.clone(), prepass.clone()], &[], &[]).is_ok());
    assert!(validate_frame(&[prepass, shadow], &[], &[]).is_ok());
}

use crate::*;

fn attachment(load: AttachmentLoad, store: wgpu::StoreOp) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource: ResourceRef::new("color"),
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        size: [8, 8],
        samples: 1,
        load,
        store,
        read_only: false,
    }
}

#[test]
fn missing_producers_and_discarded_loads_are_typed() {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(AttachmentLoad::Load, wgpu::StoreOp::Store));
    assert!(matches!(
        validate_frame(&[pass.clone()], &[], &[]),
        Err(FrameError::MissingProducer(_))
    ));
    pass.color.as_mut().unwrap().load = AttachmentLoad::ClearColor(wgpu::Color::BLACK);
    pass.color.as_mut().unwrap().store = wgpu::StoreOp::Discard;
    let mut consumer = pass.clone();
    consumer.slot = PassSlot::TransparentWater;
    consumer.color.as_mut().unwrap().load = AttachmentLoad::Load;
    assert!(matches!(
        validate_frame(&[pass, consumer], &[], &[]),
        Err(FrameError::LoadAfterDiscard(_))
    ));
}

#[test]
fn active_attachment_sampling_and_unsupported_effects_fail_before_execution() {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    ));
    let mut feature = FeatureDeclaration::new(PassSlot::Opaque);
    feature.passes[0].reads.push(ResourceRef::new("color"));
    assert!(matches!(
        validate_frame(&[pass.clone()], &[feature], &[]),
        Err(FrameError::AttachmentHazard(_))
    ));
    assert_eq!(
        validate_frame(&[pass], &[], &["SSAO"]),
        Err(FrameError::UnsupportedEffect("SSAO"))
    );
}

#[test]
fn the_same_entry_twice_is_refused_and_never_silently_reordered() {
    let passes = [
        PassDeclaration::new(PassSlot::Opaque),
        PassDeclaration::new(PassSlot::Opaque),
    ];
    assert_eq!(
        validate_frame(&passes, &[], &[]),
        Err(FrameError::DuplicatePass(PassEntry::new(PassSlot::Opaque)))
    );
}

#[test]
fn attachment_and_pipeline_mismatches_are_rejected() {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    assert_eq!(
        validate_frame(&[pass.clone()], &[], &[]),
        Err(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::Opaque
        )))
    );
    pass.color = Some(attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    ));
    let mut feature = FeatureDeclaration::new(PassSlot::Opaque);
    feature.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
    assert_eq!(
        validate_frame(&[pass.clone()], &[feature], &[]),
        Err(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::Opaque
        )))
    );
    let mut loaded = pass.clone();
    loaded.slot = PassSlot::TransparentWater;
    loaded.color.as_mut().unwrap().load = AttachmentLoad::Load;
    loaded.color.as_mut().unwrap().size = [4, 4];
    assert_eq!(
        validate_frame(&[pass.clone(), loaded], &[], &[]),
        Err(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::TransparentWater
        )))
    );
    pass.color.as_mut().unwrap().samples = 4;
    assert_eq!(
        validate_frame(&[pass], &[FeatureDeclaration::new(PassSlot::Opaque)], &[]),
        Err(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::Opaque
        )))
    );
}

#[test]
fn resource_versions_and_ambiguous_writes_are_not_inferred() {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    ));
    let mut repeated = pass.clone();
    repeated.slot = PassSlot::TransparentWater;
    assert!(matches!(
        validate_frame(&[pass.clone(), repeated], &[], &[]),
        Err(FrameError::AttachmentHazard(_))
    ));
    let mut feature = FeatureDeclaration::new(PassSlot::Opaque);
    feature.passes[0].reads.push(ResourceRef {
        version: 1,
        ..ResourceRef::new("color")
    });
    assert!(matches!(
        validate_frame(&[pass], &[feature], &[]),
        Err(FrameError::MissingProducer(_))
    ));
}

#[test]
fn every_selectable_sample_count_validates_and_other_counts_are_rejected() {
    let declare = |samples: u32| {
        let mut pass = PassDeclaration::new(PassSlot::Opaque);
        for (format, load) in [
            (
                wgpu::TextureFormat::Rgba16Float,
                AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            ),
            (
                wgpu::TextureFormat::Depth32Float,
                AttachmentLoad::ClearDepth(1.0),
            ),
        ] {
            let mut declaration = attachment(load, wgpu::StoreOp::Store);
            declaration.format = format;
            declaration.samples = samples;
            if format.is_depth_stencil_format() {
                declaration.resource = ResourceRef::new("depth");
                pass.depth = Some(declaration);
            } else {
                pass.color = Some(declaration);
            }
        }
        pass
    };
    for samples in [1, 2, 4, 8] {
        assert_eq!(
            validate_frame(&[declare(samples)], &[], &[]),
            Ok(()),
            "{samples}x"
        );
    }
    for samples in [0, 3, 16] {
        assert_eq!(
            validate_frame(&[declare(samples)], &[], &[]),
            Err(FrameError::AttachmentContract(PassEntry::new(
                PassSlot::Opaque
            ))),
            "{samples}x"
        );
    }
}

#[test]
fn the_mask_pass_sits_between_the_water_and_alpha_passes_and_loads_what_the_host_cleared() {
    assert!(PassSlot::TransparentWater < PassSlot::TransparentWaterMask);
    assert!(PassSlot::TransparentWaterMask < PassSlot::TransparentAlpha);
    let mask = ResourceRef::new("temporal reactive mask");
    let mut loading = attachment(AttachmentLoad::Load, wgpu::StoreOp::Store);
    loading.resource = mask;
    let mut after_water = PassDeclaration::new(PassSlot::TransparentWaterMask);
    after_water.color = Some(loading);
    assert_eq!(
        validate_frame(&[after_water.clone()], &[], &[]),
        Err(FrameError::MissingProducer(mask))
    );
    let mut clearing = attachment(
        AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        wgpu::StoreOp::Store,
    );
    clearing.resource = mask;
    let mut first = PassDeclaration::new(PassSlot::Opaque);
    first.color = Some(clearing);
    assert!(validate_frame(&[first, after_water], &[], &[]).is_ok());
}

fn writing(slot: PassSlot, occurrence: u32, name: &'static str) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(slot, occurrence);
    let mut color = attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    );
    color.resource = ResourceRef::new(name);
    pass.color = Some(color);
    pass
}

fn reading(entry: PassEntry, name: &'static str) -> FeatureDeclaration {
    let mut feature = FeatureDeclaration::new(entry.slot());
    feature.passes[0] = FeaturePass::nth(entry.slot(), entry.occurrence());
    feature.passes[0].reads.push(ResourceRef::new(name));
    feature
}

#[test]
fn authored_order_decides_whether_a_read_has_a_producer_not_the_slot_enum() {
    assert!(PassSlot::ShadowCasters(0) < PassSlot::Opaque);
    let producer = writing(PassSlot::Opaque, 0, "a");
    let consumer = writing(PassSlot::ShadowCasters(0), 0, "b");
    let read = reading(PassEntry::new(PassSlot::ShadowCasters(0)), "a");
    assert_eq!(
        validate_frame(
            &[producer.clone(), consumer.clone()],
            std::slice::from_ref(&read),
            &[]
        ),
        Ok(()),
        "a later enum slot may run first when the author lists it first"
    );
    assert_eq!(
        validate_frame(&[consumer, producer], &[read], &[]),
        Err(FrameError::MissingProducer(ResourceRef::new("a"))),
        "a read whose producer is listed after it is refused by resource name"
    );
}

#[test]
fn a_repeated_entry_is_refused_by_name() {
    let entry = PassEntry::nth(PassSlot::Opaque, 1);
    let passes = [
        writing(PassSlot::Opaque, 0, "a"),
        writing(PassSlot::Opaque, 1, "b"),
        writing(PassSlot::Opaque, 1, "c"),
    ];
    assert_eq!(
        validate_frame(&passes, &[], &[]),
        Err(FrameError::DuplicatePass(entry))
    );
}

#[test]
fn one_role_may_appear_twice_and_a_feature_draws_only_in_the_entry_it_names() {
    let first = PassEntry::new(PassSlot::Opaque);
    let second = PassEntry::nth(PassSlot::Opaque, 1);
    let passes = [
        writing(PassSlot::Opaque, 0, "a"),
        writing(PassSlot::Opaque, 1, "b"),
    ];
    assert_eq!(validate_frame(&passes, &[], &[]), Ok(()));
    // "a" is stored by the first entry, so the second may sample it, but the first may not.
    assert_eq!(
        validate_frame(&passes, &[reading(second, "a")], &[]),
        Ok(())
    );
    assert_eq!(
        validate_frame(&passes, &[reading(first, "a")], &[]),
        Err(FrameError::AttachmentHazard(ResourceRef::new("a")))
    );
    assert_eq!(
        validate_frame(&passes, &[reading(first, "b")], &[]),
        Err(FrameError::MissingProducer(ResourceRef::new("b")))
    );
    let mut wrong_format = reading(second, "a");
    wrong_format.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
    assert_eq!(
        validate_frame(&passes, &[wrong_format], &[]),
        Err(FrameError::AttachmentContract(second)),
        "the error names the entry whose feature broke the contract"
    );
}

#[test]
fn a_feature_naming_an_entry_the_frame_does_not_author_is_refused() {
    let absent = PassEntry::nth(PassSlot::Opaque, 2);
    let passes = [
        writing(PassSlot::Opaque, 0, "a"),
        writing(PassSlot::Opaque, 1, "b"),
    ];
    assert_eq!(
        validate_frame(&passes, &[reading(absent, "a")], &[]),
        Err(FrameError::MissingPass(absent))
    );
}

#[test]
fn a_read_of_an_earlier_frames_contents_validates_only_through_a_retained_view() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let carried = ResourceRef::new("carried");
    let texture =
        crate::targets::color_texture(&device, "carried", [4, 4], wgpu::TextureFormat::Rgba8Unorm);
    let plain = AttachmentView::new(carried, &texture).unwrap();
    let retained = plain.clone().with_retained_contents();
    let passes = [writing(PassSlot::Opaque, 0, "out")];
    let read = [reading(PassEntry::new(PassSlot::Opaque), "carried")];
    let validate = |views: &[AttachmentView]| {
        crate::frame::validate_frame_inputs(&passes, &read, &[], views, &[])
    };
    assert_eq!(validate(std::slice::from_ref(&retained)), Ok(()));
    let refused = Err(FrameError::MissingProducer(carried));
    assert_eq!(validate(std::slice::from_ref(&plain)), refused);
    assert_eq!(validate(&[]), refused);
}

/// Operations that leave `outputs` written; encoding is never reached by validation.
struct Leaves(Vec<OperationsOutput>);
impl FrameOperations for Leaves {
    fn produces(&self) -> &[OperationsOutput] {
        &self.0
    }
    fn encode_operations(
        &self,
        _: Option<&TimingState>,
        _: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        Ok(())
    }
}

const OFFSCREEN: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Two `Opaque` entries, the second reading `texture`, which a group may leave written.
struct Offscreen {
    device: wgpu::Device,
    texture: wgpu::Texture,
    resource: ResourceRef,
    passes: Vec<PassDeclaration>,
    entries: [PassEntry; 2],
}
impl Offscreen {
    fn new() -> Self {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let texture = crate::targets::color_texture(&device, "offscreen", [4, 4], OFFSCREEN);
        Self {
            device,
            texture,
            resource: ResourceRef::new("offscreen"),
            passes: vec![
                writing(PassSlot::Opaque, 0, "a"),
                writing(PassSlot::Opaque, 1, "b"),
            ],
            entries: [
                PassEntry::new(PassSlot::Opaque),
                PassEntry::nth(PassSlot::Opaque, 1),
            ],
        }
    }
    fn output(&self) -> OperationsOutput {
        OperationsOutput::new(self.resource, OFFSCREEN, [4, 4])
    }
    fn leaves(&self) -> Leaves {
        Leaves(vec![self.output()])
    }
    /// The view a host hands in for the written texture.
    fn view(&self) -> AttachmentView {
        AttachmentView::new(self.resource, &self.texture)
            .unwrap()
            .with_retained_contents()
    }
    fn views(&self) -> Vec<AttachmentView> {
        let mut views = self.pass_views();
        views.push(self.view());
        views
    }
    fn pass_views(&self) -> Vec<AttachmentView> {
        let mut views = Vec::new();
        for pass in &self.passes {
            let color = pass.color.as_ref().unwrap();
            let texture = crate::targets::color_texture(
                &self.device,
                color.resource.name,
                color.size,
                color.format,
            );
            views.push(AttachmentView::new(color.resource, &texture).unwrap());
        }
        views
    }
    /// The same read with no group and no texture handed in at all.
    fn validate_unhosted(&self, reader: PassEntry) -> Result<(), FrameError> {
        crate::frame::validate_plan(
            &self.passes,
            &[reading(reader, "offscreen")],
            &[],
            &self.pass_views(),
            &[],
        )
    }
    fn validate(
        &self,
        reader: PassEntry,
        groups: &[OperationsGroup<'_>],
    ) -> Result<(), FrameError> {
        crate::frame::validate_plan(
            &self.passes,
            &[reading(reader, "offscreen")],
            &[],
            &self.views(),
            groups,
        )
    }
}
fn group_before(entry: Option<PassEntry>, work: &Leaves) -> OperationsGroup<'_> {
    OperationsGroup {
        before: entry,
        work,
    }
}

#[test]
fn a_read_at_or_after_a_groups_position_has_the_groups_output_as_its_producer() {
    let rig = Offscreen::new();
    let [first, second] = rig.entries;
    let ops = rig.leaves();
    assert_eq!(
        rig.validate(second, &[group_before(Some(second), &ops)]),
        Ok(()),
        "the group runs immediately before the entry that reads"
    );
    assert_eq!(
        rig.validate(second, &[group_before(Some(first), &ops)]),
        Ok(())
    );
}

#[test]
fn a_read_with_no_group_or_a_group_that_runs_later_has_no_producer() {
    let rig = Offscreen::new();
    let [first, second] = rig.entries;
    let missing = Err(FrameError::MissingProducer(rig.resource));
    let ops = rig.leaves();
    assert_eq!(rig.validate_unhosted(second), missing, "no group");
    assert_eq!(
        rig.validate(first, &[group_before(Some(second), &ops)]),
        missing,
        "the group runs after the reader"
    );
    assert_eq!(
        rig.validate(second, &[group_before(None, &ops)]),
        missing,
        "a group after every pass produces for no pass"
    );
}

#[test]
fn the_group_decides_which_version_of_a_resource_a_read_may_name() {
    let rig = Offscreen::new();
    let second = rig.entries[1];
    let mut newer = rig.resource;
    newer.version = 1;
    let ops = rig.leaves();
    let mut read = reading(second, "offscreen");
    read.passes[0].reads = vec![newer];
    assert_eq!(
        crate::frame::validate_plan(
            &rig.passes,
            &[read],
            &[],
            &rig.views(),
            &[group_before(Some(second), &ops)]
        ),
        Err(FrameError::MissingProducer(newer))
    );
}

#[test]
fn a_geometry_pass_that_writes_a_groups_output_is_refused_by_name() {
    let rig = Offscreen::new();
    let [first, second] = rig.entries;
    let ops = rig.leaves();
    let groups = [group_before(Some(second), &ops)];
    let refused = |entry| {
        Err(FrameError::OffscreenWrite {
            pass: entry,
            resource: rig.resource,
        })
    };
    // Clearing and storing it, in a pass after the group and in one before it.
    for (target, pass) in [(second, 1), (first, 0)] {
        let mut writer = rig.passes.clone();
        writer[pass].color.as_mut().unwrap().resource = rig.resource;
        assert_eq!(
            crate::frame::validate_plan(&writer, &[], &[], &rig.views(), &groups),
            refused(target)
        );
    }
    // Loading it and storing it back is a write too.
    let mut loader = rig.passes.clone();
    loader[1].color.as_mut().unwrap().resource = rig.resource;
    loader[1].color.as_mut().unwrap().load = AttachmentLoad::Load;
    assert_eq!(
        crate::frame::validate_plan(&loader, &[], &[], &rig.views(), &groups),
        refused(second)
    );
}

#[test]
fn a_read_only_depth_attachment_may_load_a_groups_output() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let depth = ResourceRef::new("offscreen depth");
    let format = wgpu::TextureFormat::Depth32Float;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen depth"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = AttachmentView::new(depth, &texture)
        .unwrap()
        .with_retained_contents();
    let ops = Leaves(vec![OperationsOutput::new(depth, format, [4, 4])]);
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.depth = Some(AttachmentDeclaration {
        resource: depth,
        format,
        size: [4, 4],
        samples: 1,
        load: AttachmentLoad::Load,
        store: wgpu::StoreOp::Store,
        read_only: true,
    });
    let entry = pass.entry();
    let validate = |pass: &PassDeclaration, groups: &[OperationsGroup<'_>]| {
        crate::frame::validate_plan(
            std::slice::from_ref(pass),
            &[],
            &[],
            std::slice::from_ref(&view),
            groups,
        )
    };
    assert_eq!(validate(&pass, &[group_before(Some(entry), &ops)]), Ok(()));
    assert_eq!(
        validate(&pass, &[group_before(None, &ops)]),
        Err(FrameError::MissingProducer(depth))
    );
    pass.depth.as_mut().unwrap().read_only = false;
    assert_eq!(
        validate(&pass, &[group_before(Some(entry), &ops)]),
        Err(FrameError::OffscreenWrite {
            pass: entry,
            resource: depth
        })
    );
}

#[test]
fn an_output_the_frame_has_no_matching_retained_view_for_is_refused() {
    let rig = Offscreen::new();
    let second = rig.entries[1];
    let ops = rig.leaves();
    let groups = [group_before(Some(second), &ops)];
    let refused = Err(FrameError::AttachmentView(rig.resource));
    let validate = |views: &[AttachmentView]| {
        crate::frame::validate_plan(
            &rig.passes,
            &[reading(second, "offscreen")],
            &[],
            views,
            &groups,
        )
    };
    let others: Vec<_> = rig
        .views()
        .into_iter()
        .filter(|view| view.resource() != rig.resource)
        .collect();
    assert_eq!(validate(&others), refused, "no view");
    let mut plain = others.clone();
    plain.push(AttachmentView::new(rig.resource, &rig.texture).unwrap());
    assert_eq!(validate(&plain), refused, "contents not declared retained");
    let wrong = Leaves(vec![OperationsOutput::new(
        rig.resource,
        wgpu::TextureFormat::Rgba16Float,
        [4, 4],
    )]);
    assert_eq!(
        crate::frame::validate_plan(
            &rig.passes,
            &[reading(second, "offscreen")],
            &[],
            &rig.views(),
            &[group_before(Some(second), &wrong)]
        ),
        refused,
        "the group declares another format than the texture has"
    );
}

#[test]
fn a_group_that_produces_nothing_changes_nothing() {
    let rig = Offscreen::new();
    let [first, second] = rig.entries;
    let nothing = Leaves(Vec::new());
    for reader in [first, second] {
        for groups in [
            vec![group_before(Some(second), &nothing)],
            vec![group_before(None, &nothing)],
        ] {
            assert_eq!(
                rig.validate(reader, &groups),
                rig.validate(reader, &[]),
                "{reader}"
            );
        }
    }
    // A retained view alone is the pre-existing contract: its contents exist from the start.
    assert_eq!(rig.validate(first, &[]), Ok(()));
}
