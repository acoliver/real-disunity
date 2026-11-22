//! The report names the texture globals a frame set that no draw looked up. The camera shaders read
//! `_Albedo`; a global set as `Albedo` is a name no slot asks for, so it is listed, and a global
//! set as `_Albedo` is looked up by the draw and is not.

use std::collections::BTreeSet;

use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue as WorldValue, TextureRef,
};

use super::stand_ins::{camera_scene, gamma_rig};
use super::support::{set_global_in_earlier_take, texture_slot, world};

fn global(name: &str) -> WorldValue {
    WorldValue::Texture(name.into(), texture_slot(TextureRef::None))
}

fn names(entries: &[&str]) -> BTreeSet<String> {
    entries.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_global_the_world_holds_under_a_name_no_slot_asks_for_is_reported_unread() {
    let mut rig = gamma_rig();
    let mut world = world();
    set_global_in_earlier_take(&mut world, global("Albedo"));
    set_global_in_earlier_take(&mut world, global("_Albedo"));
    camera_scene(&mut world, "camera", None);
    let (adapted, _operations, _reads) = rig.frame(&mut world, &[]);
    let reads = &adapted.report.global_reads;
    assert_eq!(reads.set, names(&["Albedo", "_Albedo"]));
    assert_eq!(reads.unread(), names(&["Albedo"]));
}

#[test]
fn a_global_a_command_sets_in_the_frame_is_reported_unread_by_the_same_rule() {
    let mut rig = gamma_rig();
    let mut world = world();
    for name in ["Albedo", "_Albedo"] {
        world
            .record_offscreen(OffscreenWork::SetGlobal(global(name)))
            .unwrap();
    }
    camera_scene(&mut world, "camera", None);
    let (adapted, _operations, _reads) = rig.frame(&mut world, &[]);
    let reads = &adapted.report.global_reads;
    assert_eq!(reads.set, names(&["Albedo", "_Albedo"]));
    assert_eq!(reads.unread(), names(&["Albedo"]));
}
