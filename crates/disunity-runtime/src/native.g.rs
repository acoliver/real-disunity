// Generated from disunity-ffi ABI definitions. Do not edit.

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisunityResult {
    pub code: i32,
    pub detail: u64,
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
    pub scale: f32,
    pub sorting_order: i32,
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

/// The ABI revision these records belong to.
pub const DISUNITY_ABI_VERSION: u32 = 3;

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
