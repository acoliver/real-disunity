/// A 3D light of a player scene, as the managed `Light` takes it. The engine draws no 3D
/// lights, so the values are state that scripts read back.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Light3D {
    /// The `LightType` member name: Spot, Directional, Point, Rectangle or Disc.
    pub kind: String,
    /// RGBA color as authored.
    pub color: [f64; 4],
    pub intensity: f64,
    pub range: f64,
    pub spot_angle: f64,
    pub inner_spot_angle: f64,
    /// The `LightShadows` member name: None, Hard or Soft.
    pub shadows: String,
    pub shadow_strength: f64,
    pub shadow_bias: f64,
    pub shadow_normal_bias: f64,
    pub shadow_near_plane: f64,
    pub culling_mask: u32,
    pub rendering_layer_mask: u32,
    pub bounce_intensity: f64,
    pub color_temperature: f64,
    pub use_color_temperature: bool,
    pub cookie_size: f64,
    /// The value of the managed `LightmapBakeType`.
    pub lightmap_bake_type: i64,
}
