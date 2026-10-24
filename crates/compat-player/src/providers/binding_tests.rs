use super::Binding;
use std::path::Path;

#[test]
fn current_provider_generation_matches_compiled_inputs_before_boot() {
    let root = Path::new(
        option_env!("PROVIDER_QUALIFICATION_ROOT")
            .unwrap_or(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    );
    if let Err(error) = Binding::load(root) {
        panic!("current provider generation must match compiled inputs: {error}");
    }
}

#[test]
fn compiled_extractor_must_match_the_authenticated_tuple_before_boot() {
    let root = Path::new(
        option_env!("PROVIDER_QUALIFICATION_ROOT")
            .unwrap_or(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    );
    let selected = std::fs::read(root.join(
        ".work/resumed-provider-qualification-v2/source/crates/compat-player/src/extraction.rs",
    ))
    .unwrap();
    let result = Binding::load(root);
    if selected == include_bytes!("../extraction.rs") {
        assert!(result.is_ok(), "matching compiled extractor must qualify");
    } else {
        let Err(error) = result else {
            panic!("different compiled extractor accepted the unchanged sealed tuple");
        };
        assert!(error.contains("compiled provider input differs"), "{error}");
    }
}
