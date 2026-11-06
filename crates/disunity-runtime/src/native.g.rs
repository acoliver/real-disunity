// Generated from disunity-ffi ABI definitions. Do not edit.

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisunityResult {
    pub code: i32,
    pub detail: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityCamera {
    pub enabled: u32,
    pub clear_flags: u32,
    pub orthographic: u32,
    pub depth: i32,
    pub orthographic_size: f32,
    pub near: f32,
    pub far: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub field_of_view: f32,
    pub target_texture: u64,
    pub culling_mask: u32,
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub depth_texture_mode: u32,
    pub allow_hdr: u32,
    pub allow_msaa: u32,
    pub allow_dynamic_resolution: u32,
    pub use_occlusion_culling: u32,
    pub target_display: u32,
    pub camera_type: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCanvasScale {
    pub mode: u32,
    pub screen_match: u32,
    pub factor: f32,
    pub reference_x: f32,
    pub reference_y: f32,
    pub match_height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityCanvasGroup {
    pub enabled: u32,
    pub alpha: f32,
    pub interactable: u32,
    pub blocks_raycasts: u32,
    pub ignore_parent_groups: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityUiVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityUiGraphicInfo {
    pub texture: u64,
    pub vertex_count: u32,
    pub index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityCanvasRenderer {
    pub mesh: u64,
    pub texture: u64,
    pub alpha_texture: u64,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub alpha: f32,
    pub cull: u32,
    pub cull_transparent_mesh: u32,
    pub has_pop_instruction: u32,
    pub has_rect_clipping: u32,
    pub clip_x: f32,
    pub clip_y: f32,
    pub clip_width: f32,
    pub clip_height: f32,
    pub softness_x: f32,
    pub softness_y: f32,
    pub material_count: u32,
    pub pop_material_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityMeshRenderer {
    pub kind: u32,
    pub enabled: u32,
    pub force_rendering_off: u32,
    pub receive_shadows: u32,
    pub allow_occlusion_when_dynamic: u32,
    pub static_shadow_caster: u32,
    pub shadow_casting_mode: u32,
    pub motion_vector_mode: u32,
    pub light_probe_usage: u32,
    pub reflection_probe_usage: u32,
    pub rendering_layer_mask: u32,
    pub renderer_priority: i32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub material_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityDrawnMaterial {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub texture: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunitySortingGroup {
    pub enabled: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub sort_at_root: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityLight {
    pub cookie_texture: u64,
    pub enabled: u32,
    pub kind: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub intensity: f32,
    pub range: f32,
    pub spot_angle: f32,
    pub inner_spot_angle: f32,
    pub shadows: u32,
    pub shadow_strength: f32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_near_plane: f32,
    pub culling_mask: u32,
    pub rendering_layer_mask: u32,
    pub bounce_intensity: f32,
    pub color_temperature: f32,
    pub use_color_temperature: u32,
    pub cookie_size: f32,
    pub lightmap_bake_type: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunitySprite {
    pub identity: u64,
    pub texture: u64,
    pub present: u32,
    pub width: u32,
    pub height: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub pixels_per_unit: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub enabled: u32,
    pub flip_x: u32,
    pub flip_y: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub lighting: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityMatrix {
    pub m0: f32,
    pub m1: f32,
    pub m2: f32,
    pub m3: f32,
    pub m4: f32,
    pub m5: f32,
    pub m6: f32,
    pub m7: f32,
    pub m8: f32,
    pub m9: f32,
    pub m10: f32,
    pub m11: f32,
    pub m12: f32,
    pub m13: f32,
    pub m14: f32,
    pub m15: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityPresentation {
    pub present: u32,
    pub quads: u32,
    pub textures: u32,
    /// How many mesh renderers the frame shows; read them with `disunity_presentation_mesh_object`.
    pub mesh_objects: u32,
    pub width: u32,
    pub height: u32,
    /// 0 for an orthographic lens, 1 for a perspective lens.
    pub lens: u32,
    pub eye_x: f64,
    pub eye_y: f64,
    pub eye_z: f64,
    pub forward_x: f64,
    pub forward_y: f64,
    pub forward_z: f64,
    pub up_x: f64,
    pub up_y: f64,
    pub up_z: f64,
    pub half_height: f64,
    pub near: f64,
    pub far: f64,
    /// Vertical field of view in radians; zero for an orthographic lens.
    pub vertical_fov: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityQuad {
    pub texture: u64,
    pub textured: u32,
    pub overlay: u32,
    pub lighting: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub p0_x: f32,
    pub p0_y: f32,
    pub p0_z: f32,
    pub p1_x: f32,
    pub p1_y: f32,
    pub p1_z: f32,
    pub p2_x: f32,
    pub p2_y: f32,
    pub p2_z: f32,
    pub p3_x: f32,
    pub p3_y: f32,
    pub p3_z: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCoverage {
    pub identity: u64,
    pub width: u32,
    pub height: u32,
    pub length: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityMeshObject {
    pub object: u64,
    pub mesh: u64,
    pub slot_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityCommand {
    pub object: u64,
    pub kind: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityObjectState {
    pub object: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y_radians: f32,
    pub material_slot: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityAdvance {
    /// 0: frame complete, 1: callback, 2: lifecycle complete.
    pub kind: u32,
    pub message: u32,
    pub ticket: u64,
    pub script: u64,
    pub frame: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityClock {
    pub delta: f64,
    pub time: f64,
    pub frame: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTime {
    pub unscaled_delta: f64,
    pub unscaled_time: f64,
    pub smooth_delta: f64,
    pub time_scale: f64,
    pub maximum_delta: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityBuffer {
    pub handle: u64,
    pub length: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCloneRow {
    pub source: u64,
    pub target: u64,
    pub owner: u64,
    /// 0: object, 1: managed script, 2: native component.
    pub kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCallback {
    pub ticket: u64,
    pub script: u64,
    pub routine: u64,
    pub collider: u64,
    pub other: u64,
    pub phase: u32,
    pub trigger: u32,
    pub input_phase: u32,
    pub point_x: f32,
    pub point_y: f32,
    pub normal_x: f32,
    pub normal_y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityInputAxis {
    pub negative: u32,
    pub positive: u32,
    pub alt_negative: u32,
    pub alt_positive: u32,
    pub gravity: f32,
    pub sensitivity: f32,
    pub dead: f32,
    /// Bit 0: snap; bit 1: invert.
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityBody2d {
    pub kind: u32,
    pub enabled: u32,
    pub mass: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub angular_velocity: f32,
    pub gravity_scale: f32,
    pub freeze_rotation: u32,
    pub freeze_position: u32,
    pub restitution: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCollider2d {
    pub kind: u32,
    pub enabled: u32,
    pub trigger: u32,
    pub layers: u32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityRayHit {
    pub collider: u64,
    pub point_x: f32,
    pub point_y: f32,
    pub normal_x: f32,
    pub normal_y: f32,
    pub distance: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityPhysicsMaterial2d {
    pub friction: f32,
    pub bounciness: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub position_x: f32,
    pub position_y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCanvas {
    pub mode: u32,
    pub enabled: u32,
    pub camera: u64,
    pub orthographic_size: f32,
    pub plane_distance: f32,
    pub sorting_order: i32,
    pub sorting_layer: i32,
    pub override_sorting: u32,
    pub target_display: u32,
    pub additional_shader_channels: u32,
    pub reference_pixels_per_unit: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTransform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub qx: f32,
    pub qy: f32,
    pub qz: f32,
    pub qw: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTextStyle {
    pub font: u64,
    pub alignment: u32,
    pub horizontal_overflow: u32,
    pub vertical_overflow: u32,
    pub font_style: u32,
    pub rich_text: u32,
    pub best_fit: u32,
    pub line_spacing: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTilemap {
    pub enabled: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub cell_x: f32,
    pub cell_y: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTile {
    pub collider: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub m00: f32,
    pub m01: f32,
    pub m10: f32,
    pub m11: f32,
    pub tx: f32,
    pub ty: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTileCell {
    pub x: i32,
    pub y: i32,
    pub tile: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityTextureInfo {
    pub identity: u64,
    /// Starts at 1 and rises by one each time the identity is stored again.
    pub generation: u64,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    /// Nonzero when the texels are linear data rather than sRGB colour.
    pub linear: u32,
    pub length: u32,
    pub present: u32,
    pub distance_spread: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunitySampler {
    /// Starts at 1 when the texture is first stored and rises each time a write changes a value.
    pub generation: u64,
    pub wrap_u: u32,
    pub wrap_v: u32,
    pub wrap_w: u32,
    pub filter: u32,
    pub aniso_level: u32,
    pub present: u32,
    pub changed: u32,
    pub mip_bias: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshInfo {
    pub identity: u64,
    /// Starts at 1 and rises by one each time the identity is stored again.
    pub generation: u64,
    /// Zero when a read names an identity nothing stored.
    pub present: u32,
    /// Nonzero when the stored generation is newer than the one the read passed.
    pub changed: u32,
    /// Nonzero when the read copied the buffers.
    pub copied: u32,
    pub channels: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub submesh_count: u32,
    pub left_out: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityMeshVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub nx: f32,
    pub ny: f32,
    pub nz: f32,
    pub u: f32,
    pub v: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshSubMesh {
    pub slot: u32,
    pub first_index: u32,
    pub index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshChannelsInfo {
    pub identity: u64,
    /// The generation of the fixed mesh the channels were stored for, or one more than the held
    /// channels' when they are stored again without the fixed mesh changing.
    pub generation: u64,
    /// Zero when a read names an identity with no stored channels.
    pub present: u32,
    /// Nonzero when the stored generation is newer than the one the read passed.
    pub changed: u32,
    /// Nonzero when the read copied the buffers.
    pub copied: u32,
    pub vertex_count: u32,
    pub channel_count: u32,
    pub float_count: u32,
    pub index_count: u32,
    pub range_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshChannel {
    pub number: u32,
    pub dimension: u32,
    pub offset: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshRange {
    pub start: u32,
    pub count: u32,
    pub base_vertex: u32,
    pub topology: u32,
}

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
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityUiDraw {
    /// 0: image/solid graphic; 1: text sampling prepared R8 coverage.
    pub kind: u32,
    pub hierarchy_order: u64,
    pub clip_x: u32,
    pub clip_y: u32,
    pub clip_width: u32,
    pub clip_height: u32,
}

/// The ABI revision these records belong to.
pub const DISUNITY_ABI_VERSION: u32 = 7;

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisunityErrorReason {
    InvalidArgument = 1,
    InvalidTime = 2,
    WrongCallback = 3,
    CallbackFailed = 4,
    Faulted = 5,
    AbortedScope = 6,
    Capacity = 7,
    SingularParent = 8,
    Physics = 9,
    StaleHandle = 10,
    AdvanceInProgress = 11,
    InvalidComponent = 12,
    Ui = 13,
    NotReady = 14,
}
