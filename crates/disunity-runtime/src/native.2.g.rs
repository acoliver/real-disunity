// Generated from disunity-ffi ABI definitions. Do not edit.

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityFaceInfo {
    pub face_index: u32,
    pub units_per_em: u32,
    pub point_size: f32,
    pub scale: f32,
    pub line_height: f32,
    pub ascent_line: f32,
    pub cap_line: f32,
    pub mean_line: f32,
    pub baseline: f32,
    pub descent_line: f32,
    pub superscript_offset: f32,
    pub superscript_size: f32,
    pub subscript_offset: f32,
    pub subscript_size: f32,
    pub underline_offset: f32,
    pub underline_thickness: f32,
    pub strikethrough_offset: f32,
    pub strikethrough_thickness: f32,
    pub tab_width: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityGlyphMetrics {
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityGlyphPair {
    pub first: u32,
    pub second: u32,
    pub x_advance: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityGlyphRender {
    pub glyph: u32,
    pub mode: u32,
    pub padding: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub atlas_width: u32,
    pub atlas_height: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeShaderDesc {
    pub path_id: i64,
    pub name_length: u32,
    pub file_length: u32,
    pub kernel_count: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeKernel {
    pub name_offset: u32,
    pub name_length: u32,
    pub size_x: u32,
    pub size_y: u32,
    pub size_z: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeShaderInfo {
    pub path_id: i64,
    pub name_length: u32,
    pub file_length: u32,
    pub kernel_count: u32,
    pub text_length: u32,
    pub generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBufferDesc {
    pub count: u32,
    pub stride: u32,
    pub kind: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBufferInfo {
    pub count: u32,
    pub stride: u32,
    pub kind: u32,
    pub generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeGlobalTexture {
    pub name_length: u32,
    pub global_length: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBinding {
    pub kind: u32,
    pub name_offset: u32,
    pub name_length: u32,
    pub float_offset: u32,
    pub int_value: i32,
    pub texture_kind: u32,
    pub resource: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBindingsInfo {
    pub entry_count: u32,
    pub float_count: u32,
    pub text_length: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityShaderInfo {
    pub path_id: i64,
    pub name_length: u32,
    pub file_length: u32,
    pub has_source: u32,
    pub text_capacity: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityActivePass {
    pub material: u64,
    pub pass: u32,
    pub active: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMaterialInfo {
    pub shader: u64,
    pub render_queue: i32,
    pub keyword_count: u32,
    pub disabled_pass_count: u32,
    pub generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityRenderTexture {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub depth_bits: u32,
    pub mip_count: u32,
    pub srgb: u32,
    pub dimension: u32,
    pub volume_depth: u32,
    pub wrap_u: u32,
    pub wrap_v: u32,
    pub wrap_w: u32,
    pub filter: u32,
    pub aniso_level: u32,
    pub mip_bias: f32,
    pub generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityTargetRef {
    pub kind: u32,
    pub name_id: i32,
    pub render: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityOffscreenWork {
    pub kind: u32,
    pub camera: u64,
    pub target: u64,
    pub source_kind: u32,
    pub source: u64,
    pub material: u64,
    pub pass: i32,
    pub source_element: u32,
    pub source_mip: u32,
    pub target_element: u32,
    pub target_mip: u32,
    pub shader: u64,
    pub kernel: u32,
    pub groups_x: u32,
    pub groups_y: u32,
    pub groups_z: u32,
    pub bindings: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityTextureSlot {
    pub kind: u32,
    pub texture: u64,
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityPropertyRead {
    pub present: u32,
    pub float_value: f32,
    pub int_value: i32,
    pub vector_x: f32,
    pub vector_y: f32,
    pub vector_z: f32,
    pub vector_w: f32,
    pub texture_kind: u32,
    pub texture: u64,
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityDrawRendererList {
    pub cull: u64,
    pub override_material: u64,
    pub queue_min: i32,
    pub queue_max: i32,
    pub layer_mask: u32,
    pub rendering_layer_mask: u32,
    pub sorting: u32,
    pub per_object: u32,
    pub override_pass: i32,
    pub tags_len: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityShadowRendererList {
    pub sphere_x: f32,
    pub sphere_y: f32,
    pub sphere_z: f32,
    pub sphere_radius: f32,
    pub light_index: i32,
    pub plane_count: u32,
    pub objects_filter: u32,
    pub projection: u32,
    pub rendering_layer_test: u32,
    pub padding: u32,
    pub cull: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityUiDraw {
    /// Low two bits: 0 quad graphic, 1 text, 2 triangle. Bit 2 enables scene-depth testing.
    pub kind: u32,
    pub hierarchy_order: u64,
    pub clip_x: u32,
    pub clip_y: u32,
    pub clip_width: u32,
    pub clip_height: u32,
    /// Top-row-origin UV pairs in corner order for triangles; zero for quad graphics and text.
    pub triangle_u0: f32,
    pub triangle_v0: f32,
    pub triangle_u1: f32,
    pub triangle_v1: f32,
    pub triangle_u2: f32,
    pub triangle_v2: f32,
    /// Zero for white corners, one for explicit linear RGBA in BL/BR/TR/TL order.
    pub corner_colors_present: u32,
    pub corner_0_r: f32,
    pub corner_0_g: f32,
    pub corner_0_b: f32,
    pub corner_0_a: f32,
    pub corner_1_r: f32,
    pub corner_1_g: f32,
    pub corner_1_b: f32,
    pub corner_1_a: f32,
    pub corner_2_r: f32,
    pub corner_2_g: f32,
    pub corner_2_b: f32,
    pub corner_2_a: f32,
    pub corner_3_r: f32,
    pub corner_3_g: f32,
    pub corner_3_b: f32,
    pub corner_3_a: f32,
}
