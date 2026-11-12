use super::*;

pub(crate) fn install_source(
    frame: &crate::compositor::FrameRenderer,
    convention: worldrunner_render_plan::SceneSpecularConvention,
    raw: bool,
    mut source: String,
) {
    let water = frame.water.as_ref().unwrap();
    let base = convention.shader_options();
    let options = [
        base[0],
        base[1],
        base[2],
        base[3],
        terms::mask(&water.settings),
    ];
    if raw {
        let selected = "let adjusted_visibility = direct_sunlight_visibility(raw_visibility);";
        assert_eq!(source.matches(selected).count(), 1);
        source = source.replace(
            selected,
            &format!("{selected} captured_visibility = adjusted_visibility;"),
        );
        source.push_str("\nvar<private> captured_visibility: f32;\n");
        source = source.replace(
            "vec4<f32>(color, 1.0)",
            "vec4<f32>(vec3<f32>(captured_visibility), 1.0)",
        );
    }
    let mut compiled_options = options;
    if water.settings[0].material.shadow_response.is_some() {
        compiled_options[4] |= shadow_response::SPECIALIZATION;
    }
    water.substitute((options, true), &source, compiled_options);
}
