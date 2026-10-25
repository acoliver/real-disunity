use crate::bridge_support::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use worldrunner_render_gpu::submission::{FrameOwner, LeaseError, SubmitError};
use worldrunner_render_post::contracts::*;

#[test]
fn external_owner_rejects_expired_exports_and_tracks_prepared_and_submitted_use() {
    let (device, authority) = gpu();
    let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let mut owner = FrameOwner::new(output.clone(), &authority);
    let lease = owner.lease();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import_leased(&output, &lease, Semantic::LinearColor, None)
        .unwrap();
    let exported = registry
        .view(key, TextureRange::single(0, 0, wgpu::TextureAspect::All), 1)
        .unwrap();
    let raster = pass("leased frame output", exported.clone());
    legacy_prepared_lease(&device, &authority, &registry, &mut owner, &raster);
    let op = TypedOperation::new(
        raster.id,
        TypedWork::Draws {
            pass: raster,
            draws: vec![],
        },
    );
    let plan = registry
        .compile_operations(&device, std::slice::from_ref(&op))
        .unwrap();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            plan.encode_checked(&registry, encoder, None)
        })
        .unwrap();
    assert!(matches!(owner.retire(), Err(LeaseError::Busy)));
    prepared.cancel();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            plan.encode_checked(&registry, encoder, None)
        })
        .unwrap();
    let serial = authority.submitted_serial() + 1;
    prepared.submit().unwrap();
    assert_eq!(lease.last_submission(), serial);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    owner.retire().unwrap();
    assert_eq!(
        registry.check_view(&exported),
        Err(ContractError::Lease(LeaseError::Expired))
    );
    assert!(registry.compile_operations(&device, &[op]).is_err());
    assert!(
        authority
            .prepare(&device, |encoder, _| plan
                .encode_checked(&registry, encoder, None))
            .is_err()
    );
}
#[test]
fn dropping_external_owner_invalidates_prepared_work_before_submission() {
    let (device, authority) = gpu();
    let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let dropped = Arc::new(AtomicUsize::new(0));
    let owner = FrameOwner::new((output.clone(), DropProbe(dropped.clone())), &authority);
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import_leased(&output, &owner.lease(), Semantic::LinearColor, None)
        .unwrap();
    let target = registry
        .view(key, TextureRange::single(0, 0, wgpu::TextureAspect::All), 1)
        .unwrap();
    let raster = pass("cancelled external owner", target);
    let plan = registry
        .compile_operations(
            &device,
            &[TypedOperation::new(
                raster.id,
                TypedWork::Draws {
                    pass: raster,
                    draws: vec![],
                },
            )],
        )
        .unwrap();
    let foreign =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(authority.queue().clone());
    assert!(
        foreign
            .prepare(&device, |encoder, _| plan
                .encode_checked(&registry, encoder, None))
            .is_err()
    );
    let prepared = authority
        .prepare(&device, |encoder, _| {
            plan.encode_checked(&registry, encoder, None)
        })
        .unwrap();
    drop(owner);
    drop(plan);
    drop(registry);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    assert!(matches!(
        prepared.submit(),
        Err(SubmitError::Lease(LeaseError::Expired))
    ));
    assert_eq!(authority.submitted_serial(), 0);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

struct DropProbe(Arc<AtomicUsize>);
impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn legacy_prepared_lease(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    registry: &ResourceRegistry,
    owner: &mut FrameOwner<wgpu::Texture>,
    raster: &RasterPass,
) {
    let plan = registry
        .compile(device, std::slice::from_ref(raster))
        .unwrap();
    let prepared = authority
        .prepare(device, |encoder, _| {
            plan.encode(registry, encoder, |_, _| {})
        })
        .unwrap();
    assert!(matches!(owner.retire(), Err(LeaseError::Busy)));
    prepared.cancel();
}
