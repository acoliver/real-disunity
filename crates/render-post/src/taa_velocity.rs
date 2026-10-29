//! Synthetic object-motion residual for the TAA resolve tests.
//!
//! The residual is `uv(P_prev * world_prev) - uv(P_prev * world_now)` of the surface a texel
//! shows, in previous-frame uv units, so zero means camera reprojection alone. The GPU stores it
//! as `Rg16Float`; tests use values that are exact in half precision.

/// A residual per texel and sample, row-major; one plane per sample.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Velocity {
    pub size: [i32; 2],
    pub planes: Vec<Vec<[f32; 2]>>,
}
impl Velocity {
    /// `residual(x, y, sample)` for `samples` planes.
    pub fn from_fn(
        size: [i32; 2],
        samples: usize,
        mut residual: impl FnMut(i32, i32, usize) -> [f32; 2],
    ) -> Self {
        let planes = (0..samples)
            .map(|s| {
                (0..size[1])
                    .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
                    .map(|(x, y)| residual(x, y, s))
                    .collect()
            })
            .collect();
        Self { size, planes }
    }

    /// The residual of a texel (edge-clamped) in one sample.
    pub fn load(&self, at: [i32; 2], sample: usize) -> [f64; 2] {
        let x = at[0].clamp(0, self.size[0] - 1);
        let y = at[1].clamp(0, self.size[1] - 1);
        self.planes[sample][usize::try_from(y * self.size[0] + x).unwrap()].map(f64::from)
    }
}
