use super::*;

#[test]
fn readback_preflight_rejects_row_overflow_without_rejecting_valid_texture_bytes() {
    let limits = wgpu::Limits {
        max_texture_dimension_2d: u32::MAX,
        max_buffer_size: u64::MAX,
        ..wgpu::Limits::default()
    };
    let mut desc = descriptor(1, ReceiverDimension::D2);
    desc.budget.max_resolution = u32::MAX;
    desc.budget.max_texture_bytes = u64::MAX;
    desc.budget.max_readback_bytes = u64::MAX;
    for resolution in [u32::MAX / 4, u32::MAX / 4 + 1] {
        desc.resolution = resolution;
        assert_eq!(desc.validate(&limits), Ok(()));
        assert_eq!(
            desc.validate_readback(&limits),
            Err(DepthMapError::SizeOverflow)
        );
    }
    desc.readback = false;
    assert_eq!(
        desc.validate_readback(&limits),
        Err(DepthMapError::ReadbackDisabled)
    );
}

#[test]
fn texture_validation_defers_readback_limits() {
    let mut desc = descriptor(1, ReceiverDimension::D2);
    let limits = wgpu::Limits {
        max_buffer_size: 128 * 1024 * 1024,
        ..wgpu::Limits::default()
    };
    desc.resolution = 8192;
    desc.budget.max_resolution = 8192;
    desc.budget.max_texture_bytes = 8192 * 8192 * 4;
    desc.budget.max_readback_bytes = u64::MAX;
    assert_eq!(desc.validate(&limits), Ok(()));
    assert_eq!(
        desc.validate_readback(&limits),
        Err(DepthMapError::ReadbackBudget {
            required: 8192 * 8192 * 4,
            limit: limits.max_buffer_size,
        })
    );
    desc.budget.max_readback_bytes = 0;
    assert_eq!(desc.validate(&limits), Ok(()));
    assert!(matches!(
        desc.validate_readback(&limits),
        Err(DepthMapError::ReadbackBudget { limit: 0, .. })
    ));
}

#[test]
fn rejects_invalid_dimensions_before_allocation() {
    let limits = wgpu::Limits::default();
    let valid = descriptor(4, ReceiverDimension::D2Array);
    assert!(valid.validate(&limits).is_ok());
    for resolution in [0, 65, u32::MAX] {
        let invalid = DepthMapDescriptor {
            resolution,
            ..valid
        };
        assert!(matches!(
            invalid.validate(&limits),
            Err(DepthMapError::Resolution { .. })
        ));
    }
    for layers in [0, 5, u32::MAX] {
        let invalid = DepthMapDescriptor { layers, ..valid };
        assert!(matches!(
            invalid.validate(&limits),
            Err(DepthMapError::Layers { .. })
        ));
    }
    let invalid = DepthMapDescriptor {
        receiver_dimension: ReceiverDimension::D2,
        ..valid
    };
    assert_eq!(
        invalid.validate(&limits),
        Err(DepthMapError::ReceiverLayers)
    );
}

#[test]
fn rejects_texture_and_padded_readback_over_budget() {
    let limits = wgpu::Limits::default();
    let mut invalid = descriptor(4, ReceiverDimension::D2Array);
    invalid.budget.max_texture_bytes = 17 * 17 * 4 * 4 - 1;
    assert!(matches!(
        invalid.validate(&limits),
        Err(DepthMapError::TextureBudget { .. })
    ));
    invalid = descriptor(4, ReceiverDimension::D2Array);
    invalid.budget.max_readback_bytes = 256 * 17 - 1;
    assert!(matches!(
        invalid.validate_readback(&limits),
        Err(DepthMapError::ReadbackBudget { .. })
    ));
    invalid.readback = false;
    assert!(invalid.validate(&limits).is_ok());
    invalid.resolution = u32::MAX;
    invalid.layers = u32::MAX;
    invalid.budget.max_resolution = u32::MAX;
    invalid.budget.max_layers = u32::MAX;
    let enormous = wgpu::Limits {
        max_texture_dimension_2d: u32::MAX,
        max_texture_array_layers: u32::MAX,
        ..limits
    };
    assert_eq!(
        invalid.validate(&enormous),
        Err(DepthMapError::SizeOverflow)
    );
}

#[test]
fn honors_device_dimension_layer_and_buffer_limits() {
    let valid = descriptor(4, ReceiverDimension::D2Array);
    let mut limits = wgpu::Limits {
        max_texture_dimension_2d: 16,
        ..wgpu::Limits::default()
    };
    assert!(matches!(
        valid.validate(&limits),
        Err(DepthMapError::Resolution { .. })
    ));
    limits = wgpu::Limits {
        max_texture_array_layers: 3,
        ..wgpu::Limits::default()
    };
    assert!(matches!(
        valid.validate(&limits),
        Err(DepthMapError::Layers { .. })
    ));
    limits = wgpu::Limits {
        max_buffer_size: 256 * 17 - 1,
        ..wgpu::Limits::default()
    };
    assert_eq!(valid.validate(&limits), Ok(()));
    assert!(matches!(
        valid.validate_readback(&limits),
        Err(DepthMapError::ReadbackBudget { .. })
    ));
    limits.max_buffer_size = 256 * 17;
    assert_eq!(valid.validate_readback(&limits), Ok(()));
}
