use super::*;
use crate::object::skin_source::skinned_source;
use crate::renderer::depth::DepthMode;
use wgpu::FrontFace;
use worldrunner_render_program::state::RasterState;

#[test]
fn contour_states_only_read_equal_depth_without_stencil_or_integer_a2c() {
    for mode in [DepthMode::Independent, DepthMode::Shared, DepthMode::Reuse] {
        let original = mode.render_state(
            1,
            RasterState {
                cull: None,
                front_face: FrontFace::Ccw,
                alpha_to_coverage: true,
            },
        );
        let state = state(original);
        assert_eq!(
            state.depth.as_ref().unwrap().test,
            wgpu::CompareFunction::Equal
        );
        assert!(!state.depth.as_ref().unwrap().write);
        assert!(state.stencil.is_none());
        assert!(!state.raster.alpha_to_coverage);
        assert!(state.colors[0].as_ref().unwrap().blend.is_none());
    }
}

#[test]
fn contour_shader_entries_validate_for_static_and_skinned_geometry() {
    let source = format!(
        "{}\n{}",
        crate::shader::ENVIRONMENT_WGSL,
        include_str!("../shader/contour.wgsl")
    );
    for source in [source.clone(), skinned_source(&source)] {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        for key in PipelineKey::ALL {
            assert!(module.entry_points.iter().any(|ep| ep.name == entry(key)));
        }
    }
}
