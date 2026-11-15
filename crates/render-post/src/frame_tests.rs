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
    let validate =
        |views: &[AttachmentView]| crate::frame::validate_frame_inputs(&passes, &read, &[], views);
    assert_eq!(validate(std::slice::from_ref(&retained)), Ok(()));
    let refused = Err(FrameError::MissingProducer(carried));
    assert_eq!(validate(std::slice::from_ref(&plain)), refused);
    assert_eq!(validate(&[]), refused);
}
