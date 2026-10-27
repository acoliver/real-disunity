//! Patch overlays: ordered, hash-pinned edits applied to a game's original scripts as they load.

use worldrunner_script::{
    Edit, Overlay, Patch, PatchError, PatchSet, ScriptConfig, ScriptError, ScriptHost, sha256_hex,
};

/// An original script with a Lua 5.1 idiom and an unsafe default, as a game might ship it.
const ORIGINAL: &str = "local M = {}\nfunction M.sum(...)\n  local n = 0\n  for _, v in ipairs(arg) do n = n + v end\n  return n\nend\nM.limit = 0\nreturn M\n";

fn edit(find: &str, replace: &str) -> Edit {
    Edit {
        find: find.into(),
        replace: replace.into(),
    }
}

/// Two patches in order: the Lua 5.4 varargs fix, then a security limit.
fn set(pin: String) -> PatchSet {
    PatchSet {
        file: "units/sum.lua".into(),
        original_sha256: pin,
        patches: vec![
            Patch {
                name: "lua54-varargs".into(),
                edits: vec![edit(
                    "  local n = 0\n",
                    "  local arg = {...}\n  local n = 0\n",
                )],
            },
            Patch {
                name: "limit-sum".into(),
                edits: vec![edit("M.limit = 0", "M.limit = 100")],
            },
        ],
    }
}

fn overlay() -> Overlay {
    Overlay::new([set(sha256_hex(ORIGINAL.as_bytes()))]).unwrap()
}

#[test]
fn pinned_patches_apply_in_order_before_the_script_loads() {
    let mut host = ScriptHost::new(ScriptConfig::default()).unwrap();
    let applied = host
        .load_patched_module("units/sum.lua", ORIGINAL.as_bytes(), &overlay())
        .unwrap();
    assert_eq!(applied, ["lua54-varargs", "limit-sum"]);
    let result = host
        .eval_json("local m = require('units/sum.lua') return { m.sum(1, 2, 3), m.limit }")
        .unwrap();
    assert_eq!(result, serde_json::json!([6, 100]));
}

#[test]
fn files_without_patches_load_unchanged() {
    let patched = overlay().apply("units/other.lua", b"return 1").unwrap();
    assert_eq!(patched.source, b"return 1");
    assert!(patched.applied.is_empty());
}

#[test]
fn a_changed_original_is_refused_naming_both_hashes() {
    let changed = ORIGINAL.replace("M.limit = 0", "M.limit = 5");
    let mut host = ScriptHost::new(ScriptConfig::default()).unwrap();
    let error = host
        .load_patched_module("units/sum.lua", changed.as_bytes(), &overlay())
        .unwrap_err();
    let ScriptError::Patch(PatchError::Stale { pinned, found, .. }) = &error else {
        panic!("{error}");
    };
    assert_eq!(pinned, &sha256_hex(ORIGINAL.as_bytes()));
    assert_eq!(found, &sha256_hex(changed.as_bytes()));
    assert!(error.to_string().contains("units/sum.lua"), "{error}");
    let missing = host
        .eval_json("return pcall(require, 'units/sum.lua')")
        .unwrap();
    assert_eq!(missing, serde_json::json!(false), "nothing was loaded");
}

#[test]
fn an_edit_must_match_exactly_once() {
    let pin = sha256_hex(ORIGINAL.as_bytes());
    // "local" occurs twice in the original and once more after the first patch.
    for (find, count) in [("absent", 0), ("local", 3), ("", 0)] {
        let mut bad = set(pin.clone());
        bad.patches[1].edits.push(edit(find, "x"));
        let error = Overlay::new([bad])
            .unwrap()
            .apply("units/sum.lua", ORIGINAL.as_bytes())
            .unwrap_err();
        let expected = PatchError::Match {
            file: "units/sum.lua".into(),
            patch: "limit-sum".into(),
            edit: 1,
            count,
        };
        assert_eq!(error, expected, "find {find:?}");
    }
    // Edits see the text left by earlier patches: `M.limit = 100` now occurs once.
    let mut chained = set(pin);
    chained.patches.push(Patch {
        name: "raise".into(),
        edits: vec![edit("M.limit = 100", "M.limit = 200")],
    });
    let patched = Overlay::new([chained])
        .unwrap()
        .apply("units/sum.lua", ORIGINAL.as_bytes())
        .unwrap();
    assert!(
        String::from_utf8(patched.source)
            .unwrap()
            .contains("M.limit = 200")
    );
}

#[test]
fn malformed_overlays_and_review_report_every_problem() {
    let pin = sha256_hex(ORIGINAL.as_bytes());
    let upper = Overlay::new([set(pin.to_uppercase())]).unwrap_err();
    assert!(upper.to_string().contains("64 lowercase hex"), "{upper}");
    let twice = Overlay::new([set(pin.clone()), set(pin.clone())]).unwrap_err();
    assert!(twice.to_string().contains("two patch sets"), "{twice}");

    let mut gone = set(pin);
    gone.file = "units/gone.lua".into();
    let overlay = Overlay::new([set(sha256_hex(ORIGINAL.as_bytes())), gone]).unwrap();
    let changed = ORIGINAL.replace("return M", "return M -- v2");
    let problems =
        overlay.review(|file| (file == "units/sum.lua").then(|| changed.clone().into_bytes()));
    let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(problems.len(), 2, "{messages:?}");
    assert!(messages[0].contains("units/gone.lua") && messages[0].contains("missing"));
    assert!(matches!(problems[1], PatchError::Stale { .. }));
    assert!(
        overlay
            .review(|file| (file == "units/sum.lua" || file == "units/gone.lua")
                .then(|| ORIGINAL.as_bytes().to_vec()))
            .is_empty()
    );
}

#[test]
fn pins_are_sha256_as_sha256sum_prints_it() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
