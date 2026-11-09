//! Source regression guard for the fragment-depth call and its reachable inputs.
//! Matching WGSL graphs do not guarantee cross-pipeline fragment-depth bits on another
//! compiler or device. The paired GPU attachment differentials provide that evidence.
use super::position_dependencies::graph;
use crate::scene_metadata::{
    MetadataOutput,
    receiver::{self, ReceiverLayout},
};

fn depth_call(source: &str) {
    for suffix in ["opaque", "masked"] {
        for prefix in [
            "fs_relief_",
            "reflected_fs_relief_",
            "shaded_relief_",
            "shaded_",
        ] {
            let name = format!("{prefix}{suffix}");
            if source.contains(&format!("fn {name}(")) {
                let function = receiver::function_source(source, &name);
                if prefix == "shaded_"
                    && !function
                        .split_once('{')
                        .unwrap()
                        .0
                        .contains("SceneSurfaceOutput")
                {
                    continue;
                }
                let body: String = function
                    .split_once('{')
                    .unwrap()
                    .1
                    .split_whitespace()
                    .collect();
                assert!(depth_body_matches(&body, suffix), "{name}: {body}");
            }
        }
    }
}

fn depth_body_matches(body: &str, suffix: &str) -> bool {
    if body.starts_with("letdepth=scene_relief_depth(input);") {
        return body.contains("return")
            && body
                .split("return")
                .skip(1)
                .all(|value| value.split_once(';').unwrap().0.ends_with(",depth.value)"));
    }
    body == format!(
        "letshaded=shaded_{suffix}(input,front);varout=reflection_output(shaded.colour);out.depth=shaded.depth;returnout;}}"
    ) || body
        == format!(
            "letshaded=shaded_relief_{suffix}(input,front);returnSceneVelocityReliefOutput(shaded.colour,shaded.depth,scene_object_velocity(input));}}"
        )
        || body
            == format!(
                "letr=reflected_fs_relief_{suffix}(input,front);returnSceneVelocityReliefOutput(r.depth,r.color,r.environment,r.factor,scene_object_velocity(input));}}"
            )
}

#[test]
fn relief_depth_graph_and_call_inputs_match_metadata_shadow_reflection_and_velocity() {
    let expected = graph(super::ENVIRONMENT_WGSL, "scene_relief_depth");
    for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
        for output in [MetadataOutput::Normal, MetadataOutput::Merged] {
            let (object, _) = receiver::producer_sources(
                output,
                layout,
                0.012,
                crate::scene_metadata::receiver::ReceiverCapture::Normal,
            );
            assert_eq!(expected, graph(&object, "scene_relief_depth"));
            depth_call(&object);
        }
        for reflection in [false, true] {
            for velocity in [false, true] {
                let (object, _) = crate::shadows::receiver_sources(true, reflection, velocity);
                let object = receiver::consumer_source(&object, layout, false);
                assert_eq!(expected, graph(&object, "scene_relief_depth"));
                depth_call(&object);
            }
        }
    }
}

#[test]
fn relief_depth_graph_rejects_sampling_projection_bias_and_input_mutations() {
    let source = super::ENVIRONMENT_WGSL;
    let expected = graph(source, "scene_relief_depth");
    for (from, to) in [
        (
            "depth * material.impostor_relief.x",
            "depth * material.impostor_relief.y",
        ),
        ("clip.z / clip.w", "clip.z * clip.w"),
        ("dpdx(uv[0])", "dpdy(uv[0])"),
        (
            "@group(1) @binding(8) var impostor_depth",
            "@group(1) @binding(19) var impostor_depth",
        ),
    ] {
        assert!(source.contains(from), "mutation must exercise {from}");
        assert_ne!(
            expected,
            graph(&source.replace(from, to), "scene_relief_depth")
        );
    }
}

#[test]
fn relief_depth_call_rejects_changed_arguments_and_returned_depth() {
    let body = "letdepth=scene_relief_depth(input);returnSceneSurfaceOutput(colour,depth.value);}";
    assert!(depth_body_matches(body, "opaque"));
    for changed in [
        body.replace("scene_relief_depth(input)", "scene_relief_depth(other)"),
        body.replace("depth.value)", "depth.value+0.001)"),
        body.replace("depth.value)", "input.clip_position.z)"),
    ] {
        assert!(!depth_body_matches(&changed, "opaque"));
    }
}
