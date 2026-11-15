use crate::shader::rewrite::ShaderRewrite;

pub(crate) fn source(axis: [i32; 2]) -> String {
    assert!(matches!(axis, [1, 0] | [0, 1]));
    include_str!("bilateral.wgsl")
        .anchored("ssao::bilateral::source", 1, "AXIS_X", &axis[0].to_string())
        .anchored("ssao::bilateral::source", 1, "AXIS_Y", &axis[1].to_string())
}
