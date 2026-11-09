use super::{ReceiverCapture, ReceiverLayout};
use crate::scene_metadata::MetadataOutput;
use std::fmt::Write;

fn fields(layout: ReceiverLayout, first: u32) -> String {
    let mut result = String::new();
    for side in 0..layout.sides() {
        let location = first + side * 2;
        write!(result, " @location({location}) receiver_{side}_a: vec4<f32>, @location({}) receiver_{side}_b: vec2<f32>,", location + 1).unwrap();
    }
    result
}
fn values(layout: ReceiverLayout, prefix: &str) -> String {
    let mut result = String::new();
    for side in 0..layout.sides() {
        write!(
            result,
            ", {prefix}receiver_{side}_a, {prefix}receiver_{side}_b"
        )
        .unwrap();
    }
    result
}
fn capture(layout: ReceiverLayout, bias: f32, terrain: bool) -> String {
    let (position, dx, dy, mut body) = if terrain {
        (
            "input: TerrainVertexOutput",
            "scene_receiver_dx",
            "scene_receiver_dy",
            "let plane = normalize(terrain_face_normal(input.world_position.xy, input.plane_index, input.skirt_axis));\nlet scene_receiver_dx = scene_receiver_displacement(input.clip_position, vec2<f32>(1.0, 0.0), plane);\nlet scene_receiver_dy = scene_receiver_displacement(input.clip_position, vec2<f32>(0.0, 1.0), plane);\n".to_owned(),
        )
    } else {
        (
            "position: vec3<f32>",
            "dpdx(position)",
            "dpdy(position)",
            String::new(),
        )
    };
    let mut globals = String::new();
    for side in 0..layout.sides() {
        let normal = if side == 0 { "normal" } else { "-normal" };
        let biased = |axis| {
            if bias == 0.0 {
                String::new()
            } else {
                format!(" + dpd{axis}({normal}) * bitcast<f32>({}u)", bias.to_bits())
            }
        };
        writeln!(body,
            "let dx_{side} = {dx}{};\nlet dy_{side} = {dy}{};\nreceiver_{side}_a = vec4<f32>(dx_{side}, dy_{side}.x); receiver_{side}_b = dy_{side}.yz;",
            biased('x'), biased('y'),
        ).unwrap();
        writeln!(
            globals,
            "var<private> receiver_{side}_a: vec4<f32>; var<private> receiver_{side}_b: vec2<f32>;"
        )
        .unwrap();
    }
    format!(
        "const SCENE_RECEIVER_SIDES: u32 = {}u;\n{globals}\nfn scene_capture_receiver({position}, normal: vec3<f32>) {{ {body} }}",
        layout.sides()
    )
}

fn terrain_displacement(source: &str) -> String {
    // Keep the material footprint's plane solve, including its grazing-angle bounds.
    function(source, "terrain_quad_footprint")
        .replace("terrain_quad_footprint(", "scene_receiver_displacement(")
        .replace(") -> vec2<f32>", ") -> vec3<f32>")
        .replace(
            "clamp(displacement.xy * direction, vec2<f32>(-1e4), vec2<f32>(1e4))",
            "clamp(displacement * direction, vec3<f32>(-1e4), vec3<f32>(1e4))",
        )
}

pub(super) fn producer(
    output: MetadataOutput,
    layout: ReceiverLayout,
    bias: f32,
    evaluation: ReceiverCapture,
) -> (String, String) {
    assert!(
        evaluation != ReceiverCapture::Position || (layout == ReceiverLayout::Front && bias == 0.0)
    );
    let (object, terrain) = crate::scene_metadata::geometry::sources(output);
    let merged = output == MetadataOutput::Merged;
    let first = if merged { 2 } else { 1 };
    let extra = fields(layout, first);
    let values = values(layout, "");
    let object_capture = capture(layout, bias, false);
    let terrain_capture = capture(layout, bias, true);
    #[cfg(test)]
    let (object_capture, terrain_capture) = if evaluation == ReceiverCapture::MainArithmetic {
        (
            super::arithmetic_reference::capture(&object_capture, layout, bias),
            super::arithmetic_reference::capture(&terrain_capture, layout, bias),
        )
    } else {
        (object_capture, terrain_capture)
    };
    let displacement = terrain_displacement(&terrain);
    let object = if merged {
        object
            .replace(
                "@location(1) material: vec4<f32>,",
                &format!("@location(1) material: vec4<f32>,{extra}"),
            )
            .replace(
                "DIELECTRIC_F0), roughness));",
                &format!("DIELECTRIC_F0), roughness){values});"),
            )
            .replace(
                "surface.material, depth.value",
                &format!(
                    "surface.material{}, depth.value",
                    self::values(layout, "surface.")
                ),
            )
    } else {
        let object = object.replace("fn reflection_metadata(original: VertexOutput, front: bool, masked: bool, position: vec3<f32>, visible: bool) -> vec4<f32>", "fn reflection_metadata(original: VertexOutput, front: bool, masked: bool, position: vec3<f32>, visible: bool) -> SceneReceiverOutput")
            .replace("return vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0);",
                &format!("return SceneReceiverOutput(vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0){values});"));
        let object = object
            .replace(
                "-> @location(0) vec4<f32> { return reflection_metadata",
                "-> SceneReceiverOutput { return reflection_metadata",
            )
            .replace(
                "front: bool) -> SceneSurfaceOutput {",
                "front: bool) -> SceneReceiverReliefOutput {",
            )
            .replace(
                "SceneSurfaceOutput(metadata, depth.value)",
                &format!(
                    "SceneReceiverReliefOutput(metadata.normal{}, depth.value)",
                    self::values(layout, "metadata.")
                ),
            );
        format!(
            "{object}\nstruct SceneReceiverOutput {{ @location(0) normal: vec4<f32>,{extra} }}\nstruct SceneReceiverReliefOutput {{ @location(0) normal: vec4<f32>,{extra} @builtin(frag_depth) depth: f32, }}"
        )
    };
    let object = object.replace("    return SceneMetadataOutput(", "    scene_capture_receiver(input.world_position, projected.normal);\n    return SceneMetadataOutput(")
        .replace("    return SceneReceiverOutput(", "    scene_capture_receiver(input.world_position, projected.normal);\n    return SceneReceiverOutput(");
    let terrain = if merged {
        terrain.replace("@location(1) material: vec4<f32>,", &format!("@location(1) material: vec4<f32>,{extra}"))
            .replace("projected.properties.x));", &format!("projected.properties.x){values});"))
            .replace("return SceneMetadataOutput(", "scene_capture_receiver(input.world_position, projected.normal); return SceneMetadataOutput(")
    } else {
        let normal =
            "vec4<f32>(normalize((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0)";
        terrain.replace("fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32>", "fn fs_terrain(input: TerrainVertexOutput) -> SceneReceiverOutput")
            .replace(&format!("return {normal};"), &format!("scene_capture_receiver(input.world_position, projected.normal); return SceneReceiverOutput({normal}{values});"))
            + &format!("\nstruct SceneReceiverOutput {{ @location(0) normal: vec4<f32>,{extra} }}")
    };
    let terrain = terrain.replace(
        "scene_capture_receiver(input.world_position, projected.normal)",
        "scene_capture_receiver(input, projected.normal)",
    );
    (
        format!(
            "{}\n{object_capture}",
            capture_before_coverage(&object, evaluation)
        ),
        format!("{terrain}\n{terrain_capture}\n{displacement}"),
    )
}

fn capture_before_coverage(source: &str, evaluation: ReceiverCapture) -> String {
    let body = function(source, "reflection_metadata");
    let coverage = "if !scene_geometry_covered(input) || (masked && !(albedo.a >= scene_alpha_cutoff(input, false))) || !visible { discard; }";
    let capture = "scene_capture_receiver(input.world_position, projected.normal);";
    assert_eq!(body.matches(coverage).count(), 1);
    assert_eq!(body.matches(capture).count(), 1);
    let prepared = if evaluation == ReceiverCapture::Position {
        body.replace(capture, "").replace(
            coverage,
            &format!("scene_capture_receiver(input.world_position, geometric);\n    {coverage}"),
        )
    } else {
        body.replace(coverage, "")
            .replace(capture, &format!("{capture}\n    {coverage}"))
    };
    source.replace(body, &prepared)
}

pub(super) fn function<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(&format!("fn {name}("))
        .expect("receiver function");
    let mut depth = 0;
    for (offset, c) in source[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[start..=start + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated receiver function")
}
fn loaded(function: &str, side: u32) -> String {
    function
        .replace(
            "let receiver_dx = bitcast<vec3<f32>>(select(bitcast<vec3<u32>>(dpdx(position) + dpdx(normal) * shadow.bias.y), bitcast<vec3<u32>>(dpdx(position)), shadow.bias.y == 0.0));",
            &format!("let receiver_dx = scene_receiver_load({side}).dx;"),
        )
        .replace(
            "let receiver_dy = bitcast<vec3<f32>>(select(bitcast<vec3<u32>>(dpdy(position) + dpdy(normal) * shadow.bias.y), bitcast<vec3<u32>>(dpdy(position)), shadow.bias.y == 0.0));",
            &format!("let receiver_dy = scene_receiver_load({side}).dy;"),
        )
}
pub(super) fn consumer(source: &str, layout: ReceiverLayout, terrain: bool) -> String {
    let back = u32::from(layout == ReceiverLayout::Opposing);
    let cascaded = function(source, "cascaded_sunlight_visibility");
    let raw = function(source, "raw_sunlight_visibility");
    let card = function(source, "scene_prepare_card_shadow");
    let back_cascaded = loaded(cascaded, back).replacen(
        "cascaded_sunlight_visibility",
        "scene_receiver_back_cascaded",
        1,
    );
    let back_raw = raw
        .replacen("raw_sunlight_visibility", "scene_receiver_back_raw", 1)
        .replace(
            "cascaded_sunlight_visibility",
            "scene_receiver_back_cascaded",
        );
    let back_card =
        loaded(card, back).replacen("scene_prepare_card_shadow", "scene_receiver_back_card", 1);
    let mut source = source
        .replace(cascaded, &loaded(cascaded, 0))
        .replace(card, &loaded(card, 0));
    source = source
        .replace(
            "sunlight_visibility(input.world_position, -normal)",
            "scene_receiver_back_visibility(input.world_position, -normal)",
        )
        .replace(
            "scene_prepare_card_shadow(input.world_position, -projected.normal)",
            "scene_receiver_back_card(input.world_position, -projected.normal)",
        );
    if terrain {
        for name in ["fs_terrain", "shaded_terrain"] {
            if source.contains(&format!("fn {name}(")) {
                let body = function(&source, name).to_owned();
                source = source.replace(
                    &body,
                    &body.replacen(
                        '{',
                        "{ scene_receiver_pixel = vec2<i32>(input.clip_position.xy);",
                        1,
                    ),
                );
            }
        }
    } else {
        source = source.replace("var input = original;", "scene_receiver_pixel = vec2<i32>(original.clip_position.xy);\n    var input = original;");
    }
    let source = if terrain {
        source
    } else {
        super::cards::source(&source)
    };
    format!(
        "{source}\n{back_cascaded}\n{back_raw}\n{back_card}\n{}",
        include_str!("consumer.wgsl")
    )
}
