use super::scene::Scene;
use worldrunner_render_post::{FrameConfig, Readback};
use worldrunner_render_scene::{compositor::FeatureTarget, device::GpuContext, wgpu};

const TOLERANCE_UV: f32 = 1e-3;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Products {
    color: Vec<u8>,
    output: Vec<u8>,
    normals: Vec<u8>,
    velocity: Vec<u8>,
    confidence: Vec<u8>,
}

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        16 * 1024 * 1024,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

fn halves(bytes: &[u8]) -> Vec<f32> {
    assert_eq!(bytes.len() % 2, 0);
    bytes
        .chunks_exact(2)
        .map(|word| half::f16::from_bits(u16::from_le_bytes([word[0], word[1]])).to_f32())
        .collect()
}

impl Products {
    pub(super) fn read(context: &GpuContext, scene: &Scene) -> Self {
        let frame = &scene.frame;
        let normals = frame.feature_target(FeatureTarget::Normals).unwrap();
        let velocity = frame.targets().velocity_texture().unwrap();
        let confidence = frame.feature_capture(FeatureTarget::SsrConfidence).unwrap();
        let pixels = u64::from(normals.width()) * u64::from(normals.height());
        assert_eq!(
            [normals.width(), normals.height()],
            [velocity.width(), velocity.height()]
        );
        let normal_bytes = read(context, normals);
        let velocity_bytes = read(context, velocity);
        assert_eq!(u64::try_from(normal_bytes.len()).unwrap(), pixels * 8);
        assert_eq!(u64::try_from(velocity_bytes.len()).unwrap(), pixels * 4);
        Self {
            color: read(context, frame.targets().scene().texture()),
            output: read(context, &scene.output),
            normals: normal_bytes,
            velocity: velocity_bytes,
            confidence: read(context, confidence.texture),
        }
    }

    /// What only the refused suffix would have written is as the completed frame left it.
    pub(super) fn assert_completed_unchanged(&self, completed: &Self) {
        assert_eq!(self.color, completed.color, "completed raw scene colour");
        assert_eq!(self.output, completed.output, "completed final AA output");
        assert_eq!(
            self.velocity, completed.velocity,
            "completed motion: the forward Opaque pass of the refused suffix writes velocity"
        );
        assert_eq!(
            self.confidence, completed.confidence,
            "completed confidence"
        );
    }

    pub(super) fn assert_prefix_matches(&self, retried: &Self) {
        assert_eq!(
            self.normals, retried.normals,
            "refused prefix coverage matches retry"
        );
    }

    pub(super) fn assert_confidence(&self, expected: f32) {
        let values = halves(&self.confidence);
        assert!(!values.is_empty());
        assert!(
            values
                .chunks_exact(4)
                .all(|pixel| pixel[3].to_bits() == expected.to_bits())
        );
    }

    pub(super) fn assert_instance_motion(&self, initial: &Self) {
        let before = halves(&initial.velocity);
        let after = halves(&self.velocity);
        let (mut moving, mut largest) = (0, 0.0_f32);
        for (index, covered) in self.coverage().into_iter().enumerate() {
            if !covered {
                continue;
            }
            let offset = index * 2;
            let residual = [after[offset], after[offset + 1]];
            assert!(residual.iter().all(|value| value.is_finite()));
            let change = (residual[0] - before[offset])
                .abs()
                .max((residual[1] - before[offset + 1]).abs());
            if change > 1e-6 {
                moving += 1;
                largest = largest.max(change);
            }
        }
        assert!(
            moving >= 200,
            "only {moving} covered texels changed velocity"
        );
        assert!(
            largest > 0.01,
            "the retained transform residual was lost: {largest}"
        );
    }

    /// Whether the prepass covered each texel: the metadata normal's alpha.
    fn coverage(&self) -> Vec<bool> {
        halves(&self.normals)
            .chunks_exact(4)
            .map(|normal| normal[3] != 0.0)
            .collect()
    }

    pub(super) fn assert_motion_settled(&self) {
        let covered = self
            .coverage()
            .into_iter()
            .filter(|covered| *covered)
            .count();
        assert!(
            covered >= 200,
            "settled frame lost stored geometry: {covered}"
        );
        let velocity = halves(&self.velocity);
        assert!(velocity.iter().all(|value| value.is_finite()));
        assert!(
            velocity.iter().all(|value| value.abs() < TOLERANCE_UV),
            "unchanged completed frame retained instance motion"
        );
    }
}
