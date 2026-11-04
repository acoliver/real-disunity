//! Materials drawn into a target: the colour a property gives, the variant a feature selects,
//! and the clip convention of a WGSL position.

use crate::common::{COLOR, Gpu, Rig, assert_all, draw_of, full_target_triangle};

#[test]
fn a_full_target_triangle_takes_the_colour_of_the_materials_property() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    let image = rig.render(&draw).unwrap();
    assert_all(&image, COLOR);
}

#[test]
fn a_feature_selects_the_variant_that_doubles_red_and_clearing_it_restores_the_first() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    assert_all(&rig.render(&draw).unwrap(), COLOR);

    rig.runtime.set_features(rig.material, &["DOUBLE"]).unwrap();
    assert_all(&rig.render(&draw).unwrap(), [0.5, 0.5, 0.75, 1.0]);

    rig.runtime.set_features(rig.material, &[]).unwrap();
    assert_all(&rig.render(&draw).unwrap(), COLOR);
}
