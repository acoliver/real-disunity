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
    pub width: u32,
    pub height: u32,
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
