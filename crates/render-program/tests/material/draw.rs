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
