use super::*;
mod ui_font {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ui_font/mod.rs"));
}
mod positioning {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/ui_font/positioning.rs"
    ));
}
use positioning::{Feature, Lookup, Script, font, gpos, pair};

#[test]
fn resolved_subtables_follow_lookup_list_order() {
    let scripts = [Script {
        tag: *b"DFLT",
        required: Some(0),
        features: vec![1, 0, 1],
    }];
    let features = [Feature::kern(vec![1, 0, 1]), Feature::kern(vec![0])];
    let lookups = [
        Lookup::new(
            vec![pair(1, -40, false, false), pair(2, -300, false, false)],
            false,
        ),
        Lookup::pair(2, true),
    ];
    let bytes = font(
        Some(gpos(&scripts, &features, &lookups)),
        Some(positioning::kern(&[(1, 2, -50)])),
    );
    let face = Face::parse(&bytes, 0).unwrap();
    let pairs = PairPositioning::new(&face).unwrap();
    let indices: Vec<_> = pairs
        .subtables
        .as_ref()
        .unwrap()
        .iter()
        .map(|s| s.lookup)
        .collect();
    assert_eq!(indices, [0, 0, 1]);
    assert_eq!(
        pairs.storage_bytes(),
        3 * std::mem::size_of::<PairSubtable>()
    );
    assert_eq!(
        pairs.scaled(&face, 20.).x_advance(1, 2).to_bits(),
        (-2.8_f32).to_bits()
    );
}

#[test]
fn no_gpos_retains_no_subtable_storage() {
    let bytes = ui_font::font();
    let face = Face::parse(&bytes, 0).unwrap();
    let pairs = PairPositioning::new(&face).unwrap();
    assert!(pairs.subtables.is_none());
    assert_eq!(pairs.storage_bytes(), 0);
    let bytes = font(
        Some(positioning::default_gpos(&[Lookup::pair(1, false)])),
        None,
    );
    let face = Face::parse(&bytes, 0).unwrap();
    assert!(PairPositioning::new(&face).unwrap().storage_bytes() > 0);
}
