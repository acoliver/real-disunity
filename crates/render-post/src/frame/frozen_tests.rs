//! CPU admission checks for contours against every opaque occurrence.
use super::*;
use crate::frame::validate_stock_order;

fn contour_plan(position: usize, prepass: bool) -> (Vec<PassDeclaration>, FeatureDeclaration) {
    let color = attachment(
        AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        wgpu::StoreOp::Store,
    );
    let depth = AttachmentDeclaration {
        resource: ResourceRef::new("scene depth"),
        format: wgpu::TextureFormat::Depth32Float,
        load: AttachmentLoad::ClearDepth(1.0),
        ..color.clone()
    };
    let seed = if prepass {
        PassSlot::DepthNormalPrepass
    } else {
        PassSlot::Celestial
    };
    let mut first = PassDeclaration::new(seed);
    first.depth = Some(depth.clone());
    let mut passes = vec![first];
    for occurrence in [0, 7] {
        let mut opaque = PassDeclaration::nth(PassSlot::Opaque, occurrence);
        opaque.color = Some(AttachmentDeclaration {
            load: if occurrence == 0 {
                color.load
            } else {
                AttachmentLoad::Load
            },
            ..color.clone()
        });
        opaque.depth = Some(AttachmentDeclaration {
            load: AttachmentLoad::Load,
            ..depth.clone()
        });
        passes.push(opaque);
    }
    let mut contour = PassDeclaration::nth(PassSlot::ContourIds, 1);
    contour.color = Some(AttachmentDeclaration {
        resource: ResourceRef::new("visible contour IDs"),
        format: wgpu::TextureFormat::R8Uint,
        ..color
    });
    contour.depth = Some(AttachmentDeclaration {
        load: AttachmentLoad::Load,
        read_only: true,
        ..depth
    });
    passes.insert(position, contour);
    let declaration = contour_declaration(&passes);
    (passes, declaration)
}

fn contour_declaration(passes: &[PassDeclaration]) -> FeatureDeclaration {
    FeatureDeclaration {
        passes: passes
            .iter()
            .map(|pass| {
                let mut feature = FeaturePass::nth(pass.slot, pass.occurrence);
                feature.color_format = pass.color.as_ref().map(|color| color.format);
                feature.depth_format = pass.depth.as_ref().map(|depth| depth.format);
                feature
            })
            .collect(),
        effects: vec![],
    }
}

fn contour_order(position: usize, prepass: bool) -> Result<(), FrameError> {
    let (passes, declaration) = contour_plan(position, prepass);
    let features = [declaration];
    assert_eq!(validate_frame(&passes, &features, &[]), Ok(()));
    validate_stock_order(&passes, &features)
}

#[test]
fn frozen_contour_before_all_opaque_entries_is_refused() {
    for prepass in [false, true] {
        assert_eq!(
            contour_order(1, prepass),
            Err(FrameError::StockOrder {
                pass: PassEntry::nth(PassSlot::ContourIds, 1),
                needs: PassEntry::new(PassSlot::Opaque),
            })
        );
    }
}

#[test]
fn frozen_contour_between_opaque_entries_is_refused() {
    for prepass in [false, true] {
        assert_eq!(
            contour_order(2, prepass),
            Err(FrameError::StockOrder {
                pass: PassEntry::nth(PassSlot::ContourIds, 1),
                needs: PassEntry::nth(PassSlot::Opaque, 7),
            })
        );
    }
}

#[test]
fn frozen_contour_after_all_opaque_entries_is_admitted() {
    for prepass in [false, true] {
        assert_eq!(contour_order(3, prepass), Ok(()));
    }
}
