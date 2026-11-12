use std::fmt::Write;

#[cfg(test)]
#[path = "reconstruction_tests.rs"]
mod tests;

pub(super) fn source(levels: usize, banked: bool) -> String {
    let mut source = format!(
        "{}\n{}\n{}\nfn reflection_extent(level: u32) -> vec2<i32> {{ switch level {{",
        include_str!("reconstruction_receiver.wgsl"),
        include_str!("reconstruction_geometry.wgsl"),
        include_str!("reconstruction.wgsl"),
    );
    for level in 0..levels {
        write!(
            source,
            "case {level}u: {{ return vec2<i32>(textureDimensions(reflection_{level})) / vec2<i32>(SAMPLE_LANES, 1); }}"
        )
        .expect("reflection logical extent");
    }
    source.push_str("default: { return vec2<i32>(1); } } }\n");
    if banked {
        source = source
            .replace(
                "textureLoad(reflection_0, pixel + offset, 0)",
                "textureLoad(reflection_0, pixel + offset, reflection_bank_index, 0)",
            )
            .replace(
                "fn reconstructed_reflection(uv: vec2<f32>, sample: u32, receiver: ReflectionReceiver, roughness: f32) -> vec4<f32> {",
                "fn reconstructed_reflection(uv: vec2<f32>, sample: u32, receiver: ReflectionReceiver, roughness: f32) -> vec4<f32> {\n    reflection_bank_index = select(0, 1, receiver.owner == 2u && camera.policy.x != 0.0);",
            );
        source.insert_str(0, "var<private> reflection_bank_index: i32 = 0;\n");
    }
    source
}

pub(super) fn shared(source: &mut String, columns: u32) {
    let start = source
        .find("fn reflection_extent(")
        .expect("logical extent");
    let marker = "default: { return vec2<i32>(1); } } }\n";
    let end = start + source[start..].find(marker).expect("extent end") + marker.len();
    source.replace_range(
        start..end,
        "fn reflection_extent(level: u32) -> vec2<i32> { return logical_sizes[level]; }\n",
    );
    *source = source.replace(
        "vec2<i32>(i32(sample) * reflection_extent(0u).x, 0)",
        &format!(
            "vec2<i32>(i32(sample % {columns}u), i32(sample / {columns}u)) * reflection_extent(0u)"
        ),
    );
}
