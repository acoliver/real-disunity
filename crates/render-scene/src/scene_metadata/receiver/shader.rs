use super::{ReceiverCapture, ReceiverLayout};
use crate::scene_metadata::MetadataOutput;
use crate::shader::rewrite::ShaderRewrite;
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
            "scene_prepare_terrain_receiver(input);\n".to_owned(),
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
    let solve = function(source, "terrain_quad_footprint")
        .anchored(
            "scene_metadata::receiver::shader::terrain_displacement",
            1,
            "terrain_quad_footprint(",
            "scene_receiver_displacement(",
        )
        .anchored(
            "scene_metadata::receiver::shader::terrain_displacement",
            1,
            ") -> vec2<f32>",
            ") -> vec3<f32>",
        )
        .anchored(
            "scene_metadata::receiver::shader::terrain_displacement",
            1,
            "clamp(displacement.xy * direction, vec2<f32>(-1e4), vec2<f32>(1e4))",
            "clamp(displacement * direction, vec3<f32>(-1e4), vec3<f32>(1e4))",
        );
    format!("{solve}\n{}", include_str!("terrain.wgsl"))
}

pub(super) fn terrain_local(source: &str) -> String {
    let entry = "fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> {";
    assert_eq!(source.matches(entry).count(), 1);
    let mut prepared = source.anchored(
        "scene_metadata::receiver::shader::terrain_local",
        1,
        entry,
        &format!("{entry}\n    scene_prepare_terrain_receiver(input);"),
    );
    for axis in ['x', 'y'] {
        let hardware = format!(
            "bitcast<vec3<f32>>(select(bitcast<vec3<u32>>(dpd{axis}(position) + dpd{axis}(normal) * shadow.bias.y), bitcast<vec3<u32>>(dpd{axis}(position)), shadow.bias.y == 0.0))"
        );
        assert_eq!(prepared.matches(&hardware).count(), 2);
        prepared = prepared.anchored("scene_metadata::receiver::shader::terrain_local", 2, &hardware, &format!("bitcast<vec3<f32>>(select(bitcast<vec3<u32>>(scene_receiver_d{axis} + dpd{axis}(normal) * shadow.bias.y), bitcast<vec3<u32>>(scene_receiver_d{axis}), shadow.bias.y == 0.0))"));
    }
    format!("{prepared}\n{}", terrain_displacement(source))
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
            .anchored(
                "scene_metadata::receiver::shader::producer",
                2,
                "@location(1) material: vec4<f32>,",
                &format!("@location(1) material: vec4<f32>,{extra}"),
            )
            .anchored(
                "scene_metadata::receiver::shader::producer",
                1,
                "DIELECTRIC_F0), roughness));",
                &format!("DIELECTRIC_F0), roughness){values});"),
            )
            .anchored(
                "scene_metadata::receiver::shader::producer",
                2,
                "surface.material, depth.value",
                &format!(
                    "surface.material{}, depth.value",
                    self::values(layout, "surface.")
                ),
            )
    } else {
        let object = object.anchored("scene_metadata::receiver::shader::producer", 1, "fn reflection_metadata(original: VertexOutput, front: bool, masked: bool, position: vec3<f32>, visible: bool) -> vec4<f32>", "fn reflection_metadata(original: VertexOutput, front: bool, masked: bool, position: vec3<f32>, visible: bool) -> SceneReceiverOutput")
            .anchored("scene_metadata::receiver::shader::producer", 1, "return vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0);",
                &format!("return SceneReceiverOutput(vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0){values});"));
        let object = object
            .anchored(
                "scene_metadata::receiver::shader::producer",
                2,
                "-> @location(0) vec4<f32> { return reflection_metadata",
                "-> SceneReceiverOutput { return reflection_metadata",
            )
            .anchored(
                "scene_metadata::receiver::shader::producer",
                2,
                "front: bool) -> SceneSurfaceOutput {",
                "front: bool) -> SceneReceiverReliefOutput {",
            )
            .anchored(
                "scene_metadata::receiver::shader::producer",
                2,
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
    let object = capture_before_return(&object, merged);
    let terrain = if merged {
        terrain.anchored("scene_metadata::receiver::shader::producer", 1, "@location(1) material: vec4<f32>,", &format!("@location(1) material: vec4<f32>,{extra}"))
            .anchored("scene_metadata::receiver::shader::producer", 1, "projected.properties.x));", &format!("projected.properties.x){values});"))
            .anchored("scene_metadata::receiver::shader::producer", 1, "return SceneMetadataOutput(", "scene_capture_receiver(input.world_position, projected.normal); return SceneMetadataOutput(")
    } else {
        let normal =
            "vec4<f32>(normalize((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0)";
        terrain.anchored("scene_metadata::receiver::shader::producer", 1, "fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32>", "fn fs_terrain(input: TerrainVertexOutput) -> SceneReceiverOutput")
            .anchored("scene_metadata::receiver::shader::producer", 1, &format!("return {normal};"), &format!("scene_capture_receiver(input.world_position, projected.normal); return SceneReceiverOutput({normal}{values});"))
            + &format!("\nstruct SceneReceiverOutput {{ @location(0) normal: vec4<f32>,{extra} }}")
    };
    let terrain = terrain.anchored(
        "scene_metadata::receiver::shader::producer",
        1,
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

/// Captures the receiver before the object returns: merged objects return the metadata
/// output, the others the receiver output of [`producer`].
fn capture_before_return(object: &str, merged: bool) -> String {
    let output = if merged {
        "    return SceneMetadataOutput("
    } else {
        "    return SceneReceiverOutput("
    };
    object.anchored(
        "scene_metadata::receiver::shader::producer",
        1,
        output,
        &format!("    scene_capture_receiver(input.world_position, projected.normal);\n{output}"),
    )
}

fn capture_before_coverage(source: &str, evaluation: ReceiverCapture) -> String {
    let body = function(source, "reflection_metadata");
    let coverage = "if !scene_geometry_covered(input) || (masked && !(albedo.a >= scene_alpha_cutoff(input, false))) || !visible { discard; }";
    let capture = "scene_capture_receiver(input.world_position, projected.normal);";
    assert_eq!(body.matches(coverage).count(), 1);
    assert_eq!(body.matches(capture).count(), 1);
    let prepared = if evaluation == ReceiverCapture::Position {
        body.anchored(
            "scene_metadata::receiver::shader::capture_before_coverage",
            1,
            capture,
            "",
        )
        .anchored(
            "scene_metadata::receiver::shader::capture_before_coverage",
            1,
            coverage,
            &format!("scene_capture_receiver(input.world_position, geometric);\n    {coverage}"),
        )
    } else {
        body.anchored(
            "scene_metadata::receiver::shader::capture_before_coverage",
            1,
            coverage,
            "",
        )
        .anchored(
            "scene_metadata::receiver::shader::capture_before_coverage",
            1,
            capture,
            &format!("{capture}\n    {coverage}"),
        )
    };
    source.anchored(
        "scene_metadata::receiver::shader::capture_before_coverage",
        1,
        body,
        &prepared,
    )
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
    let mut loaded = function.to_owned();
    for axis in ['x', 'y'] {
        let position = if function.contains("scene_receiver_dx") {
            format!("scene_receiver_d{axis}")
        } else {
            format!("dpd{axis}(position)")
        };
        let expression = format!(
            "let receiver_d{axis} = bitcast<vec3<f32>>(select(bitcast<vec3<u32>>({position} + dpd{axis}(normal) * shadow.bias.y), bitcast<vec3<u32>>({position}), shadow.bias.y == 0.0));"
        );
        loaded = loaded.anchored(
            "scene_metadata::receiver::shader::loaded",
            1,
            &expression,
            &format!("let receiver_d{axis} = scene_receiver_load({side}).d{axis};"),
        );
    }
    loaded
}
pub(super) fn consumer(source: &str, layout: ReceiverLayout, terrain: bool) -> String {
    let back = u32::from(layout == ReceiverLayout::Opposing);
    let cascaded = function(source, "cascaded_sunlight_visibility");
    let raw = function(source, "raw_sunlight_visibility");
    let card = function(source, "scene_prepare_card_shadow");
    let back_cascaded = loaded(cascaded, back).anchored_first(
        "scene_metadata::receiver::shader::consumer",
        1,
        "cascaded_sunlight_visibility",
        "scene_receiver_back_cascaded",
    );
    let back_raw = raw
        .anchored_first(
            "scene_metadata::receiver::shader::consumer",
            1,
            "raw_sunlight_visibility",
            "scene_receiver_back_raw",
        )
        .anchored(
            "scene_metadata::receiver::shader::consumer",
            1,
            "cascaded_sunlight_visibility",
            "scene_receiver_back_cascaded",
        );
    let back_card = loaded(card, back).anchored_first(
        "scene_metadata::receiver::shader::consumer",
        1,
        "scene_prepare_card_shadow",
        "scene_receiver_back_card",
    );
    let mut source = source
        .anchored(
            "scene_metadata::receiver::shader::consumer",
            1,
            cascaded,
            &loaded(cascaded, 0),
        )
        .anchored(
            "scene_metadata::receiver::shader::consumer",
            1,
            card,
            &loaded(card, 0),
        );
    if !terrain {
        // Only object surfaces shade a back side.
        source = source
            .anchored(
                "scene_metadata::receiver::shader::consumer",
                1,
                "sunlight_visibility(input.world_position, -normal)",
                "scene_receiver_back_visibility(input.world_position, -normal)",
            )
            .anchored(
                "scene_metadata::receiver::shader::consumer",
                1,
                "scene_prepare_card_shadow(input.world_position, -projected.normal)",
                "scene_receiver_back_card(input.world_position, -projected.normal)",
            );
    }
    if terrain {
        for name in ["fs_terrain", "shaded_terrain"] {
            if source.contains(&format!("fn {name}(")) {
                let body = function(&source, name).to_owned();
                source = source.anchored(
                    "scene_metadata::receiver::shader::consumer",
                    1,
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
        // shade, shade_material (the G-buffer producers' material entry) and the forward card
        // entry each start from the original vertex output.
        source = source.anchored("scene_metadata::receiver::shader::consumer", 3, "var input = original;", "scene_receiver_pixel = vec2<i32>(original.clip_position.xy);\n    var input = original;");
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
