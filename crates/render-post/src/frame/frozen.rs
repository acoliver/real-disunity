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
    AttachmentLoad, FeatureDeclaration, FrameError, PassDeclaration, PassEntry, PassSlot,
    ResourceRef,
};

/// Refuses a frozen plan whose entries are listed in an order the stock passes cannot run in.
///
/// # Errors
/// [`FrameError::StockOrder`] for a role listed before a role it must follow,
/// [`FrameError::IncompleteDepth`] for a read of the opaque depth before it is complete, and
/// [`FrameError::AttachmentContract`] for an `OpaqueGbuffer` producer that tests depth no
/// prepass has completed.
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
const fn must_follow(slot: PassSlot, earlier: PassSlot) -> bool {
    match slot {
        PassSlot::OpaqueLighting => matches!(earlier, PassSlot::OpaqueGbuffer),
        PassSlot::Opaque => matches!(earlier, PassSlot::OpaqueLighting),
        PassSlot::ContourIds => matches!(earlier, PassSlot::Opaque),
        PassSlot::ReflectionTrace | PassSlot::ScreenSpaceAfterTransparent => {
            matches!(earlier, PassSlot::OpaqueLighting | PassSlot::Opaque)
        }
        _ => false,
    }
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

/// A G-buffer producer that tests depth in `OpaqueGbuffer` tests the depth a depth prepass
/// completed, read-only, so that depth ownership stays with the prepass and Opaque.
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
                passes[..index].iter().any(|earlier| {
                    earlier.slot == PassSlot::DepthNormalPrepass
                        && writes_depth(earlier, depth.resource)
                })
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

/// A pass that reads the depth Opaque draws with reads it complete: after a prepass wrote it, or
/// after every Opaque entry that writes it.
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
        let prepass = passes.iter().position(|pass| {
            pass.slot == PassSlot::DepthNormalPrepass && writes_depth(pass, resource)
        });
        let opaque = passes
            .iter()
            .rposition(|pass| pass.slot == PassSlot::Opaque && writes_depth(pass, resource));
        for (index, pass) in passes.iter().enumerate() {
            let complete = prepass.is_some_and(|prepass| prepass < index)
                || opaque.is_some_and(|opaque| opaque < index);
            if complete || attaches(pass, resource) {
                continue;
            }
            if reads(pass.entry(), features, resource) {
                return Err(FrameError::IncompleteDepth {
                    pass: pass.entry(),
                    resource,
                });
            }
        }
    }
    Ok(())
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
