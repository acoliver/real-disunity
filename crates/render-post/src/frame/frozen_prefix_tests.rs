//! CPU admission checks for the game's passes after the depth prepass.
use super::*;
use crate::frame::validate_stock_order;

const AFTER_PREPASS: PassSlot = PassSlot::Game(GamePlace::AfterDepthPrepass);

fn depth(load: AttachmentLoad, read_only: bool) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource: ResourceRef::new("scene depth"),
        format: wgpu::TextureFormat::Depth32Float,
        load,
        read_only,
        ..attachment(load, wgpu::StoreOp::Store)
    }
}

fn normals(load: AttachmentLoad) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource: ResourceRef::new("prepass normals"),
        format: wgpu::TextureFormat::Rgba16Float,
        ..attachment(load, wgpu::StoreOp::Store)
    }
}

fn prepass() -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::DepthNormalPrepass);
    pass.color = Some(normals(AttachmentLoad::ClearColor(
        wgpu::Color::TRANSPARENT,
    )));
    pass.depth = Some(depth(AttachmentLoad::ClearDepth(1.0), false));
    pass
}

/// A game pass after the prepass that loads and writes the prepass's normals and depth.
fn game_writer(occurrence: u32) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(AFTER_PREPASS, occurrence);
    pass.color = Some(normals(AttachmentLoad::Load));
    pass.depth = Some(depth(AttachmentLoad::Load, false));
    pass
}

/// A game pass after the prepass that tests the depth without writing it.
fn game_tester(occurrence: u32) -> PassDeclaration {
    let mut pass = PassDeclaration::nth(AFTER_PREPASS, occurrence);
    pass.color = Some(AttachmentDeclaration {
        resource: ResourceRef::new("game target"),
        ..attachment(
            AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
            wgpu::StoreOp::Store,
        )
    });
    pass.depth = Some(depth(AttachmentLoad::Load, true));
    pass
}

fn pyramid() -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::DepthPyramid(0));
    pass.color = Some(AttachmentDeclaration {
        resource: ResourceRef::new("linear depth"),
        format: wgpu::TextureFormat::R32Float,
        ..attachment(
            AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
            wgpu::StoreOp::Store,
        )
    });
    pass
}

fn opaque() -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        wgpu::StoreOp::Store,
    ));
    pass.depth = Some(depth(AttachmentLoad::Load, false));
    pass
}

#[test]
fn a_game_pass_after_the_prepass_completes_the_depth_it_writes() {
    let early = [prepass(), game_tester(0), game_writer(1), opaque()];
    assert_eq!(
        validate_stock_order(&early, &[]),
        Err(FrameError::IncompleteDepth {
            pass: PassEntry::nth(AFTER_PREPASS, 0),
            resource: ResourceRef::new("scene depth"),
        })
    );
    let late = [prepass(), game_writer(0), game_tester(1), opaque()];
    assert_eq!(validate_stock_order(&late, &[]), Ok(()));
}

#[test]
fn the_depth_prefix_readers_follow_the_games_passes_after_the_prepass() {
    assert_eq!(
        validate_stock_order(&[prepass(), pyramid(), game_writer(0), opaque()], &[]),
        Err(FrameError::StockOrder {
            pass: PassEntry::new(PassSlot::DepthPyramid(0)),
            needs: PassEntry::nth(AFTER_PREPASS, 0),
        })
    );
    assert_eq!(
        validate_stock_order(&[game_writer(0), prepass(), pyramid(), opaque()], &[]),
        Err(FrameError::StockOrder {
            pass: PassEntry::nth(AFTER_PREPASS, 0),
            needs: PassEntry::new(PassSlot::DepthNormalPrepass),
        })
    );
    assert_eq!(
        validate_stock_order(&[prepass(), opaque(), game_writer(0), pyramid()], &[]),
        Err(FrameError::StockOrder {
            pass: PassEntry::new(PassSlot::Opaque),
            needs: PassEntry::nth(AFTER_PREPASS, 0),
        })
    );
    assert_eq!(
        validate_stock_order(&[prepass(), game_writer(0), pyramid(), opaque()], &[]),
        Ok(())
    );
}
