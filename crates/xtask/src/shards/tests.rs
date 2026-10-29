use super::*;

fn unit(kind: Kind, name: &str) -> Unit {
    Unit {
        kind,
        name: name.to_owned(),
    }
}

fn target(name: &str, kinds: &[&str]) -> metadata::Target {
    metadata::Target {
        name: name.to_owned(),
        kind: kinds.iter().map(|kind| (*kind).to_owned()).collect(),
    }
}

fn names(count: usize) -> BTreeSet<String> {
    (0..count)
        .map(|index| format!("module::test_{index:04}"))
        .collect()
}

fn units() -> BTreeSet<Unit> {
    BTreeSet::from([
        unit(Kind::Test, "live_window"),
        unit(Kind::Test, "callback_failure"),
        unit(Kind::Test, "play_window"),
    ])
}

#[test]
fn harness_free_targets_are_read_from_manifest_text() {
    let manifest = r#"
[package]
name = "sample"

[[bin]]
name = "play"
path = "src/main.rs"

[[test]]
name = "play_window"
harness = false

[[test]]
name = "ordinary"
harness = true

[[example]]
name = "demo"
harness = false

[lib]
harness = true
"#;
    assert_eq!(
        harness_free(manifest).unwrap(),
        BTreeSet::from([unit(Kind::Test, "play_window"), unit(Kind::Example, "demo")])
    );
    assert!(
        harness_free("[package]\nname = \"plain\"\n")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unshardable_manifests_fail_closed() {
    assert!(harness_free("[lib]\nharness = false\n").is_err());
    assert!(harness_free("[[test]]\nharness = false\n").is_err());
    assert!(harness_free("lib = 3\n").is_err());
    assert!(harness_free("not toml [").is_err());
}

#[test]
fn targets_are_classified_by_cargo_kind_and_manifest_harness() {
    let targets = [
        target("sample", &["cdylib", "rlib"]),
        target("play", &["bin"]),
        target("play_window", &["test"]),
        target("ordinary", &["test"]),
        target("speed", &["bench"]),
        target("demo", &["example"]),
        target("build-script-build", &["custom-build"]),
    ];
    let manifest = "[[test]]\nname = \"play_window\"\nharness = false\n";
    let classified = classify(&targets, manifest).unwrap();
    assert_eq!(classified.len(), 6);
    assert!(classified.contains(&(unit(Kind::Lib, "sample"), true)));
    assert!(classified.contains(&(unit(Kind::Test, "play_window"), false)));
    assert!(classified.contains(&(unit(Kind::Test, "ordinary"), true)));
    assert!(classify(&[target("odd", &["mystery"])], "").is_err());
}

#[test]
fn target_flags_select_every_libtest_binary_and_set_whole_units_aside() {
    let merged = merge(vec![
        (unit(Kind::Lib, "a"), true),
        (unit(Kind::Lib, "b"), true),
        (unit(Kind::Bin, "tool"), true),
        (unit(Kind::Test, "ordinary"), true),
        (unit(Kind::Bench, "speed"), true),
        (unit(Kind::Example, "demo"), true),
        (unit(Kind::Test, "live_window"), false),
    ])
    .unwrap();
    assert_eq!(
        merged.libtest_flags(),
        [
            "--lib",
            "--bin",
            "tool",
            "--test",
            "ordinary",
            "--bench",
            "speed",
            "--example",
            "demo"
        ]
    );
    assert_eq!(
        merged.whole().iter().cloned().collect::<Vec<_>>(),
        [unit(Kind::Test, "live_window")]
    );
}

#[test]
fn a_harness_free_name_shared_with_a_libtest_target_is_rejected() {
    let error = merge(vec![
        (unit(Kind::Lib, "a"), true),
        (unit(Kind::Test, "window"), true),
        (unit(Kind::Test, "window"), false),
    ])
    .unwrap_err();
    assert!(error.contains("test target `window`"), "{error}");
    let other_kind = merge(vec![
        (unit(Kind::Lib, "a"), true),
        (unit(Kind::Test, "window"), true),
        (unit(Kind::Bin, "window"), false),
    ]);
    assert!(other_kind.is_ok());
    assert!(merge(vec![(unit(Kind::Test, "only"), false)]).is_err());
}

#[test]
fn listing_output_yields_test_names_only() {
    let listing = "alpha::one: test\nalpha::two: test\n\n2 tests, 0 benchmarks\nspeed: benchmark\nalpha::one: test\n\nbeta::three: test\n";
    assert_eq!(
        parse_names(listing).into_iter().collect::<Vec<_>>(),
        ["alpha::one", "alpha::two", "beta::three"]
    );
    assert!(parse_names("0 tests, 0 benchmarks\n").is_empty());
}

#[test]
fn shard_specs_accept_only_one_based_ranges() {
    assert_eq!(parse_shard("1/1").unwrap(), (1, 1));
    assert_eq!(parse_shard("3/8").unwrap(), (3, 8));
    assert_eq!(parse_shard("8/8").unwrap(), (8, 8));
    for bad in [
        "",
        "1",
        "0/8",
        "9/8",
        "1/0",
        "a/b",
        "1/",
        "/8",
        "+1/8",
        "-1/8",
        "1/8/2",
        " 1/8",
        "1/257",
        "99999999999999999999/8",
    ] {
        assert!(parse_shard(bad).is_err(), "{bad:?} must be rejected");
    }
    assert_eq!(parse_count("8").unwrap(), 8);
    for bad in ["", "0", "x", "-3", "1/2", "257"] {
        assert!(parse_count(bad).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn every_shard_count_covers_the_input_exactly_once() {
    let all_names = names(53);
    let all_units = units();
    for count in 1..=20 {
        let shards = partition(&all_units, &all_names, &Weights::uniform(), count).unwrap();
        assert_eq!(shards.len(), count);
        let mut union_names = BTreeSet::new();
        let mut union_units = BTreeSet::new();
        let mut total = 0;
        for shard in &shards {
            total += shard.names.len() + shard.units.len();
            union_names.extend(shard.names.iter().cloned());
            union_units.extend(shard.units.iter().cloned());
        }
        assert_eq!(union_names, all_names);
        assert_eq!(union_units, all_units);
        assert_eq!(total, all_names.len() + all_units.len(), "shards overlap");
        verify(&shards, &all_units, &all_names).unwrap();
    }
}

#[test]
fn equal_weights_deal_round_robin_with_units_first() {
    let shards = partition(&units(), &names(53), &Weights::uniform(), 8).unwrap();
    let sizes: Vec<_> = shards
        .iter()
        .map(|shard| shard.names.len() + shard.units.len())
        .collect();
    assert!(sizes.iter().max().unwrap() - sizes.iter().min().unwrap() <= 1);
    assert_eq!(shards[0].units, [unit(Kind::Test, "callback_failure")]);
    assert_eq!(shards[1].units, [unit(Kind::Test, "live_window")]);
    assert_eq!(shards[2].units, [unit(Kind::Test, "play_window")]);
    assert_eq!(shards[3].names[0], "module::test_0000");
    assert!(shards.iter().all(|shard| shard.names.is_sorted()));
}

const WEIGHTS: &str = r#"
default_test = 1
default_unit = 60

[units]
"test play_window" = 120

[tests]
"module::test_0000" = 360
"module::test_0008" = 130
"gone::test" = 9
"#;

#[test]
fn heavy_tests_go_to_separate_shards_and_loads_balance() {
    let weights = Weights::parse(WEIGHTS).unwrap();
    let all_names = names(200);
    let shards = partition(&units(), &all_names, &weights, 8).unwrap();
    verify(&shards, &units(), &all_names).unwrap();
    let holder = |name: &str| {
        shards
            .iter()
            .position(|shard| shard.names.iter().any(|n| n == name))
    };
    assert_ne!(holder("module::test_0000"), holder("module::test_0008"));
    let play = shards
        .iter()
        .position(|shard| shard.units.contains(&unit(Kind::Test, "play_window")));
    assert_ne!(play, holder("module::test_0000"));
    let loads: Vec<_> = shards.iter().map(|shard| load(shard, &weights)).collect();
    assert_eq!(loads.iter().sum::<u64>(), 360 + 130 + 198 + 120 + 60 + 60);
    assert_eq!(*loads.iter().max().unwrap(), 360);
}

#[test]
fn weights_default_unlisted_items_and_report_stale_entries() {
    let weights = Weights::parse(WEIGHTS).unwrap();
    assert_eq!(weights.test("module::test_0000"), 360);
    assert_eq!(weights.test("module::test_0001"), 1);
    assert_eq!(weights.unit(&unit(Kind::Test, "play_window")), 120);
    assert_eq!(weights.unit(&unit(Kind::Test, "live_window")), 60);
    assert_eq!(weights.unmatched(&units(), &names(10)), ["gone::test"]);
}

#[test]
fn malformed_weights_are_rejected() {
    for bad in [
        "default_unit = 60",
        "default_test = 0\ndefault_unit = 60",
        "default_test = 1\ndefault_unit = -5",
        "default_test = 1\ndefault_unit = 60\n[tests]\n\"a\" = 1.5",
        "default_test = 1\ndefault_unit = 60\ntest = 3",
        "default_test = 1\ndefault_unit = 60\ntests = 3",
        "not toml =",
    ] {
        assert!(Weights::parse(bad).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn the_committed_weight_file_parses() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Weights::read(&root).unwrap();
}

#[test]
fn partition_is_deterministic_and_tolerates_more_shards_than_items() {
    let first = partition(&units(), &names(3), &Weights::uniform(), 8).unwrap();
    let second = partition(&units(), &names(3), &Weights::uniform(), 8).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.iter().filter(|shard| shard.names.is_empty()).count(),
        5
    );
    assert!(partition(&units(), &names(3), &Weights::uniform(), 0).is_err());
}

#[test]
fn verification_detects_duplicates_and_omissions() {
    let all_names = names(5);
    let all_units = units();
    let good = partition(&all_units, &all_names, &Weights::uniform(), 3).unwrap();
    verify(&good, &all_units, &all_names).unwrap();

    let mut duplicated = partition(&all_units, &all_names, &Weights::uniform(), 3).unwrap();
    let copy = duplicated[0].names[0].clone();
    duplicated[1].names.push(copy);
    assert!(verify(&duplicated, &all_units, &all_names).is_err());

    let mut missing = partition(&all_units, &all_names, &Weights::uniform(), 3).unwrap();
    missing[2].names.pop();
    assert!(verify(&missing, &all_units, &all_names).is_err());

    let mut unit_twice = partition(&all_units, &all_names, &Weights::uniform(), 3).unwrap();
    let copy = unit_twice[0].units[0].clone();
    unit_twice[2].units.push(copy);
    assert!(verify(&unit_twice, &all_units, &all_names).is_err());
}

#[test]
fn test_arguments_keep_the_selection_and_pass_harness_arguments_last() {
    let selection = strings(&["--package", "a", "--package", "b", "--all-features"]);
    let args = test_args(&selection, &strings(&["--lib"]), &strings(&RUN_ARGS));
    assert_eq!(
        args,
        [
            "test",
            "--locked",
            "--jobs",
            "2",
            "--package",
            "a",
            "--package",
            "b",
            "--all-features",
            "--lib",
            "--",
            "--include-ignored",
            "--test-threads=1"
        ]
    );
    assert_eq!(selected_packages(&selection), ["a", "b"]);
}
