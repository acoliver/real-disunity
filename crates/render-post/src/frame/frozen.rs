//! Admission of a frozen frame plan.
//!
//! A frozen plan is encoded exactly as given: nothing adds, removes or sorts its entries. The
//! stock passes still assume some orders, because the draws, uploads and attachments they were
//! written for follow them. A plan that breaks one is refused here, by name and before any
//! feature prepares, instead of producing a frame that is quietly wrong.
//!
//! These checks add to the contract checks of [`validate_plan`](super::validate_plan), which a
//! frozen plan also meets before preparation: a read of a resource nothing earlier wrote
//! ([`FrameError::MissingProducer`], unless the view declares retained contents), an entry whose
//! attachments differ from what its feature declared ([`FrameError::AttachmentContract`]) and a
//! target written twice in one pass ([`FrameError::AttachmentHazard`]).
use super::{
    AttachmentLoad, FeatureDeclaration, FrameError, GamePlace, PassDeclaration, PassEntry,
    PassSlot, ResourceRef,
};

/// Refuses a frozen plan whose entries are listed in an order the stock passes cannot run in.
///
/// # Errors
/// [`FrameError::StockOrder`] for a role listed before a role it must follow,
/// [`FrameError::IncompleteDepth`] for a read of the opaque depth before it is complete, and
/// [`FrameError::AttachmentContract`] for an `OpaqueGbuffer` producer that tests depth the depth
/// prefix has not completed.
///
/// The depth prefix is the depth/normal prepass and the game's passes after it
/// ([`GamePlace::AfterDepthPrepass`]); the last of them that writes a depth completes it.
pub(crate) fn validate_stock_order(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
) -> Result<(), FrameError> {
    validate_role_order(passes)?;
    validate_gbuffer_depth(passes, features)?;
    validate_opaque_depth(passes, features)
}

/// Whether the stock passes of `slot` need every entry of the role `earlier` listed before them.
///
/// The lighting pass lights what the G-buffer pass stored. Opaque loads what the lighting pass
/// wrote, the colour and the reflection products it takes over, and draws over them. The
/// reflection trace and the screen-space resolve read those products, so the lighting pass and
/// Opaque must have finished them. Contours attach depth read-only; resource validation alone
/// cannot establish that every Opaque entry has finished writing it.
///
/// The game's passes after the depth prepass load what the prepass wrote, and every pass that
/// reads the depth prefix (the depth pyramid, the occlusion metadata, SSAO, the G-buffer and
/// Opaque) must see what they added. Resource validation alone cannot establish that: those
/// passes read products the prepass already produced.
const fn must_follow(slot: PassSlot, earlier: PassSlot) -> bool {
    match slot {
        PassSlot::Game(GamePlace::AfterDepthPrepass) => {
            matches!(earlier, PassSlot::DepthNormalPrepass)
        }
        PassSlot::DepthPyramid(_)
        | PassSlot::OcclusionMetadata(_)
        | PassSlot::Ssao
        | PassSlot::SsaoBlurHorizontal
        | PassSlot::SsaoBlur
        | PassSlot::OpaqueGbuffer => {
            matches!(earlier, PassSlot::Game(GamePlace::AfterDepthPrepass))
        }
        PassSlot::OpaqueLighting => matches!(earlier, PassSlot::OpaqueGbuffer),
        PassSlot::Opaque => matches!(
            earlier,
            PassSlot::OpaqueLighting | PassSlot::Game(GamePlace::AfterDepthPrepass)
        ),
        PassSlot::ContourIds => matches!(earlier, PassSlot::Opaque),
        PassSlot::ReflectionTrace | PassSlot::ScreenSpaceAfterTransparent => {
            matches!(earlier, PassSlot::OpaqueLighting | PassSlot::Opaque)
        }
        _ => false,
    }
}

/// The roles of the depth prefix: the depth/normal prepass and the game's passes after it.
const fn completes_depth_prefix(slot: PassSlot) -> bool {
    matches!(
        slot,
        PassSlot::DepthNormalPrepass | PassSlot::Game(GamePlace::AfterDepthPrepass)
    )
}

/// The position of the last depth-prefix entry that writes `resource`, after which the prefix's
/// depth is complete.
fn prefix_depth_writer(passes: &[PassDeclaration], resource: ResourceRef) -> Option<usize> {
    passes
        .iter()
        .rposition(|pass| completes_depth_prefix(pass.slot) && writes_depth(pass, resource))
}

fn validate_role_order(passes: &[PassDeclaration]) -> Result<(), FrameError> {
    for (index, pass) in passes.iter().enumerate() {
        if let Some(needs) = passes[index + 1..]
            .iter()
            .find(|listed| must_follow(pass.slot, listed.slot))
        {
            return Err(FrameError::StockOrder {
                pass: pass.entry(),
                needs: needs.entry(),
            });
        }
    }
    Ok(())
}

/// A G-buffer producer that tests depth in `OpaqueGbuffer` tests the depth the depth prefix
/// completed, read-only, so that depth ownership stays with the prefix and Opaque.
fn validate_gbuffer_depth(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
) -> Result<(), FrameError> {
    let producers = features
        .iter()
        .flat_map(|feature| &feature.passes)
        .filter(|pass| pass.slot == PassSlot::OpaqueGbuffer && pass.depth_format.is_some());
    for producer in producers {
        let Some((index, pass)) = passes
            .iter()
            .enumerate()
            .find(|(_, pass)| pass.entry() == producer.entry())
        else {
            // Frame validation names an entry no pass declares.
            continue;
        };
        let completed = pass
            .depth
            .as_ref()
            .filter(|depth| depth.read_only && depth.load == AttachmentLoad::Load)
            .is_some_and(|depth| {
                prefix_depth_writer(passes, depth.resource).is_some_and(|writer| writer < index)
            });
        if !completed {
            return Err(FrameError::AttachmentContract(producer.entry()));
        }
    }
    Ok(())
}

fn writes_depth(pass: &PassDeclaration, resource: ResourceRef) -> bool {
    pass.depth
        .as_ref()
        .is_some_and(|depth| depth.resource == resource && !depth.read_only)
}

/// A pass that reads the depth Opaque draws with reads it complete: after the last depth-prefix
/// entry that writes it, or after every Opaque entry that writes it. A pass reads it by listing it among its feature's
/// reads or by testing against it through a read-only depth attachment; a pass that attaches it
/// for writing is a writer and is not held to this.
fn validate_opaque_depth(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
) -> Result<(), FrameError> {
    let mut resources: Vec<ResourceRef> = Vec::new();
    for opaque in passes.iter().filter(|pass| pass.slot == PassSlot::Opaque) {
        let depth = opaque.depth.as_ref().filter(|depth| !depth.read_only);
        if let Some(depth) = depth
            && !resources.contains(&depth.resource)
        {
            resources.push(depth.resource);
        }
    }
    for resource in resources {
        let prepass = prefix_depth_writer(passes, resource);
        let opaque = passes
            .iter()
            .rposition(|pass| pass.slot == PassSlot::Opaque && writes_depth(pass, resource));
        for (index, pass) in passes.iter().enumerate() {
            let complete = prepass.is_some_and(|prepass| prepass < index)
                || opaque.is_some_and(|opaque| opaque < index);
            if complete {
                continue;
            }
            let reader = tests_depth(pass, resource)
                || (!attaches(pass, resource) && reads(pass.entry(), features, resource));
            if reader {
                return Err(FrameError::IncompleteDepth {
                    pass: pass.entry(),
                    resource,
                });
            }
        }
    }
    Ok(())
}

/// Whether the pass tests against `resource` through a read-only depth attachment.
fn tests_depth(pass: &PassDeclaration, resource: ResourceRef) -> bool {
    pass.depth
        .as_ref()
        .is_some_and(|depth| depth.resource == resource && depth.read_only)
}

fn attaches(pass: &PassDeclaration, resource: ResourceRef) -> bool {
    pass.color
        .iter()
        .chain(&pass.additional_colors)
        .chain(&pass.depth)
        .any(|attachment| attachment.resource == resource)
}

fn reads(entry: PassEntry, features: &[FeatureDeclaration], resource: ResourceRef) -> bool {
    features
        .iter()
        .flat_map(|feature| &feature.passes)
        .any(|pass| pass.entry() == entry && pass.reads.contains(&resource))
}
