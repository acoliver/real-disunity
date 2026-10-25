use compat_player::Installation;

#[path = "../src/test_support.rs"]
pub mod support;

#[test]
fn portable_interactive_fixtures_compile_every_current_original_script() {
    let installation = support::installation();
    for (name, count) in [
        ("snake-interactive", 8),
        ("pong-interactive", 20),
        ("breakout-interactive", 7),
    ] {
        let artifacts = support::scratch(name);
        let request = support::fixture_request(name, artifacts.path(), true);
        assert_eq!(request.metadata["script_count"], count);
        let assembly = compat_player::compile_game(&installation, &request).unwrap();
        assert!(assembly.is_file(), "{name}");
    }
}
