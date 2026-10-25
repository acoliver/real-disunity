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
fn slot_order_is_explicit_and_never_silently_reordered() {
    let passes = [
        PassDeclaration::new(PassSlot::Opaque),
        PassDeclaration::new(PassSlot::Opaque),
    ];
    assert_eq!(
        validate_frame(&passes, &[], &[]),
        Err(FrameError::SlotOrder)
    );
}

#[test]
fn attachment_and_pipeline_mismatches_are_rejected() {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    assert_eq!(
        validate_frame(&[pass.clone()], &[], &[]),
        Err(FrameError::AttachmentContract(PassSlot::Opaque))
    );
    pass.color = Some(attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    ));
    let mut feature = FeatureDeclaration::new(PassSlot::Opaque);
    feature.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
    assert_eq!(
        validate_frame(&[pass.clone()], &[feature], &[]),
        Err(FrameError::AttachmentContract(PassSlot::Opaque))
    );
    let mut loaded = pass.clone();
    loaded.slot = PassSlot::TransparentWater;
    loaded.color.as_mut().unwrap().load = AttachmentLoad::Load;
    loaded.color.as_mut().unwrap().size = [4, 4];
    assert_eq!(
        validate_frame(&[pass.clone(), loaded], &[], &[]),
        Err(FrameError::AttachmentContract(PassSlot::TransparentWater))
    );
    pass.color.as_mut().unwrap().samples = 4;
    assert_eq!(
        validate_frame(&[pass], &[FeatureDeclaration::new(PassSlot::Opaque)], &[]),
        Err(FrameError::AttachmentContract(PassSlot::Opaque))
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
