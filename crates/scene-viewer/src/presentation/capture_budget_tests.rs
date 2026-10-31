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
    let mut presentation = Presentation::with_settings(
        &context,
        COLOR,
        [first, first],
        crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        None,
        &PresentationPolicy {
            budget: crate::PresentationBudget::default().into(),
            capture_pool: crate::CapturePoolPolicy::default(),
            recipes: false,
        },
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
        assert!(presentation.capture_pool.is_none());
    }
}

use super::*;

#[test]
fn near_budget_hdr_msaa_inventory_admits_without_recipes() {
    let size = Viewport {
        width: 4096,
        height: 2400,
    };
    let limits = wgpu::Limits {
        max_texture_dimension_2d: 8192,
        max_buffer_size: 1024 * 1024 * 1024,
        ..Default::default()
    };
    let mut settings = RendererConfiguration::legacy([size.width, size.height], COLOR);
    settings.frame = FrameConfig::new(
        [size.width, size.height],
        wgpu::TextureFormat::Rgba16Float,
        4,
    );
    settings.hdr = Some((1.0, crate::ToneMap::Reinhard));
    settings.additional = capture_allocations(size);
    assert_eq!(settings.additional.validate(&limits).unwrap(), 275_251_200);
    settings
        .allocations()
        .validate_budget::<std::convert::Infallible>(
            &limits,
            AllocationBudget(crate::PresentationBudget::default().0),
        )
        .unwrap();
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
    let first = tiny_extents([16, 16]);
    let next = tiny_extents([24, 16]);
    assert_eq!(first.render, next.render);
    let mut presentation = Presentation::with_settings(
        &context,
        COLOR,
        [first.render, first.surface],
        crate::FrameProfile::Legacy,
        None,
        &PresentationPolicy {
            budget: crate::PresentationBudget::default().into(),
            capture_pool: crate::CapturePoolPolicy::default(),
            recipes: false,
        },
    )
    .unwrap();
    let before = additional_bytes(&presentation, &context);
    presentation
        .resize(&context, next.render, next.surface)
        .unwrap();
    assert_eq!(presentation.capture_size, next.surface);
    assert_eq!(
        presentation.scheduler.configuration().frame.output_extent(),
        [8, 8]
    );
    let after = additional_bytes(&presentation, &context);
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

fn tiny_extents([width, height]: [u32; 2]) -> Extents {
    Extents {
        render: Viewport {
            width: 8,
            height: 8,
        },
        surface: Viewport { width, height },
    }
}

fn additional_bytes(
    presentation: &Presentation,
    context: &worldrunner_render_scene::device::GpuContext,
) -> u64 {
    presentation
        .scheduler
        .configuration()
        .additional
        .validate(&context.device().limits())
        .unwrap()
}
