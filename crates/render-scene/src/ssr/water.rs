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
        views.push(context.resource(self.depth)?.view().clone());
        Ok(())
    }
    pub(super) fn resolve_views(&self, views: &mut Vec<wgpu::TextureView>) {
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
        packed = water_normal;
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
            "{source}\n@group(0) @binding(10) var water_normals: WATER_TEXTURE;\n@group(0) @binding(11) var water_material: WATER_TEXTURE;\n@group(0) @binding(12) var water_depth: WATER_DEPTH;"
        ),
        samples,
    )
}
pub(super) fn resolve_source(source: &str, levels: usize, samples: u32) -> String {
    let source = source.replace("let surface = textureLoad(material, p, 0);", "var surface = textureLoad(material, p, 0);")
        .replace("let normal = textureLoad(normals, p, 0);", "var normal = textureLoad(normals, p, 0);\n    let water_normal = textureLoad(water_normals, p, SAMPLE_INDEX);\n    let is_water = water_normal.a != 0.0;\n    if is_water { normal = water_normal; let water_surface = textureLoad(water_material, p, SAMPLE_INDEX); surface = vec4<f32>(vec3<f32>(water_surface.b), water_surface.a); }")
        .replace("if abs(textureLoad(final_depth", "if !is_water && abs(textureLoad(final_depth")
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
    let first = levels + 7;
    sampled(
        &format!(
            "{source}\n@group(0) @binding({first}) var water_normals: WATER_TEXTURE;\n@group(0) @binding({}) var water_material: WATER_TEXTURE;\n@group(0) @binding({}) var water_environment: WATER_TEXTURE;\n@group(0) @binding({}) var water_lut: texture_2d<f32>;\n{}",
            first + 1,
            first + 2,
            first + 3,
            include_str!("water_lut.wgsl")
        ),
        samples,
    )
}
