use glam::Vec2;

pub(super) struct Footprint {
    pub(super) direction: Vec2,
    pub(super) count: u16,
    pub(super) lod: f32,
}

impl Footprint {
    pub(super) fn new(derivatives: [[f32; 2]; 2], dimensions: Vec2, level: u16) -> Self {
        let (axis, major, minor) = principal_axes(derivatives, dimensions);
        if major <= 1.0 {
            return Self {
                direction: axis * (major / dimensions),
                count: 1,
                lod: 0.0,
            };
        }
        let ratio = major / minor.max(1e-20);
        let count = (1..=level)
            .find(|n| f32::from(*n) >= ratio)
            .unwrap_or(level);
        let lod = (major / f32::from(count)).max(minor).max(1e-20).log2();
        Self {
            direction: axis * (major / dimensions),
            count,
            lod: lod.max(0.0),
        }
    }
}

pub(super) fn principal_axes(derivatives: [[f32; 2]; 2], dimensions: Vec2) -> (Vec2, f32, f32) {
    let [east, north] = derivatives.map(Vec2::from_array);
    let scale = east.abs().max(north.abs()).max_element();
    if scale <= 1e-20 / dimensions.max_element() {
        return (Vec2::X, 0.0, 0.0);
    }
    // Scale before the texel conversion too, so finite UV gradients cannot overflow it.
    // A power-of-two scale matches WGSL ldexp without a subnormal GPU reciprocal.
    let scale = f32::from_bits(scale.to_bits() & 0x7f80_0000);
    let east = Vec2::new(east.x / scale, east.y / scale) * dimensions;
    let north = Vec2::new(north.x / scale, north.y / scale) * dimensions;
    let texel_scale = east.abs().max(north.abs()).max_element();
    let texel_scale = f32::from_bits(texel_scale.to_bits() & 0x7f80_0000);
    let east = east / texel_scale;
    let north = north / texel_scale;
    let a = east.x * east.x + north.x * north.x;
    let b = east.x * east.y + north.x * north.y;
    let c = east.y * east.y + north.y * north.y;
    let eigenvalue = 0.5 * (a + c + Vec2::new(a - c, 2.0 * b).length());
    let major = eigenvalue.sqrt();
    let minor = east.perp_dot(north).abs() / major;
    let axis = if b == 0.0 {
        if a >= c { Vec2::X } else { Vec2::Y }
    } else {
        let angle = 0.5 * (2.0 * b).atan2(a - c);
        Vec2::new(angle.cos(), angle.sin())
    };
    // Singular lengths beyond f32's range saturate without corrupting the unit axis.
    let lengths = Vec2::new(major, minor) * texel_scale;
    let lengths = lengths * Vec2::splat(scale).min(Vec2::splat(f32::MAX) / lengths.max(Vec2::ONE));
    let lengths = lengths.min(Vec2::splat(f32::MAX));
    (axis, lengths.x, lengths.y)
}
