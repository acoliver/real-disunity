use super::admission::{LayeredGpuError, buffer_size, supported};

#[test]
fn storage_and_allocation_limits_are_both_enforced() {
    for limits in [
        wgpu::Limits {
            max_storage_buffer_binding_size: 128,
            ..Default::default()
        },
        wgpu::Limits {
            max_buffer_size: 128,
            ..Default::default()
        },
    ] {
        for buffer in ["parameters", "texels", "images", "positions", "products"] {
            assert!(buffer_size(&limits, buffer, 128).is_ok());
            assert!(matches!(
                buffer_size(&limits, buffer, 192),
                Err(LayeredGpuError::BufferLimit {
                    required: 192,
                    limit: 128,
                    ..
                })
            ));
        }
    }
}

#[test]
fn layered_capability_requires_five_storage_bindings_and_compute_limits() {
    let limits = wgpu::Limits::default();
    assert!(supported(&limits));
    assert!(!supported(&wgpu::Limits {
        max_storage_buffers_per_shader_stage: 4,
        ..limits
    }));
    assert!(!supported(&wgpu::Limits {
        max_bindings_per_bind_group: 10,
        ..limits
    }));
    assert!(!supported(&wgpu::Limits {
        max_compute_workgroup_size_x: 32,
        ..limits
    }));
    assert!(!supported(&wgpu::Limits {
        max_compute_workgroup_size_y: 4,
        ..limits
    }));
    assert!(!supported(&wgpu::Limits {
        max_compute_invocations_per_workgroup: 32,
        ..limits
    }));
    assert!(!supported(&wgpu::Limits {
        max_storage_textures_per_shader_stage: 0,
        ..limits
    }));
}
