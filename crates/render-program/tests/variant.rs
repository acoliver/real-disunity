//! Variant selection: the variant this engine selects for a set of enabled keywords.

use std::collections::BTreeSet;

use worldrunner_render_program::unity::variant::{
    VariantError, declared_player_keywords, select_variant,
};
use worldrunner_unity_serialized::shader_program::PlayerSubProgram;

fn set(values: &[u16]) -> BTreeSet<u16> {
    values.iter().copied().collect()
}

fn player(blob_index: u32, keywords: &[u16]) -> PlayerSubProgram {
    PlayerSubProgram {
        blob_index,
        keyword_indices: keywords.to_vec(),
        requirements: 0,
        gpu_program_type: 0,
    }
}

#[test]
fn exact_keyword_set_selects_its_variant() {
    let variants = [(vec![], 'a'), (vec![1], 'b'), (vec![1, 2], 'c')];
    let pick = |enabled: &[u16]| {
        select_variant(&variants, |v| &v.0, &[1, 2], &set(enabled))
            .unwrap()
            .1
    };
    assert_eq!(pick(&[]), 'a');
    assert_eq!(pick(&[1]), 'b');
    assert_eq!(pick(&[1, 2]), 'c');
}

#[test]
fn keywords_the_stage_does_not_declare_are_ignored() {
    let variants = [(vec![], 'a'), (vec![1], 'b')];
    let picked = select_variant(&variants, |v| &v.0, &[1], &set(&[1, 7, 9])).unwrap();
    assert_eq!(picked.1, 'b');
    let picked = select_variant(&variants, |v| &v.0, &[1], &set(&[7])).unwrap();
    assert_eq!(picked.1, 'a');
}

#[test]
fn a_variant_order_does_not_change_the_match() {
    let variants = [(vec![2, 1], 'x')];
    let picked = select_variant(&variants, |v| &v.0, &[1, 2], &set(&[1, 2])).unwrap();
    assert_eq!(picked.1, 'x');
}

#[test]
fn a_missing_combination_is_a_named_error() {
    let variants = [(vec![1], 'b'), (vec![2], 'c')];
    let error = select_variant(&variants, |v| &v.0, &[1, 2], &set(&[1, 2])).unwrap_err();
    assert_eq!(
        error,
        VariantError::NoMatch {
            wanted: vec![1, 2],
            available: 2
        }
    );
    let none: [(Vec<u16>, char); 0] = [];
    assert!(select_variant(&none, |v| &v.0, &[], &set(&[])).is_err());
}

#[test]
fn the_first_of_identical_keyword_sets_wins() {
    let variants = [(vec![1], 'p'), (vec![1], 'q')];
    let picked = select_variant(&variants, |v| &v.0, &[1], &set(&[1])).unwrap();
    assert_eq!(picked.1, 'p');
}

#[test]
fn player_variants_declare_the_keywords_they_use() {
    let lists = [
        vec![],
        vec![player(1, &[2])],
        vec![player(2, &[]), player(3, &[4])],
    ];
    let variants: Vec<&PlayerSubProgram> = lists.iter().flatten().collect();
    let declared = declared_player_keywords(variants.iter().copied());
    assert_eq!(declared, vec![2, 4]);
    let pick = |enabled: &[u16]| {
        select_variant(&variants, |v| &v.keyword_indices, &declared, &set(enabled))
            .map(|v| v.blob_index)
    };
    assert_eq!(pick(&[2]), Ok(1));
    assert_eq!(pick(&[]), Ok(2));
    assert_eq!(pick(&[4]), Ok(3));
    assert!(pick(&[2, 4]).is_err());
}

#[test]
fn a_group_every_variant_takes_one_of_defaults_to_its_lowest_keyword() {
    // Keywords 2 and 3 are alternatives and every variant has one; 5 is optional.
    let variants = [
        (vec![3, 5], 'a'),
        (vec![3], 'b'),
        (vec![2, 5], 'c'),
        (vec![2], 'd'),
    ];
    let pick = |enabled: &[u16]| select_variant(&variants, |v| &v.0, &[2, 3, 5], &set(enabled));
    assert_eq!(pick(&[5]).unwrap().1, 'c');
    assert_eq!(pick(&[]).unwrap().1, 'd');
    assert_eq!(pick(&[3]).unwrap().1, 'b');
    assert_eq!(pick(&[3, 5]).unwrap().1, 'a');
    assert_eq!(
        pick(&[2, 3]).unwrap_err(),
        VariantError::NoMatch {
            wanted: vec![2, 3],
            available: 4
        }
    );
}

#[test]
fn a_keyword_set_with_no_group_to_default_stays_a_named_error() {
    let variants = [
        (vec![1], 'a'),
        (vec![1, 2], 'b'),
        (vec![], 'c'),
        (vec![2], 'd'),
    ];
    assert!(select_variant(&variants, |v| &v.0, &[1, 2], &set(&[1, 2])).is_ok());
    let variants = [(vec![1], 'a'), (vec![1, 2], 'b')];
    assert!(select_variant(&variants, |v| &v.0, &[1, 2], &set(&[2])).is_err());
}

#[test]
fn each_group_is_completed_on_its_own() {
    // Groups {1, 2} and {3, 4}: every variant has one of each.
    let variants = [
        (vec![2, 4], 'a'),
        (vec![2, 3], 'b'),
        (vec![1, 4], 'c'),
        (vec![1, 3], 'd'),
    ];
    let pick = |enabled: &[u16]| {
        select_variant(&variants, |v| &v.0, &[1, 2, 3, 4], &set(enabled))
            .unwrap()
            .1
    };
    assert_eq!(pick(&[]), 'd');
    assert_eq!(pick(&[2]), 'b');
    assert_eq!(pick(&[4]), 'c');
}
