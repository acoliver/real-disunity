use glam::Vec2;

pub(super) struct Footprint {
    pub(super) direction: Vec2,
    pub(super) count: u16,
    pub(super) lod: f32,
}

impl Footprint {
    pub(super) fn new(derivatives: [[f32; 2]; 2], dimensions: Vec2, level: u16) -> Self {
        let [east, north] = derivatives.map(|d| Vec2::from_array(d) * dimensions);
        let a = east.x * east.x + north.x * north.x;
        let b = east.x * east.y + north.x * north.y;
        let c = east.y * east.y + north.y * north.y;
        let eigenvalue = 0.5 * (a + c + Vec2::new(a - c, 2.0 * b).length());
        let major = eigenvalue.sqrt();
        let minor = east.perp_dot(north).abs() / major.max(1e-20);
        let axis = if b == 0.0 {
            if a >= c { Vec2::X } else { Vec2::Y }
        } else if a >= c {
            Vec2::new(eigenvalue - c, b).normalize()
        } else {
            Vec2::new(b, eigenvalue - a).normalize()
        };
        let ratio = major / minor.max(1e-20);
        let count = (1..=level)
            .find(|n| f32::from(*n) >= ratio)
            .unwrap_or(level);
        let lod = (major / f32::from(count)).max(minor).max(1e-20).log2();
        Self {
            direction: axis * major / dimensions,
            count,
            lod,
        }
    }
}
