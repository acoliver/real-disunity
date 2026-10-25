pub(crate) fn source(axis: [i32; 2]) -> String {
    assert!(matches!(axis, [1, 0] | [0, 1]));
    include_str!("bilateral.wgsl")
        .replace("AXIS_X", &axis[0].to_string())
        .replace("AXIS_Y", &axis[1].to_string())
}
