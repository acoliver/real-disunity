use super::{composition::*, mips::*, selection::*};
use worldrunner_render_plan::{TerrainPageSettings, TerrainPageSettingsParts};

fn settings(budget: u32) -> TerrainPageSettings {
    TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [3, 5],
        border: 1,
        filter_radius: 1,
        maximum_depth: 2,
        split_threshold_millipixels: 2_000,
        merge_threshold_millipixels: 1_000,
        maximum_selected_pages: budget,
        resident_bytes: 100_000,
        upload_bytes_per_frame: 100_000,
    })
    .unwrap()
}

#[test]
fn page_selection_covers_rectangular_topology_without_fixed_intervals() {
    for intervals in [[7, 13], [128, 32]] {
        let topology = PageTopology::new(intervals).unwrap();
        assert_eq!(topology.intervals(), intervals);
        let selected = select_pages(settings(16), None, |_| 3_000);
        assert_eq!(selected.leaves().len(), 16);
        assert!(selected.leaves().iter().all(|key| key.depth() == 2));
        let area: f64 = selected
            .leaves()
            .iter()
            .map(|key| 0.25_f64.powi(i32::from(key.depth())))
            .sum();
        assert_eq!(area.to_bits(), 1.0_f64.to_bits());
    }
    assert!(PageTopology::new([0, 4]).is_err());
    assert!(PageKey::new(2, [4, 0]).is_err());
    assert!(PageKey::new(17, [0, 0]).is_err());
}

#[test]
fn terrain_pages_hysteresis_retains_split_until_merge_threshold() {
    let fine = select_pages(settings(4), None, |_| 2_001);
    assert_eq!(fine.leaves().len(), 4);
    let held = select_pages(settings(4), Some(&fine), |_| 1_500);
    assert_eq!(held, fine);
    let coarse = select_pages(settings(4), Some(&held), |_| 999);
    assert_eq!(coarse.leaves(), &[PageKey::ROOT]);
    assert_eq!(
        select_pages(settings(3), None, |_| u32::MAX).leaves(),
        &[PageKey::ROOT]
    );
}

#[test]
fn terrain_pages_neighbors_and_ancestor_coordinates_are_exact() {
    let key = PageKey::new(2, [1, 2]).unwrap();
    assert_eq!(
        key.neighbor([1, 0]).unwrap(),
        PageKey::new(2, [2, 2]).unwrap()
    );
    assert_eq!(key.ancestor(1).unwrap(), PageKey::new(1, [0, 1]).unwrap());
    assert_eq!(
        key.uv_in_ancestor([0.5, 0.5], PageKey::ROOT)
            .unwrap()
            .map(f64::to_bits),
        [0.375_f64, 0.625_f64].map(f64::to_bits)
    );
    assert!(PageKey::ROOT.neighbor([-1, 0]).is_none());
    assert!(
        key.uv_in_ancestor([0.5, 0.5], PageKey::new(1, [1, 1]).unwrap())
            .is_err()
    );
}

fn layer(color: [u8; 4], roughness: u8, weight: u16) -> LayerSample {
    LayerSample {
        color,
        normal: [0, 0, i16::MAX],
        roughness,
        weight,
    }
}

#[test]
fn terrain_pages_compose_linear_color_normal_and_roughness_exactly() {
    let texel = compose_texel(&[
        layer([0, 64, 128, 255], 0, 1),
        layer([200, 128, 0, 255], 200, 3),
    ])
    .unwrap();
    assert_eq!(texel.color, [150, 112, 32, 255]);
    assert_eq!(texel.normal_roughness, [128, 128, 255, 150]);
    assert!(compose_texel(&[]).is_err());
    assert!(compose_texel(&[layer([0; 4], 0, 0)]).is_err());
    assert!(
        compose_texel(&[LayerSample {
            normal: [0; 3],
            ..layer([0; 4], 0, 1)
        }])
        .is_err()
    );
}

#[test]
fn terrain_pages_compose_gutters_from_global_source_coordinates() {
    let key = PageKey::new(1, [1, 0]).unwrap();
    let mut coordinates = Vec::new();
    let image = compose_page(settings(4), key, |uv| {
        coordinates.push(uv);
        vec![layer([24, 48, 96, 255], 100, 1)]
    })
    .unwrap();
    assert_eq!(image.extent(), [5, 7]);
    assert!(coordinates[0][0] < 0.5);
    assert!(coordinates.last().unwrap()[0] > 1.0);
    assert!(coordinates[0][1] < 0.0);
    assert_eq!(image.texels().len(), 35);
    assert!(
        image
            .texels()
            .iter()
            .all(|texel| texel.color == [24, 48, 96, 255])
    );
}

#[test]
fn terrain_pages_mips_include_odd_edges_and_preserve_constant_products() {
    let constant = compose_page(settings(4), PageKey::ROOT, |_| {
        vec![layer([16, 32, 64, 255], 80, 1)]
    })
    .unwrap();
    let mips = build_mips(constant).unwrap();
    assert_eq!(
        mips.iter().map(MaterialImage::extent).collect::<Vec<_>>(),
        vec![[5, 7], [2, 3], [1, 1]]
    );
    for mip in mips {
        assert!(
            mip.texels()
                .iter()
                .all(|texel| texel.color == [16, 32, 64, 255])
        );
        assert!(
            mip.texels()
                .iter()
                .all(|texel| texel.normal_roughness == [128, 128, 255, 80])
        );
    }
    let texels = [0, 0, 255].map(|value| MaterialTexel {
        color: [value; 4],
        normal_roughness: [128, 128, 255, value],
    });
    let odd = MaterialImage::new([3, 1], texels.to_vec()).unwrap();
    assert_eq!(build_mips(odd).unwrap()[1].texels()[0].color, [85; 4]);
    assert!(MaterialImage::new([2, 2], texels.to_vec()).is_err());
}
