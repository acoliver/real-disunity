//! Variant selection: the variant Unity would pick for a set of enabled keywords.

use std::collections::BTreeSet;

use worldrunner_render_program::variant::{
    VariantError, declared_keywords, select_player_sub_program, select_sub_program, select_variant,
};
use worldrunner_unity_serialized::shader_program::{
    BindChannels, PlayerSubProgram, Program, ProgramParameters, SubProgram,
};

fn set(values: &[u16]) -> BTreeSet<u16> {
    values.iter().copied().collect()
}

fn sub(blob_index: u32, keywords: &[u16]) -> SubProgram {
    SubProgram {
        blob_index,
        channels: BindChannels {
            channels: Vec::new(),
            source_map: 0,
        },
        keyword_indices: keywords.to_vec(),
        hardware_tier: 0,
        gpu_program_type: 0,
        parameters: ProgramParameters::default(),
        requirements: 0,
    }
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
fn the_state_mask_declares_keywords_or_else_the_variants_do() {
    let mut program = Program {
        sub_programs: vec![sub(10, &[]), sub(11, &[3]), sub(12, &[3, 5])],
        ..Program::default()
    };
    assert_eq!(declared_keywords(&program), vec![3, 5]);
    program.keyword_state_mask = vec![3];
    assert_eq!(declared_keywords(&program), vec![3]);
}

#[test]
fn sub_programs_are_chosen_over_the_declared_keywords() {
    let program = Program {
        sub_programs: vec![sub(10, &[]), sub(11, &[3]), sub(12, &[3, 5])],
        ..Program::default()
    };
    assert_eq!(
        select_sub_program(&program, &set(&[3, 5]))
            .unwrap()
            .blob_index,
        12
    );
    assert_eq!(
        select_sub_program(&program, &set(&[4])).unwrap().blob_index,
        10
    );
    assert!(select_sub_program(&program, &set(&[5])).is_err());
}

#[test]
fn player_sub_programs_are_searched_across_platform_lists() {
    let program = Program {
        player_sub_programs: vec![
            vec![],
            vec![player(1, &[2])],
            vec![player(2, &[]), player(3, &[4])],
        ],
        ..Program::default()
    };
    assert_eq!(
        select_player_sub_program(&program, &set(&[2]))
            .unwrap()
            .blob_index,
        1
    );
    assert_eq!(
        select_player_sub_program(&program, &set(&[]))
            .unwrap()
            .blob_index,
        2
    );
    assert_eq!(
        select_player_sub_program(&program, &set(&[4]))
            .unwrap()
            .blob_index,
        3
    );
    assert!(select_player_sub_program(&program, &set(&[2, 4])).is_err());
}
