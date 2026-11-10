use worldrunner_render_post::{FrameError, PrepareContext, ResourceRef};

pub(crate) struct Inputs {
    receivers: [wgpu::TextureView; 3],
    lut: wgpu::TextureView,
    pub(super) depth: ResourceRef,
}
impl Inputs {
    pub(crate) fn new(water: &crate::water::Water, slot: usize) -> Self {
        Self {
            receivers: water.receivers[slot]
                .each_ref()
                .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
            lut: water.environment[1].clone(),
            depth: ResourceRef::new("scene depth"),
        }
    }
    pub(super) fn trace_views(
        &self,
        context: &PrepareContext<'_>,
        views: &mut Vec<wgpu::TextureView>,
    ) -> Result<(), FrameError> {
        views.extend(self.receivers[..2].iter().cloned());
        views.push(context.resource(self.depth)?.sampled_view());
        Ok(())
    }
    pub(super) fn resolve_views(
        &self,
        views: &mut Vec<wgpu::TextureView>,
        forward_depth: wgpu::TextureView,
    ) {
        views[3] = forward_depth;
        views.extend(self.receivers.iter().cloned());
        views.push(self.lut.clone());
    }
}
pub(super) fn reads(depth: ResourceRef) -> [ResourceRef; 4] {
    [
        ResourceRef::new("water receiver normals"),
        ResourceRef::new("water receiver material"),
        ResourceRef::new("water environment specular"),
        depth,
    ]
}
fn sampled(source: &str, samples: u32) -> String {
    let source = source.replace("SAMPLE_LANES", &samples.to_string());
    if samples == 1 {
        source
            .replace("WATER_TEXTURE", "texture_2d<f32>")
            .replace("WATER_DEPTH", "texture_depth_2d")
            .replace("SAMPLE_INDEX", "0")
    } else {
        source
            .replace("WATER_TEXTURE", "texture_multisampled_2d<f32>")
            .replace("WATER_DEPTH", "texture_depth_multisampled_2d")
            .replace("SAMPLE_INDEX", "i32(sample)")
    }
}
pub(super) fn trace_source(source: &str, samples: u32) -> String {
    let source = source.replace("// WATER_RECEIVER_SELECTION", r"
    let water_normal = textureLoad(water_normals, p, SAMPLE_INDEX);
    if water_normal.a != 0.0 {
        packed = water_receiver_normal(water_normal);
        let water_surface = textureLoad(water_material, p, SAMPLE_INDEX);
        roughness = water_surface.a;
        origin_uv = (vec2<f32>(p) + water_surface.rg) / camera.size.xy;
        let device_depth = textureLoad(water_depth, p, SAMPLE_INDEX);
        let position = camera.inverse_projection * vec4<f32>(origin_uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), device_depth, 1.0);
        origin_depth = -position.z / position.w;
    }");
    let source = source.replace("// WATER_TARGET_OWNERSHIP", &format!(
        "for (var lane = 0; lane < {samples}; lane++) {{ if textureLoad(water_normals, pixel, {}).a != 0.0 {{ return false; }} }}",
        if samples == 1 { "0" } else { "lane" }));
    sampled(
        &format!(
            "{source}\n@group(0) @binding(10) var water_normals: WATER_TEXTURE;\n@group(0) @binding(11) var water_material: WATER_TEXTURE;\n@group(0) @binding(12) var water_depth: WATER_DEPTH;\n{}",
            include_str!("water_receiver.wgsl")
        ),
        samples,
    )
}
pub(super) fn resolve_source(source: &str, levels: usize, samples: u32) -> String {
    let source = source.replace("let surface = textureLoad(material, p, 0);", "var surface = textureLoad(material, p, 0);")
        .replace("let normal = textureLoad(normals, p, 0);", "var normal = textureLoad(normals, p, 0);\n    let water_normal = textureLoad(water_normals, p, SAMPLE_INDEX);\n    let is_water = water_normal.a != 0.0;\n    if is_water { normal = water_receiver_normal(water_normal); let water_surface = textureLoad(water_material, p, SAMPLE_INDEX); surface = vec4<f32>(water_receiver_f0(water_normal, water_surface), water_surface.a); }")
        .replace("let uv = pixel.xy / camera.size.xy;", "let uv = select(pixel.xy / camera.size.xy, (vec2<f32>(p) + textureLoad(water_material, p, SAMPLE_INDEX).rg) / camera.size.xy, is_water);")
        .replace("let replacement = camera.size.w != 0.0;", "let replacement = camera.size.w != 0.0 || is_water;")
        .replace("if replacement {", "if replacement && !is_water {")
        .replace("return vec4<f32>(reflection_delta", r"
    if is_water {
        let published = textureLoad(water_environment, p, SAMPLE_INDEX);
        // The receiver stores the forward environment coordinate, including remapping.
        factor = reflection_factor(surface.rgb, water_coefficients(vec2<f32>(cosine, surface.a))) * published.a;
        environment = published.rgb;
    }
    return vec4<f32>(reflection_delta");
    let source = reconstruction_source(&source);
    let first = levels + 7;
    sampled(
        &format!(
            "{source}\n@group(0) @binding({first}) var water_normals: WATER_TEXTURE;\n@group(0) @binding({}) var water_material: WATER_TEXTURE;\n@group(0) @binding({}) var water_environment: WATER_TEXTURE;\n@group(0) @binding({}) var water_lut: texture_2d<f32>;\n{}",
            first + 1,
            first + 2,
            first + 3,
            concat!(
                include_str!("water_lut.wgsl"),
                "\n",
                include_str!("water_receiver.wgsl")
            )
        ),
        samples,
    )
}

fn reconstruction_source(source: &str) -> String {
    source
        .replace("var final_depth: texture_2d<f32>;", "var water_depth: WATER_DEPTH;")
        .replace(
            "fn reflection_receiver_visible(p: vec2<i32>, depth: f32) -> bool {\n    return abs(textureLoad(final_depth, p, 0).r-depth) <= max(0.001, depth*0.0001);\n}",
            include_str!("reconstruction_water.wgsl"),
        )
        .replace(
            "// WATER_RECONSTRUCTION_RECEIVER",
            "let water_normal = textureLoad(water_normals, p, SAMPLE_INDEX);\n    if water_normal.a != 0.0 { return reflection_water_receiver(p, sample, water_normal); }",
        )
}

pub(super) fn hit_facing_source(source: &str) -> String {
    let source = source
        .replace(
            "if water_normal.a != 0.0 {",
            "let receiver_is_water = water_normal.a != 0.0;\n    if receiver_is_water {",
        )
        .replace(
            " || dot(hit_normal.xyz, direction) >= 0.0",
            " || (!receiver_is_water && dot(hit_normal.xyz, direction) >= 0.0)",
        );
    let grid = source.find("fn grid_reflection(").expect("grid reflection");
    let source = format!(
        "{}{}",
        &source[..grid],
        source[grid..].replace("!receiver_is_water", "!policy.receiver_is_water")
    );
    grid_receiver_policy(&source)
}

fn grid_receiver_policy(source: &str) -> String {
    source
        .replace("width: vec2<f32>, allowance: u32,", "width: vec2<f32>, allowance: u32, receiver_is_water: bool,")
        .replace("grid_reflection(centre, direction, origin_uv, roughness)", "grid_reflection(centre, direction, origin_uv, water_receiver_policy(roughness, receiver_is_water))")
        .replace("origin_uv: vec2<f32>, roughness: f32) -> vec4<f32>", "origin_uv: vec2<f32>, policy: GridPolicy) -> vec4<f32>")
        .replace(
            "let intersection = grid_walk(GridRay(first, last, weighted, reciprocal),\n        GridPolicy(grid_size, depth_limit, camera.traversal.z, u32(camera.settings.x),\n            vec2<f32>(camera.settings.y, camera.traversal.y),\n            reflection_pass_count(roughness, camera.traversal.w, camera.behind)));",
            "var grid_policy = policy;\n    grid_policy.size = grid_size;\n    grid_policy.end = depth_limit;\n    let intersection = grid_walk(GridRay(first, last, weighted, reciprocal), grid_policy);",
        ) + r"
fn water_receiver_policy(roughness: f32, receiver_is_water: bool) -> GridPolicy {
    var policy: GridPolicy;
    policy.skip = camera.traversal.z;
    policy.budget = u32(camera.settings.x);
    policy.width = vec2<f32>(camera.settings.y, camera.traversal.y);
    policy.allowance = reflection_pass_count(roughness, camera.traversal.w, camera.behind);
    policy.receiver_is_water = receiver_is_water;
    return policy;
}
"
}
