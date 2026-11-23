//! Retained contour producers follow membership and publish independently per slot.
use super::*;
use crate::compositor::StockPlanStatistics;

impl Rig {
    fn select_slot(&mut self, slot: usize) {
        self.renderer.select_frame_slot(slot);
        self.gpu.select_frame_slot(slot);
    }

    fn set_contour_id(&mut self, id: u8) {
        let handle = self.gpu.instance_handles().next().unwrap();
        let object = self.gpu.instance(handle).unwrap().0;
        self.gpu
            .set_instance_object(handle, object.with_contour_id(id))
            .unwrap();
    }

    fn submitted_ids(&self, context: &GpuContext) -> Option<Vec<u8>> {
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let capture = self
            .renderer
            .encode_contour_capture(context.device(), &mut encoder)
            .unwrap();
        encoder.finish().submit().unwrap();
        capture.as_ref().map(|texture| read(context, texture))
    }
}

fn exact_quad_ids(id: u8) -> Vec<u8> {
    (0..SIZE[1])
        .flat_map(|y| {
            (0..SIZE[0]).map(move |x| {
                if (8..24).contains(&x) && (6..18).contains(&y) {
                    id
                } else {
                    0
                }
            })
        })
        .collect()
}

#[test]
fn gpu_contour_prepass_membership_transitions_match_fresh_and_reuse_plan() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Rig::new(&context, &source_with_ids(0, true), 1);
    rig.select_slot(0);
    let baseline = rig.draw(&context);
    assert!(baseline.mask.is_none());
    assert!(rig.renderer.contour_target().is_none());
    assert_eq!(
        rig.renderer.contour_statistics(),
        crate::ContourStatistics::default()
    );
    for id in [1, 255, 0, 2] {
        rig.set_contour_id(id);
        let before = rig.renderer.stock_plan_statistics();
        let image = rig.draw(&context);
        let fresh = Rig::new(&context, &source_with_ids(id, true), 1).draw(&context);
        assert_eq!(image.mask, fresh.mask);
        assert_eq!(image.mask, (id != 0).then(|| exact_quad_ids(id)));
        assert_eq!(image.color, fresh.color);
        assert_eq!(image.color, baseline.color);
        assert_eq!(rig.renderer.contour_target().is_some(), id != 0);
        assert_eq!(
            rig.renderer.contour_statistics(),
            if id == 0 {
                crate::ContourStatistics::default()
            } else {
                crate::ContourStatistics {
                    passes: 1,
                    targets: 1,
                    samples: 1,
                }
            }
        );
        if id == 255 {
            assert_eq!(
                rig.renderer.stock_plan_statistics(),
                StockPlanStatistics {
                    reuses: before.reuses + 1,
                    ..before
                },
                "the active-to-active frame must reuse its schedule and bound table"
            );
        }
    }
}

fn assert_submitted_slots(
    context: &GpuContext,
    rig: &mut Rig,
    targets: &[Option<Texture>; 3],
    ids: [u8; 3],
) {
    for (slot, id) in ids.into_iter().enumerate() {
        rig.select_slot(slot);
        assert_eq!(rig.renderer.contour_target(), targets[slot], "slot {slot}");
        assert_eq!(
            rig.submitted_ids(context),
            (id != 0).then(|| exact_quad_ids(id)),
            "submitted IDs of slot {slot}"
        );
    }
}

fn abandon_membership(context: &GpuContext, rig: &mut Rig, id: u8) {
    rig.select_slot(1);
    rig.set_contour_id(id);
    let published = rig.renderer.contour_target();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let (mask, _) = rig.encode(context, &mut encoder);
    assert_eq!(mask.is_some(), id != 0);
    assert_eq!(rig.renderer.contour_target(), published);
    drop(encoder);
    assert_eq!(rig.renderer.contour_target(), published);
}

fn exercise_slot_lifetime(context: &GpuContext, prepass: bool) {
    let mut rig = Rig::new(context, &source_with_ids(255, prepass), 1);
    let mut targets = [None, None, None];
    let mut ids = [1, 2, 255];
    for (slot, id) in ids.into_iter().enumerate() {
        rig.select_slot(slot);
        rig.set_contour_id(id);
        assert_eq!(rig.draw(context).mask, Some(exact_quad_ids(id)));
        targets[slot] = Some(rig.renderer.contour_target().unwrap());
    }
    assert_ne!(targets[0], targets[1]);
    assert_ne!(targets[0], targets[2]);
    assert_ne!(targets[1], targets[2]);
    rig.select_slot(0);
    rig.set_contour_id(ids[0]);
    assert_eq!(rig.draw(context).mask, Some(exact_quad_ids(ids[0])));
    assert_submitted_slots(context, &mut rig, &targets, ids);
    for id in [0, 255] {
        abandon_membership(context, &mut rig, id);
        assert_submitted_slots(context, &mut rig, &targets, ids);
    }
    rig.select_slot(1);
    rig.set_contour_id(0);
    assert!(rig.draw(context).mask.is_none());
    targets[1] = None;
    ids[1] = 0;
    assert_submitted_slots(context, &mut rig, &targets, ids);
    abandon_membership(context, &mut rig, 2);
    assert_submitted_slots(context, &mut rig, &targets, ids);
    rig.select_slot(1);
    rig.set_contour_id(2);
    assert_eq!(rig.draw(context).mask, Some(exact_quad_ids(2)));
    targets[1] = Some(rig.renderer.contour_target().unwrap());
    ids[1] = 2;
    assert_ne!(targets[1], targets[0]);
    assert_ne!(targets[1], targets[2]);
    assert_submitted_slots(context, &mut rig, &targets, ids);
}

#[test]
fn gpu_contour_three_active_slots_keep_submitted_targets_through_membership_changes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for prepass in [false, true] {
        exercise_slot_lifetime(&context, prepass);
    }
}
