use super::{game, log, run, two_clocks};
use worldrunner_script::ScriptError;

#[test]
fn masks_or_and_clear_all_32_bits_independently_per_clock() {
    let mut scripts = game(&two_clocks(), &[]);
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 0);
    assert_eq!(scripts.clock_pause_mask("real").unwrap(), 0);
    assert_eq!(scripts.pause_clock("game", 4).unwrap(), 4);
    assert_eq!(
        scripts.pause_clock("game", 0x8000_0002).unwrap(),
        0x8000_0006
    );
    assert_eq!(scripts.pause_clock("game", 4).unwrap(), 0x8000_0006);
    assert_eq!(scripts.pause_clock("real", 8).unwrap(), 8);
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 0x8000_0006);
    assert_eq!(scripts.resume_clock("game", 2).unwrap(), 0x8000_0004);
    assert_eq!(scripts.resume_clock("game", 16).unwrap(), 0x8000_0004);
    assert_eq!(scripts.resume_clock("game", 4).unwrap(), 0x8000_0000);
    assert!(scripts.clock_paused("game").unwrap());
    assert_eq!(scripts.resume_clock("game", 0x8000_0000).unwrap(), 0);
    assert!(!scripts.clock_paused("game").unwrap());
    assert_eq!(scripts.clock_pause_mask("real").unwrap(), 8);
    assert!(scripts.clock_paused("real").unwrap());
    assert_eq!(scripts.pause_clock("game", u32::MAX).unwrap(), u32::MAX);
    let mask = scripts.clock_pause_mask("game").unwrap();
    assert_eq!(scripts.resume_clock("game", mask).unwrap(), 0);
}

#[test]
fn zero_bits_leave_both_paused_and_unpaused_clocks_unchanged() {
    let mut scripts = game(&two_clocks(), &[]);
    for mask in [0, 4, 0x8000_0000] {
        assert_eq!(scripts.pause_clock("game", mask).unwrap(), mask);
        assert_eq!(scripts.pause_clock("game", 0).unwrap(), mask);
        assert_eq!(scripts.resume_clock("game", 0).unwrap(), mask);
        assert_eq!(scripts.clock_pause_mask("game").unwrap(), mask);
        assert_eq!(scripts.clock_paused("game").unwrap(), mask != 0);
        assert_eq!(scripts.resume_clock("game", mask).unwrap(), 0);
    }
}

#[test]
fn the_boolean_setter_and_mask_operations_share_the_same_state() {
    let mut scripts = game(&two_clocks(), &[]);
    scripts.set_clock_paused("game", true).unwrap();
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 1);
    assert_eq!(scripts.pause_clock("game", 0).unwrap(), 1);
    assert!(scripts.clock_paused("game").unwrap());
    assert_eq!(scripts.resume_clock("game", 1).unwrap(), 0);
    assert!(!scripts.clock_paused("game").unwrap());

    assert_eq!(scripts.pause_clock("game", 4).unwrap(), 4);
    scripts.set_clock_paused("game", false).unwrap();
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 0);
    assert_eq!(scripts.pause_clock("game", 0).unwrap(), 0);
    assert!(!scripts.clock_paused("game").unwrap());

    assert_eq!(
        scripts.pause_clock("game", 0x8000_0004).unwrap(),
        0x8000_0004
    );
    scripts.set_clock_paused("game", true).unwrap();
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 0x8000_0005);
    scripts.set_clock_paused("game", true).unwrap();
    assert_eq!(scripts.resume_clock("game", 1).unwrap(), 0x8000_0004);
    assert!(scripts.clock_paused("game").unwrap());
    scripts.set_clock_paused("game", false).unwrap();
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 0);
    assert!(!scripts.clock_paused("game").unwrap());
    assert_eq!(scripts.clock_pause_mask("real").unwrap(), 0);
}

#[test]
fn mask_queries_and_mutations_reject_unknown_clocks_even_for_zero_bits() {
    let mut scripts = game(&two_clocks(), &[]);
    scripts.pause_clock("game", 4).unwrap();
    let expected = scripts.set_clock_paused("missing", true).unwrap_err();
    for error in [
        scripts.clock_pause_mask("missing").unwrap_err(),
        scripts.pause_clock("missing", 0).unwrap_err(),
        scripts.resume_clock("missing", 0).unwrap_err(),
        scripts.pause_clock("missing", u32::MAX).unwrap_err(),
        scripts.resume_clock("missing", u32::MAX).unwrap_err(),
    ] {
        assert!(matches!(
            &error,
            ScriptError::Sandbox { message } if message == "no clock named missing"
        ));
        assert_eq!(error.to_string(), expected.to_string());
    }
    assert_eq!(scripts.clock_pause_mask("game").unwrap(), 4);
    assert_eq!(scripts.clock_pause_mask("real").unwrap(), 0);
}

#[test]
fn advancing_skips_masked_clocks_and_their_due_threads() {
    for mask in [4, 0x8000_0000, 0x8000_0004] {
        let mut scripts = game(&two_clocks(), &["SpawnGame", "GameTime", "RealTime"]);
        run(
            &mut scripts,
            "SpawnGame(function() Sleep(5) Note('game') end)
             Spawn(function() Sleep(5) Note('real') end)",
        );
        scripts.advance(0).unwrap();
        let before = scripts.threads();
        scripts.pause_clock("game", mask).unwrap();
        assert!(scripts.advance_clock("game", 10).unwrap().is_empty());
        assert_eq!(scripts.threads(), before);
        assert!(log(&mut scripts).is_empty());
        assert!(scripts.advance(10).unwrap().is_empty());
        assert_eq!(scripts.clock_now("game"), Some(0));
        assert_eq!(scripts.clock_now("real"), Some(10));
        assert_eq!(scripts.clock_pause_mask("game").unwrap(), mask);
        assert_eq!(scripts.thread_count(), 1);
        assert_eq!(log(&mut scripts), ["real@5"]);
        scripts.resume_clock("game", mask).unwrap();
        assert!(scripts.advance(5).unwrap().is_empty());
        assert_eq!(scripts.clock_now("game"), Some(5));
        assert_eq!(log(&mut scripts), ["real@5", "game@5"]);
        assert_eq!(scripts.thread_count(), 0);
    }
}

#[test]
fn masked_clocks_do_not_overflow_during_an_advance() {
    let mut scripts = game(&two_clocks(), &[]);
    scripts.set_clock("game", u64::MAX).unwrap();
    scripts.pause_clock("game", 0x8000_0000).unwrap();
    assert!(scripts.advance_clock("game", u64::MAX).unwrap().is_empty());
    assert!(scripts.advance(1).unwrap().is_empty());
    assert_eq!(scripts.clock_now("game"), Some(u64::MAX));
    assert_eq!(scripts.clock_now("real"), Some(1));
    scripts.resume_clock("game", 0x8000_0000).unwrap();
    assert!(scripts.advance_clock("game", 1).is_err());
}
