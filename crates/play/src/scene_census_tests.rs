use super::*;

fn counted<T: Ord + Copy>(entries: &[(T, usize)]) -> BTreeMap<T, usize> {
    entries.iter().copied().collect()
}

#[test]
fn the_labels_are_the_stable_kebab_case_names_of_the_reasons() {
    let white = [
        (WhiteReason::NoMaterialSlot, "no-material-slot"),
        (WhiteReason::TextureZero, "texture-zero"),
        (WhiteReason::TexelsNotStored, "texels-not-stored"),
        (WhiteReason::ConversionFailed, "conversion-failed"),
        (WhiteReason::HandleOverflow, "handle-overflow"),
    ];
    for (reason, label) in white {
        assert_eq!(reason.label(), label);
    }
    assert_eq!(LeftOut::MeshNotArrived.label(), "mesh-not-arrived");
    assert_eq!(LeftOut::MeshHandleOverflow.label(), "mesh-handle-overflow");
}

#[test]
fn a_census_without_white_renderers_gives_only_the_total_line() {
    let census = SubstitutionCensus {
        drawn: 12,
        ..SubstitutionCensus::default()
    };
    assert_eq!(
        census.lines(),
        ["retained renderers with texture 0 (last frame) 0 of 12"]
    );
}

#[test]
fn the_lines_rank_reasons_largest_first_and_equal_counts_by_name() {
    let census = SubstitutionCensus {
        drawn: 40,
        last_frame: counted(&[
            (WhiteReason::TextureZero, 5),
            (WhiteReason::TexelsNotStored, 9),
            (WhiteReason::NoMaterialSlot, 5),
        ]),
        most_in_one_frame: counted(&[
            (WhiteReason::TexelsNotStored, 30),
            (WhiteReason::TextureZero, 5),
        ]),
        left_out: counted(&[
            (LeftOut::MeshHandleOverflow, 1),
            (LeftOut::MeshNotArrived, 4),
        ]),
        ..SubstitutionCensus::default()
    };
    assert_eq!(
        census.lines(),
        [
            "retained renderers with texture 0 (last frame) 19 of 40",
            "retained renderers with texture 0 by reason (last frame): texels-not-stored 9, no-material-slot 5, texture-zero 5",
            "retained renderers with texture 0 by reason (most in one frame): texels-not-stored 30, texture-zero 5",
            "retained objects left out: mesh-not-arrived 4, mesh-handle-overflow 1",
        ]
    );
}

#[test]
fn the_ui_draws_left_out_for_a_game_material_have_one_line_ranked_by_shader() {
    let census = SubstitutionCensus {
        drawn: 3,
        ui_game_materials: BTreeMap::from([
            ("Game/Outline".to_owned(), 2),
            ("Game/Silhouette".to_owned(), 7),
        ]),
        ..SubstitutionCensus::default()
    };
    assert_eq!(
        census.lines().last().unwrap(),
        "UI draws with a game material by shader (last frame): Game/Silhouette 7, Game/Outline 2"
    );
    assert_eq!(census.lines().len(), 2);
}

#[test]
fn a_frame_replaces_the_last_frame_and_raises_the_worst_frame_only_where_it_was_worse() {
    let mut census = SubstitutionCensus::default();
    let mut first = FrameCensus::default();
    for white in [
        Some(WhiteReason::TextureZero),
        Some(WhiteReason::TextureZero),
        None,
    ] {
        first.drawn(white);
    }
    first.left_out(LeftOut::MeshNotArrived);
    census.end_frame(first);
    let mut second = FrameCensus::default();
    second.drawn(Some(WhiteReason::TextureZero));
    second.drawn(Some(WhiteReason::ConversionFailed));
    census.end_frame(second);
    assert_eq!(census.drawn, 2);
    assert_eq!(
        census.last_frame,
        counted(&[
            (WhiteReason::TextureZero, 1),
            (WhiteReason::ConversionFailed, 1)
        ])
    );
    assert_eq!(
        census.most_in_one_frame,
        counted(&[
            (WhiteReason::TextureZero, 2),
            (WhiteReason::ConversionFailed, 1)
        ])
    );
    assert!(census.left_out.is_empty());
}

#[test]
fn slots_after_the_first_are_counted_and_named_in_the_report_when_there_are_any() {
    let mut frame = FrameCensus::default();
    frame.drawn(None);
    frame.slots_not_drawn(2);
    frame.drawn(None);
    frame.slots_not_drawn(1);
    let mut census = SubstitutionCensus::default();
    census.end_frame(frame);
    assert_eq!(census.slots_not_drawn, 3);
    assert_eq!(
        census.lines().last().map(String::as_str),
        Some("retained renderer material slots not drawn (last frame) 3")
    );
    census.end_frame(FrameCensus::default());
    assert_eq!(
        census.lines().len(),
        1,
        "a frame with every slot drawn says nothing"
    );
}
