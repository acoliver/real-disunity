use super::{composition::*, gpu::CompositionBatch, mips::build_mips, selection::*};
use worldrunner_render_plan::{TerrainPageSettings, TerrainPageSettingsParts};

fn sample(normal: [i16; 3], weight: u16) -> LayerSample {
    LayerSample {
        color: [255; 4],
        normal,
        roughness: 255,
        weight,
    }
}

#[test]
fn terrain_pages_normal_blending_normalizes_inputs_and_rejects_cancellation() {
    let texel = compose_texel(&[sample([1, 0, 0], 1), sample([0, 0, i16::MAX], 1)]).unwrap();
    assert_eq!(texel.normal_roughness, [218, 128, 218, 255]);
    assert!(compose_texel(&[sample([1, 0, 0], 1), sample([-1, 0, 0], 1)]).is_err());
    assert!(compose_texel(&[sample([0; 3], 0), sample([0, 0, 1], 1)]).is_ok());
}

#[test]
fn terrain_pages_maximum_layer_weights_do_not_overflow() {
    let layers = vec![sample([0, 0, 1], u16::MAX); 256];
    assert_eq!(compose_texel(&layers).unwrap().color, [255; 4]);
    assert!(CompositionBatch::new(std::slice::from_ref(&layers)).is_ok());
    let mut oversized = layers;
    oversized.push(sample([0, 0, 1], 1));
    assert!(CompositionBatch::new(&[oversized]).is_err());
    assert!(CompositionBatch::new(&[]).is_err());
}

#[test]
fn terrain_pages_fractional_odd_mip_bins_use_exact_area_weights() {
    let texels = [0, 0, 100, 200, 200].map(|value| MaterialTexel {
        color: [value; 4],
        normal_roughness: [128, 128, 255, value],
    });
    let levels = build_mips(MaterialImage::new([5, 1], texels.to_vec()).unwrap()).unwrap();
    assert_eq!(levels[1].texels()[0].color, [20; 4]);
    assert_eq!(levels[1].texels()[1].color, [180; 4]);
    assert_eq!(levels[2].texels()[0].color, [100; 4]);
}

#[test]
fn terrain_pages_budget_priority_is_stable_and_never_overlaps() {
    let settings = TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [4, 4],
        border: 2,
        filter_radius: 1,
        maximum_depth: 3,
        split_threshold_millipixels: 2_000,
        merge_threshold_millipixels: 1_000,
        maximum_selected_pages: 7,
        resident_bytes: 100_000,
        upload_bytes_per_frame: 100_000,
    })
    .unwrap();
    let pages = select_pages(settings, None, |_| 3_000);
    assert_eq!(pages.leaves().len(), 7);
    assert!(!pages.leaves().contains(&PageKey::new(1, [0, 0]).unwrap()));
    for (index, page) in pages.leaves().iter().enumerate() {
        for other in pages.leaves().iter().skip(index + 1) {
            assert_ne!(other.ancestor(page.depth()), Some(*page));
        }
    }
    assert_eq!(pages, select_pages(settings, Some(&pages), |_| 3_000));
}
