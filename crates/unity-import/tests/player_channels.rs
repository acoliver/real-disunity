//! The channel report counts variants, not pairs, and checks the mask.

use worldrunner_unity_import::player_channels::ChannelReport;
use worldrunner_unity_serialized::shader_program::BindChannels;

fn table(pairs: &[[i8; 2]], mask: i32) -> BindChannels {
    BindChannels {
        channels: pairs.to_vec(),
        source_map: mask,
    }
}

#[test]
fn each_variant_counts_once_for_every_source_channel_and_input_it_reads() {
    let mut report = ChannelReport::default();
    report.add_table(&table(&[[0, 0], [4, 3]], 0b1_0001));
    report.add_table(&table(&[[0, 0]], 0b1));
    report.add_table(&table(&[[4, 3], [4, 7]], 0b1_0000));
    assert_eq!(report.variants, 3);
    assert_eq!(report.readers.get(&0), Some(&2));
    assert_eq!(
        report.readers.get(&4),
        Some(&2),
        "a variant counts once however many pairs"
    );
    assert_eq!(report.inputs.get(&3), Some(&2));
    assert_eq!(report.inputs.get(&7), Some(&1));
    assert_eq!(report.pair_counts.get(&2), Some(&2));
    assert_eq!(report.pair_counts.get(&1), Some(&1));
    assert_eq!(report.repeated_sources, 1);
    assert_eq!(report.repeated_inputs, 0);
    assert_eq!(report.mask_matches, 3);
}

#[test]
fn a_mask_that_is_not_the_bits_of_the_sources_is_not_counted_as_matching() {
    let mut report = ChannelReport::default();
    report.add_table(&table(&[[1, 1]], 0b100));
    assert_eq!(report.mask_matches, 0);
    report.add_table(&table(&[[-1, 0]], 0));
    assert_eq!(report.mask_matches, 0, "a negative channel has no bit");
}
