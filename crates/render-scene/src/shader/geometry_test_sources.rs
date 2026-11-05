//! Shared forward, normal prepass, metadata and shadow fixture sources.

pub(crate) fn geometry_sources() -> [String; 4] {
    let forward = super::ENVIRONMENT_WGSL.to_owned();
    let mut base = forward.clone();
    for entry in [
        "fs_opaque",
        "fs_masked",
        "fs_relief_opaque",
        "fs_relief_masked",
    ] {
        base = base.replace(&format!("fn {entry}("), &format!("fn unused_{entry}("));
    }
    let geometry = include_str!("../scene_metadata/geometry.wgsl");
    [
        forward.clone(),
        format!(
            "{base}\n{}",
            geometry.replace("METADATA_OUTPUT", "vec4<f32>(normal, 1.0)")
        ),
        format!(
            "{base}\n{}",
            geometry.replace("METADATA_OUTPUT", "vec4<f32>(normal, roughness)")
        ),
        format!("{forward}\n{}", include_str!("shadow_depth.wgsl")),
    ]
}
