use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::{AllocationBudget, ConfigurationError};
use worldrunner_render_plan::{SceneWaterSeedCoordinates, SceneWaterSurface};

fn configuration(body: SceneWaterBody) -> RendererConfiguration {
    let mut settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.frame.format = wgpu::TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.water = vec![body];
    settings
}

fn required(settings: &RendererConfiguration, context: &GpuContext) -> u64 {
    settings
        .allocations()
        .validate(&context.device().limits())
        .unwrap()
}

#[test]
fn endpoint_budget_plan_matches_allocated_geometry() {
    use SceneWaterSeedCoordinates::{EndpointClamped, Repeat};
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let baseline = required(&configuration(seed_tests::body(Repeat)), &context) - 72;
    assert_eq!(baseline + 72, 9500);
    for surface in [
        SceneWaterSurface::Rectangle,
        SceneWaterSurface::Mesh {
            positions_millimeters: Box::new([[0, 0], [100, 0], [0, 100]]),
            indices: Box::new([2, 0, 1, 1, 0, 2, 0, 2, 1]),
        },
    ] {
        for mode in [Repeat, EndpointClamped] {
            let mut body = seed_tests::body(mode);
            body.surface = surface.clone();
            let vertices = geometry::vertices(&body).unwrap();
            let seeds = endpoint::coordinates(&body);
            let allocated = Body::new(context.device(), &vertices, seeds.as_deref());
            let bytes =
                allocated.buffer.size() + allocated.seed.as_ref().map_or(0, wgpu::Buffer::size);
            let declared = required(&configuration(body), &context);
            assert_eq!(declared, baseline + bytes, "{mode:?}, {surface:?}");
            if mode == Repeat {
                assert_eq!(
                    declared,
                    baseline + u64::try_from(vertices.len()).unwrap() * 12
                );
            }
        }
    }
}

#[test]
fn endpoint_budget_rejects_old_short_total() {
    use SceneWaterSeedCoordinates::{EndpointClamped, Repeat};
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let old = required(&configuration(seed_tests::body(Repeat)), &context);
    let result = ConfiguredFrameRenderer::new(
        &context,
        configuration(seed_tests::body(EndpointClamped)),
        AllocationBudget(old),
    );
    assert!(
        matches!(result, Err(ConfigurationError::Budget { required, budget })
        if required == old + 48 && budget == old)
    );
}

#[test]
fn endpoint_budget_counts_empty_seed_placeholder() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut body = seed_tests::body(SceneWaterSeedCoordinates::EndpointClamped);
    body.surface = SceneWaterSurface::Mesh {
        positions_millimeters: Box::new([]),
        indices: Box::new([]),
    };
    let allocated = Body::new(context.device(), &[], Some(&[]));
    assert_eq!(allocated.seed.as_ref().unwrap().size(), 8);
    assert_eq!(geometry::allocation_sizes(&body), (12, Some(8)));
    let endpoint = required(&configuration(body.clone()), &context);
    body.material.flow_coordinates.as_mut().unwrap().mode = SceneWaterSeedCoordinates::Repeat;
    assert_eq!(endpoint, required(&configuration(body), &context) + 8);
}

#[test]
fn vertex_seed_budget_matches_allocated_bytes_for_both_address_modes() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for mode in [
        SceneWaterSeedCoordinates::Repeat,
        SceneWaterSeedCoordinates::EndpointClamped,
    ] {
        let mut body = seed_tests::body(mode);
        let fragment = required(&configuration(body.clone()), &context);
        body.material.flow_seed_sampling = worldrunner_render_plan::SceneWaterSeedSampling::Vertex;
        let vertices = geometry::vertices(&body).unwrap();
        let coordinates = endpoint::coordinates(&body);
        let allocated = Body::new(context.device(), &vertices, coordinates.as_deref());
        let (positions, seeds) = geometry::allocation_sizes(&body);
        assert_eq!(positions, allocated.buffer.size());
        assert_eq!(seeds, allocated.seed.as_ref().map(wgpu::Buffer::size));
        assert_eq!(required(&configuration(body), &context), fragment);
    }
}
