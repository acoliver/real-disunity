#[test]
fn hdr_without_recipes_has_only_ordinary_capture_inventory() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let first = Viewport {
        width: 64,
        height: 64,
    };
    let next = Viewport {
        width: 96,
        height: 64,
    };
    let mut presentation = Presentation::with_profile(
        &context,
        COLOR,
        first,
        first,
        crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        None,
    )
    .unwrap();
    for size in [first, next] {
        presentation.resize(&context, size, size).unwrap();
        assert_eq!(
            presentation
                .scheduler
                .configuration()
                .additional
                .validate(&context.device().limits())
                .unwrap(),
            capture_allocations(size)
                .validate(&context.device().limits())
                .unwrap()
        );
        assert!(!presentation.recipe_capacity);
    }
}

use super::*;

#[test]
fn near_budget_hdr_msaa_presentation_admits_without_recipes() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let size = Viewport {
        width: 4096,
        height: 2400,
    };
    let presentation = Presentation::with_profile(
        &context,
        COLOR,
        size,
        size,
        crate::FrameProfile::Hdr {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
        },
        None,
    )
    .unwrap();
    assert!(!presentation.recipe_capacity);
    assert_eq!(
        presentation
            .scheduler
            .configuration()
            .additional
            .validate(&context.device().limits())
            .unwrap(),
        275_251_200
    );
}

#[test]
fn native_capture_inventory_includes_staging_padded_read_and_three_images() {
    let limits = wgpu::Limits {
        max_texture_dimension_2d: 8192,
        max_buffer_size: 1024 * 1024 * 1024,
        ..Default::default()
    };
    for [width, height] in [[5120, 2880], [5121, 2880], [8192, 8192]] {
        let size = Viewport { width, height };
        let padded = (u64::from(width) * 4).next_multiple_of(256) * u64::from(height);
        let pixels = u64::from(width) * u64::from(height) * 4;
        let allocations = capture_allocations(size);
        assert_eq!(
            allocations.validate(&limits).unwrap(),
            4 * padded + 3 * pixels
        );
        assert_eq!(
            allocations.validate(&limits).unwrap() > 1024 * 1024 * 1024,
            width == 8192
        );
    }
}

#[test]
fn native_only_resize_refreshes_inventory_and_rejects_over_budget_before_allocation() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let first = Extents::new([4608, 2592], 8192).unwrap().unwrap();
    let next = Extents::new([5120, 2880], 8192).unwrap().unwrap();
    assert_eq!(first.render, next.render);
    let mut presentation = Presentation::with_profile(
        &context,
        COLOR,
        first.render,
        first.surface,
        crate::FrameProfile::Legacy,
        None,
    )
    .unwrap();
    let before = presentation
        .scheduler
        .configuration()
        .additional
        .validate(&context.device().limits())
        .unwrap();
    presentation
        .resize(&context, next.render, next.surface)
        .unwrap();
    assert_eq!(presentation.capture_size, next.surface);
    assert_eq!(
        presentation.scheduler.configuration().frame.output_extent(),
        [4096, 2304]
    );
    let after = presentation
        .scheduler
        .configuration()
        .additional
        .validate(&context.device().limits())
        .unwrap();
    assert!(after > before);
    assert_eq!(
        after,
        capture_allocations(next.surface)
            .validate(&context.device().limits())
            .unwrap()
    );
    let too_large = Extents::new([8192, 8192], 8192).unwrap().unwrap();
    assert!(
        presentation
            .resize(&context, too_large.render, too_large.surface)
            .is_err()
    );
    assert_eq!(presentation.capture_size, next.surface);
    assert_eq!(
        presentation
            .scheduler
            .configuration()
            .additional
            .validate(&context.device().limits())
            .unwrap(),
        after
    );
}
