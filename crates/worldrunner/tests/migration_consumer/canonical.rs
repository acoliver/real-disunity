use super::{wait, wgpu};
use worldrunner::scene::{gpu::device::GpuContext, post, resources};

#[path = "canonical_factory.rs"]
mod factory;
#[path = "canonical_store.rs"]
mod store;

#[test]
fn canonical_post_factory_and_encoded_readback_preserve_scene_and_authored_stage() {
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("canonical post consumer requires an admitted GPU");
    let mut factory = factory::Fixture::new(&context);
    for size in [[65, 47], [130, 94]] {
        let pixels = factory.render(&context, size);
        assert_eq!(pixels.len(), (size[0] * size[1] * 4) as usize);
        assert_eq!(&pixels[..4], &[255, 255, 0, 255], "authored output stage");
        let center = ((size[1] / 2 * size[0] + size[0] / 2) * 4) as usize;
        assert_eq!(
            &pixels[center..center + 4],
            &[255, 0, 0, 255],
            "scene contribution through post chain"
        );
    }
    assert_eq!(factory.builds(), 2, "resize rebuilds caller stage bindings");
}

#[test]
fn a_wrong_authored_output_stage_is_visible_in_the_readback() {
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("canonical post consumer requires an admitted GPU");
    let mut factory = factory::Fixture::with_marker(&context, 0.0);
    let pixels = factory.render(&context, [65, 47]);
    assert_ne!(
        &pixels[..4],
        &[255, 255, 0, 255],
        "wrong stage must change the marker"
    );
}

#[test]
fn canonical_store_content_ids_preserve_old_images_and_admit_new_revisions() {
    let context = wait(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("canonical texture consumer requires an admitted GPU");
    let mut textures = resources::CanonicalTextureStore::new(context.device(), context.queue());
    let original = store::upload(&mut textures, "content:first", [255, 0, 0, 255]);
    let cached = textures
        .get_or_upload(
            "content:first",
            "must reuse",
            wgpu::TextureUsages::COPY_SRC,
            || panic!("immutable identity must not execute its upload provider"),
        )
        .unwrap();
    let replacement = store::upload(&mut textures, "content:second", [0, 255, 0, 255]);
    assert_eq!(store::read(&context, &original), [255, 0, 0, 255]);
    assert_eq!(store::read(&context, &cached), [255, 0, 0, 255]);
    assert_eq!(store::read(&context, &replacement), [0, 255, 0, 255]);
    assert_eq!(
        store::read(&context, &original),
        [255, 0, 0, 255],
        "replacement must not rewrite retained content"
    );
}
