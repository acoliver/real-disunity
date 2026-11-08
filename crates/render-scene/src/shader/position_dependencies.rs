//! Conservative current-position graph: compare whole reachable declarations, including
//! control flow, input layouts, uniform bindings/types, constants and helper bodies.
//! No arithmetic is reordered. Only the known velocity-only writes are projected away;
//! the camera wrapper is inlined by exact substitution. Unexpected rewrites fail closed.
use std::collections::{BTreeMap, BTreeSet};

type Graph = BTreeMap<String, Vec<String>>;

fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "projection changed: {from}"
    );
    source.replace(from, to)
}

fn current_position_source(source: &str) -> String {
    if !source.contains("struct PreviousInstance") {
        return source.to_owned();
    }
    let mut source = source.to_owned();
    for (from, to) in [
        (
            "    @location(4) ao_delta: vec4<f32>,",
            "    @location(4) ambient_occlusion: f32,",
        ),
        (", previous: PreviousInstance", ""),
        (
            "    scene_previous_model = mat4x4<f32>(previous.model_column_0, previous.model_column_1, previous.model_column_2, previous.model_column_3);",
            "",
        ),
        ("    scene_card_local = vertex.position;", ""),
        ("            scene_card_local = shaped;", ""),
        (
            "    out.ao_delta = vec4<f32>(out.ao_delta.x, scene_object_world_delta(vertex, out.world_position, instance));",
            "",
        ),
        (
            "fn billboard_basis(mode: u32, pivot: vec3<f32>) -> mat3x3<f32> { return billboard_basis_from(frame.camera_position.xyz, mode, pivot); }",
            "",
        ),
        (
            "fn billboard_basis_from(camera: vec3<f32>, mode: u32, pivot: vec3<f32>) -> mat3x3<f32> {\n    let to_camera = camera - pivot;",
            "fn billboard_basis(mode: u32, pivot: vec3<f32>) -> mat3x3<f32> {\n    let to_camera = frame.camera_position.xyz - pivot;",
        ),
    ] {
        source = replace_once(&source, from, to);
    }
    // These are scalar occlusion accesses, never current-position writes.
    source.replace("out.ao_delta.x", "out.ambient_occlusion")
}

fn tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index += 2;
            let mut depth = 1;
            while depth != 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else {
            let start = index;
            index += 1;
            if bytes[start].is_ascii_alphanumeric() || bytes[start] == b'_' {
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
            }
            result.push(source[start..index].to_owned());
        }
    }
    result
}

fn declaration_name(tokens: &[String]) -> &str {
    let kind = tokens
        .iter()
        .position(|t| {
            matches!(
                t.as_str(),
                "fn" | "struct" | "const" | "override" | "var" | "alias"
            )
        })
        .expect("WGSL declaration");
    let mut name = kind + 1;
    if tokens[name] == "<" {
        while tokens[name] != ">" {
            name += 1;
        }
        name += 1;
    }
    &tokens[name]
}

fn declarations(source: &str) -> Graph {
    let tokens = tokens(source);
    let mut graph = Graph::new();
    let mut start = 0;
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 && (token == "}" || token == ";") {
            let declaration = &tokens[start..=index];
            start = index + 1;
            if declaration == [";"] {
                continue;
            }
            let name = declaration_name(declaration).to_owned();
            assert!(graph.insert(name, declaration.to_vec()).is_none());
        }
    }
    assert_eq!(start, tokens.len());
    graph
}

pub(super) fn graph(source: &str, entry: &str) -> Graph {
    // Validate the actual production module before projecting non-position velocity work.
    let module = naga::front::wgsl::parse_str(source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let declarations = declarations(&current_position_source(source));
    let mut pending = vec![entry.to_owned()];
    let mut visited = BTreeSet::new();
    let mut result = Graph::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let declaration = declarations.get(&name).expect("reachable declaration");
        pending.extend(
            declaration
                .iter()
                .filter(|t| declarations.contains_key(*t))
                .cloned(),
        );
        result.insert(name, declaration.clone());
    }
    result
}

#[test]
fn position_graph_rejects_input_uniform_transform_wind_jitter_and_control_flow_mutations() {
    let source = super::ENVIRONMENT_WGSL;
    let expected = graph(source, "vs_main");
    for (from, to) in [
        (
            "@location(0) position: vec3<f32>",
            "@location(16) position: vec3<f32>",
        ),
        (
            "@group(1) @binding(0) var<uniform> material",
            "@group(1) @binding(16) var<uniform> material",
        ),
        (
            "pivot + rotation * vertex.position",
            "pivot - rotation * vertex.position",
        ),
        ("sin(cycles * 2.0 * PI)", "sin(cycles * 3.0 * PI)"),
        (
            "frame.view_projection * vec4<f32>(world_position, 1.0)",
            "frame.view_projection * vec4<f32>(world_position, 2.0)",
        ),
        (
            "if material.wind_clock.x != 0u",
            "if material.wind_clock.x == 0u",
        ),
    ] {
        assert!(source.contains(from), "mutation target {from}");
        assert_ne!(
            expected,
            graph(&source.replace(from, to), "vs_main"),
            "{from}"
        );
    }
    assert_eq!(
        expected,
        graph(
            &format!("{source}\n// irrelevant fragment commentary"),
            "vs_main"
        )
    );
    let velocity = crate::velocity::object_source(source);
    assert_eq!(expected, graph(&velocity, "vs_main"));
    assert_ne!(
        expected,
        graph(
            &velocity.replace(
                "pivot + rotation * vertex.position",
                "pivot - rotation * vertex.position"
            ),
            "vs_main"
        )
    );
}
