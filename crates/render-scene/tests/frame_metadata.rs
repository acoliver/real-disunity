use worldrunner_render_scene::executor::ResourceRef;
use worldrunner_render_scene::frame::metadata::{FrameProduct, MetadataMeaning, MetadataSampling};
use worldrunner_render_scene::wgpu::TextureFormat;

#[test]
fn metadata_rejects_incompatible_sample_and_format_combinations() {
    for samples in [1, 4] {
        assert!(
            FrameProduct::new(
                ResourceRef::new("test depth"),
                MetadataMeaning::DeviceDepth,
                TextureFormat::Depth32Float,
                MetadataSampling::PerSample(samples)
            )
            .is_ok()
        );
        assert!(
            FrameProduct::new(
                ResourceRef::new("test normal"),
                MetadataMeaning::ViewNormalCoverage,
                TextureFormat::Rgba16Float,
                MetadataSampling::PerSample(samples)
            )
            .is_ok()
        );
    }
    for samples in [0, 2, 8] {
        assert!(
            FrameProduct::new(
                ResourceRef::new("test"),
                MetadataMeaning::DeviceDepth,
                TextureFormat::Depth32Float,
                MetadataSampling::PerSample(samples)
            )
            .is_err()
        );
    }
    assert!(
        FrameProduct::new(
            ResourceRef::new("test"),
            MetadataMeaning::DeviceDepth,
            TextureFormat::R32Float,
            MetadataSampling::PixelCenter
        )
        .is_err()
    );
    assert!(
        FrameProduct::new(
            ResourceRef::new("test"),
            MetadataMeaning::EnvironmentSpecularLinear,
            TextureFormat::Rgba8UnormSrgb,
            MetadataSampling::PixelCenter
        )
        .is_err()
    );
    assert!(
        FrameProduct::new(
            ResourceRef::new("test"),
            MetadataMeaning::ConservativeDepthMeters,
            TextureFormat::R32Float,
            MetadataSampling::PerSample(4)
        )
        .is_err()
    );
}

#[test]
fn shared_products_keep_executor_identity_and_numerical_meaning() {
    let product = FrameProduct::prepass_depth();
    assert_eq!(product.resource(), ResourceRef::new("prepass depth"));
    assert_eq!(product.sampling(), MetadataSampling::PixelCenter);
    assert_ne!(
        FrameProduct::linear_depth(0).meaning(),
        MetadataMeaning::AverageDepthNormalMeters
    );
    assert_eq!(FrameProduct::linear_depth(3).resource().version, 3);
    assert_eq!(
        FrameProduct::material().resource(),
        ResourceRef::new("reflection material")
    );
    assert_eq!(
        FrameProduct::final_depth(4).resource(),
        ResourceRef::new("post-water depth")
    );
    assert_eq!(
        FrameProduct::specular().meaning(),
        MetadataMeaning::EnvironmentSpecularLinear
    );
}

#[test]
fn conservative_forward_resolution_requires_legal_sources_and_depth_meaning() {
    for source_samples in [1, 4] {
        let sampling = MetadataSampling::ConservativeResolve(source_samples);
        assert_eq!(sampling.samples(), 1);
        assert!(
            FrameProduct::new(
                ResourceRef::new("resolved depth"),
                MetadataMeaning::ConservativeDepthMeters,
                TextureFormat::R32Float,
                sampling,
            )
            .is_ok()
        );
        for (meaning, format) in [
            (MetadataMeaning::DeviceDepth, TextureFormat::Depth32Float),
            (
                MetadataMeaning::ViewNormalCoverage,
                TextureFormat::Rgba16Float,
            ),
            (
                MetadataMeaning::MaterialF0Roughness,
                TextureFormat::Rgba16Float,
            ),
            (
                MetadataMeaning::EnvironmentSpecularLinear,
                TextureFormat::Rgba16Float,
            ),
            (
                MetadataMeaning::AverageDepthNormalMeters,
                TextureFormat::Rgba16Float,
            ),
        ] {
            assert!(
                FrameProduct::new(
                    ResourceRef::new("invalid reduction"),
                    meaning,
                    format,
                    sampling
                )
                .is_err()
            );
        }
    }
    for source_samples in [0, 2, 8] {
        assert!(
            FrameProduct::new(
                ResourceRef::new("invalid source count"),
                MetadataMeaning::ConservativeDepthMeters,
                TextureFormat::R32Float,
                MetadataSampling::ConservativeResolve(source_samples),
            )
            .is_err()
        );
    }
}
