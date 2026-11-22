//! Old World's `_3DNoiseTexture` is a 3D slot of a shader program. Nothing sets it and the shader
//! does not declare it, so the frame's grey volume stands in and the report counts the draw by
//! the property.

use std::collections::BTreeMap;

use worldrunner_world_simulation::graphics::{OffscreenWork, TargetRef, TextureRef};

use super::gpu::{Rig, SIDE, shader, target};
use super::support::world;

#[test]
fn a_blit_whose_3d_slot_nothing_sets_reads_the_grey_volume_and_is_counted_by_property() {
    let mut rig = Rig::new();
    let mut world = world();
    let (source, out) = (target(&mut world, 8), target(&mut world, 8));
    let volume_shader = shader(&mut world, "volumeslot");
    let material = world.create_material(volume_shader).unwrap();
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(out),
            material: Some(material),
            pass: 0,
        })
        .unwrap();
    let (adapted, operations, reads) = rig.frame(&mut world, &[out]);
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    assert_eq!(
        adapted.report.stand_ins.by_property,
        BTreeMap::from([("_3DNoiseTexture undeclared".to_owned(), 1)])
    );
    assert_eq!(adapted.report.missing_textures, BTreeMap::new());
    rig.run(operations);
    let bytes = reads.into_iter().next().unwrap().bytes(&rig.device);
    assert_eq!(bytes.len(), (SIDE * SIDE * 4) as usize);
    for texel in bytes.chunks_exact(4) {
        assert!(
            texel[..3].iter().all(|&c| c.abs_diff(128) <= 1) && texel[3] == 255,
            "{texel:?}"
        );
    }
}
