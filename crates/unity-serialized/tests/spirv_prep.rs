#[path = "support/spirv_modules.rs"]
mod spirv_modules;

use spirv_modules::{GLSL, Problems, bit_search, module, op, resources, search};
use worldrunner_unity_serialized::spirv_prep::{PrepareError, prepare};

#[test]
fn rejects_modules_that_are_not_structurally_valid() {
    let mut broken = module(Problems::default());
    // The final `OpFunctionEnd` claims a second word that is not there.
    let last = broken.last_mut().unwrap();
    assert_eq!([*last], *op(56, &[]));
    *last = op(56, &[0])[0];
    assert!(matches!(prepare(&broken), Err(PrepareError::Module(_))));
    assert!(matches!(
        prepare(&broken[..4]),
        Err(PrepareError::Module(_))
    ));
}

#[test]
fn a_panic_in_the_sampler_split_is_an_error() {
    // A resource with a descriptor set but no binding, next to an image type.
    let mut words = resources(false);
    let binding_of_45 = op(71, &[45, 33, 1]);
    let at = words.windows(4).position(|w| w == binding_of_45).unwrap();
    words.drain(at..at + 4);
    match prepare(&words) {
        Err(error @ PrepareError::Panicked { .. }) => {
            assert_eq!(error.cause(), "sampler split panic");
        }
        other => panic!("expected a caught panic, got {other:?}"),
    }
}

#[test]
fn rejects_msb_searches_whose_operand_reads_the_other_way() {
    for case in [
        search(74, false, false, false),
        search(75, true, true, true),
    ] {
        let error = prepare(&bit_search(case, GLSL)).unwrap_err();
        assert!(matches!(error, PrepareError::MsbSignedness), "{error}");
        assert_eq!(error.cause(), "Msb signedness");
    }
}
