use std::path::PathBuf;
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};

#[test]
fn original_ui_uses_typed_rect_canvas_text_and_scaler_native_fields() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile: ImportProfile =
        serde_json::from_slice(&std::fs::read(base.join("profiles/pong-source.json")).unwrap())
            .unwrap();
    let graph = import_project(
        &base.join("../../.work/games/pong"),
        "Assets/Scenes/Main.unity".as_ref(),
        &profile,
        &ImportSidecar::default(),
    )
    .unwrap();
    let rect = graph
        .components
        .iter()
        .find(|c| c.kind == "RectTransform")
        .unwrap();
    assert_eq!(rect.native_kind.as_deref(), Some("RectTransform"));
    assert_eq!(
        rect.native_fields["pivot"][0].as_f64(),
        rect.fields["m_Pivot"]["x"].as_f64()
    );
    let canvas = graph
        .components
        .iter()
        .find(|c| c.kind == "Canvas")
        .unwrap();
    assert_eq!(canvas.native_fields["render_mode"], "screen_space_camera");
    assert!(
        canvas
            .native_fields
            .get("m_AdditionalShaderChannelsFlag")
            .is_none()
    );
    let text = graph
        .components
        .iter()
        .find(|c| c.native_kind.as_deref() == Some("Text"))
        .unwrap();
    assert_eq!(
        text.native_fields["font_size"].as_f64(),
        text.fields["m_FontData"]["m_FontSize"].as_f64()
    );
    assert_eq!(text.native_fields["font"], "host.builtin-font");
    assert!(
        graph
            .components
            .iter()
            .any(|c| c.native_kind.as_deref() == Some("CanvasScaler"))
    );
}
