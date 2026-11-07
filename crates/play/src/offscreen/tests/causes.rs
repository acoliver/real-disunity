//! The inner causes a report keeps beside the names of its failures.

use worldrunner_render_program::error::{ProgramError, UnsupportedTarget};
use worldrunner_render_program::unity::layout::LayoutError;
use worldrunner_render_program::unity::offscreen::OffscreenError;

use super::super::frame::Report;

#[test]
fn a_failure_is_counted_by_its_name_and_by_the_names_nested_in_it() {
    let mut report = Report::default();
    let multisampled = OffscreenError::Program(ProgramError::UnsupportedTarget(
        UnsupportedTarget::Multisampled { samples: 4 },
    ));
    report.fail("rendertexture", &multisampled);
    report.fail("rendertexture", &multisampled);
    let storage = OffscreenError::Program(ProgramError::from(LayoutError::UnsupportedParameter {
        kind: "storage image".to_owned(),
    }));
    report.fail("record", &storage);

    assert_eq!(report.failures["rendertexture:Program"], 2);
    assert_eq!(report.failures["record:Program"], 1);
    assert_eq!(
        report.causes["rendertexture:Program/UnsupportedTarget/Multisampled"],
        2
    );
    assert_eq!(report.causes["record:Program/Layout/Declaration"], 1);
    assert!(report.examples["record:Program/Layout/Declaration"].contains("storage image"));
    assert_eq!(
        report.examples["rendertexture:Program/UnsupportedTarget/Multisampled"],
        "the pass has 4 samples per pixel; program draws are single-sample"
    );
}

#[test]
fn a_failure_whose_payload_is_text_has_only_its_own_name() {
    let mut report = Report::default();
    report.fail("record", &OffscreenError::Unknown("a pass".to_owned()));
    assert_eq!(report.causes["record:Unknown"], 1);
}

#[test]
fn reports_add_their_failures_causes_and_keep_the_first_example() {
    let mut first = Report::default();
    first.fail("texture", &OffscreenError::UnsupportedFormat { code: 7 });
    let mut second = Report::default();
    second.fail("texture", &OffscreenError::UnsupportedFormat { code: 9 });
    second.fail("mesh", &OffscreenError::UnsupportedDepth { bits: 8 });
    first.absorb_failures(second);

    assert_eq!(first.failures["texture:UnsupportedFormat"], 2);
    assert_eq!(first.causes["texture:UnsupportedFormat"], 2);
    assert_eq!(first.causes["mesh:UnsupportedDepth"], 1);
    assert_eq!(
        first.examples["texture:UnsupportedFormat"],
        "GraphicsFormat 7 is not one this renderer draws into or samples"
    );
}
