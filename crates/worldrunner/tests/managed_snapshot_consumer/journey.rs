use super::{
    controls,
    extraction::Extraction,
    native::{Fixture, Session},
    receipt::ViewReceipt,
    render::Renderer,
};
use worldrunner::scene::gpu::device::GpuContext;

pub fn run(fixture: &mut Fixture, session: &Session, original: &Extraction, context: &GpuContext) {
    let mut renderer = Renderer::new(context);
    let initial = renderer
        .capture(fixture, session, original, session.object)
        .unwrap();
    initial
        .bind_current(fixture, session, original, &renderer.target)
        .unwrap();
    controls::markers(&initial, original, session.object);
    initial.retain("initial-65x47");
    let (first, image1) = first_mutation(fixture, session, original, &initial, &mut renderer);
    let (second, image2) = second_mutation(fixture, session, &first, &image1, &mut renderer);
    controls::retained(context, &initial);
    controls::retained(context, &image1);
    controls::resize(fixture, session, &second, &mut renderer, &image2);
    controls::retained(context, &image2);
    assert!(!original.validate(fixture, session, false).unwrap());
    original.release(fixture, session).unwrap();
    first.release(fixture, session).unwrap();
    controls::lifecycle(fixture, session, &second);
    assert!(
        renderer
            .capture(fixture, session, &second, session.object)
            .is_err()
    );
}

fn first_mutation(
    fixture: &mut Fixture,
    session: &Session,
    original: &Extraction,
    initial: &ViewReceipt,
    renderer: &mut Renderer<'_>,
) -> (Extraction, ViewReceipt) {
    let bytes = original.raw().to_vec();
    fixture.advance(session);
    assert!(original.validate(fixture, session, true).is_err());
    assert!(!original.validate(fixture, session, false).unwrap());
    assert!(
        renderer
            .capture(fixture, session, original, session.object)
            .is_err(),
        "stale source before encoding"
    );
    assert!(
        !initial
            .replay_historical(fixture, session, original, &renderer.target)
            .unwrap()
    );
    let first = Extraction::capture(fixture, session).unwrap();
    controls::retained_instance(original, &first);
    controls::precision(&first, session);
    assert_eq!(original.raw(), bytes);
    assert!(
        initial
            .bind_current(fixture, session, &first, &renderer.target)
            .is_err()
    );
    assert!(
        initial
            .replay_historical(fixture, session, &first, &renderer.target)
            .is_err()
    );
    let image = renderer
        .capture(fixture, session, &first, session.object)
        .unwrap();
    image
        .bind_current(fixture, session, &first, &renderer.target)
        .unwrap();
    controls::markers(&image, &first, session.object);
    assert_ne!(initial.gpu_completion_hash, image.gpu_completion_hash);
    image.retain("update1-65x47");
    (first, image)
}

fn second_mutation(
    fixture: &mut Fixture,
    session: &Session,
    first: &Extraction,
    image1: &ViewReceipt,
    renderer: &mut Renderer<'_>,
) -> (Extraction, ViewReceipt) {
    let error = renderer
        .capture_interleaved(fixture, session, first)
        .unwrap_err();
    assert!(
        error.contains("no longer current"),
        "post-readback rejection: {error}"
    );
    assert!(!first.validate(fixture, session, false).unwrap());
    let second = Extraction::capture(fixture, session).unwrap();
    controls::retained_instance(first, &second);
    assert_eq!(second.stamp.frame, 2);
    assert_eq!(
        second.position(session.object).unwrap().map(f32::to_bits),
        [1.0_f32, 0.5, 0.0].map(f32::to_bits)
    );
    assert!(
        image1
            .bind_current(fixture, session, &second, &renderer.target)
            .is_err()
    );
    let image = renderer
        .capture(fixture, session, &second, session.object)
        .unwrap();
    image
        .bind_current(fixture, session, &second, &renderer.target)
        .unwrap();
    controls::markers(&image, &second, session.object);
    assert_ne!(image1.gpu_completion_hash, image.gpu_completion_hash);
    assert_eq!(
        image.pixel(32, 23),
        [0, 0, 0, 255],
        "old managed position clears"
    );
    image.retain("update2-65x47");
    (second, image)
}
