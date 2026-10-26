#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/objects.rs"]
mod objects;

use fixture::Fixture;
use objects::{game_object, import, transform};
use worldrunner_unity_import::{
    CameraOverride, CameraRenderType, CanvasScale, ObjectId, PackageFont, RenderMode, Scene,
    ScreenMatch, TextAnchor,
};

const UI: &str = "f70555f144d8491a825f0804e09c671c";
const LIGHT: &str = "073797afb82c5a1438f328866b10b3f0";
const CAMERA_DATA: &str = "a79441f348de89743a2939f4d699eac1";
const ARIAL: &str = "{fileID: 10102, guid: 0000000000000000e000000000000000, type: 0}";

fn behaviour(id: u64, guid: &str, file_id: i64, fields: &str) -> String {
    format!(
        "--- !u!114 &{id}\nMonoBehaviour:\n  m_GameObject: {{fileID: 1}}\n  m_Script: {{fileID: {file_id}, guid: {guid}, type: 3}}\n{fields}"
    )
}

fn text(id: u64, font: &str, material: &str) -> String {
    behaviour(
        id,
        UI,
        708_705_254,
        &format!(
            "  m_Material: {material}\n  m_Color: {{r: 1, g: 0.5, b: 0, a: 1}}\n  m_RaycastTarget: 0\n  m_FontData:\n    m_Font: {font}\n    m_FontSize: 72\n    m_FontStyle: 3\n    m_BestFit: 1\n    m_Alignment: 4\n    m_RichText: 0\n    m_HorizontalOverflow: 1\n    m_LineSpacing: 1.5\n  m_Text: Score\n"
        ),
    )
}

fn scaler(id: u64, mode: u32) -> String {
    behaviour(
        id,
        UI,
        1_980_459_831,
        &format!(
            "  m_UiScaleMode: {mode}\n  m_ReferencePixelsPerUnit: 100\n  m_ScaleFactor: 2\n  m_ReferenceResolution: {{x: 1920, y: 1080}}\n  m_ScreenMatchMode: 1\n  m_MatchWidthOrHeight: 0.5\n"
        ),
    )
}

fn light(id: u64, light_type: u32, layers: &str) -> String {
    behaviour(
        id,
        LIGHT,
        11_500_000,
        &format!(
            "  m_LightType: {light_type}\n  m_BlendStyleIndex: 0\n  m_Color: {{r: 1, g: 0.5, b: 0.25, a: 1}}\n  m_Intensity: 2\n  m_ApplyToSortingLayers: {layers}\n"
        ),
    )
}

fn camera_data(id: u64, stack: &str, camera_type: u32) -> String {
    behaviour(
        id,
        CAMERA_DATA,
        11_500_000,
        &format!(
            "  m_RenderShadows: 0\n  m_RequiresDepthTextureOption: 1\n  m_CameraType: {camera_type}\n  m_Cameras:{stack}\n  m_RendererIndex: -1\n  m_RenderPostProcessing: 1\n  m_Antialiasing: 3\n"
        ),
    )
}

const CANVAS: &str = "--- !u!223 &30\nCanvas:\n  m_GameObject: {fileID: 1}\n  m_RenderMode: 1\n  m_Camera: {fileID: 2}\n  m_PlaneDistance: 10\n  m_PixelPerfect: 1\n  m_SortingOrder: 3\n";
const RECT: &str = "--- !u!224 &31\nRectTransform:\n  m_GameObject: {fileID: 1}\n  m_Children: []\n  m_Father: {fileID: 0}\n  m_AnchorMin: {x: 0.25, y: 0.5}\n  m_AnchorMax: {x: 0.75, y: 0.5}\n  m_AnchoredPosition: {x: 4, y: -8}\n  m_SizeDelta: {x: 100, y: 50}\n";

fn scene(objects: &[String]) -> (Fixture, Scene) {
    let fixture = Fixture::new("6000.3.5f2");
    let mut text = [game_object(1, "UI", &[2]), transform(2, 1, 0.0, 0)].concat();
    text.extend(objects.iter().map(String::as_str));
    let scene = import(&fixture, "Assets/Main.unity", &text).unwrap();
    (fixture, scene)
}

fn id(scene: &Scene, file_id: i64) -> ObjectId {
    scene
        .objects()
        .find(|(_, object)| object.source.file_id == file_id)
        .map_or_else(|| panic!("no object {file_id}"), |(id, _)| id)
}

fn bits<const N: usize>(values: [f64; N]) -> [u64; N] {
    values.map(f64::to_bits)
}

#[test]
fn canvases_rect_transforms_and_scalers_read_their_layout() {
    let (_fixture, scene) = scene(&[CANVAS.into(), RECT.into(), scaler(40, 1), scaler(41, 0)]);
    let canvas = scene.canvas(id(&scene, 30)).unwrap();
    assert_eq!(canvas.render_mode, RenderMode::ScreenSpaceCamera);
    assert_eq!(canvas.camera, Some(id(&scene, 2)));
    assert!(canvas.pixel_perfect);
    assert_eq!(
        (canvas.plane_distance.to_bits(), canvas.sorting_order),
        (10.0_f64.to_bits(), 3)
    );
    let rect = scene.rect_transform(id(&scene, 31)).unwrap();
    assert_eq!(bits(rect.anchor_min), bits([0.25, 0.5]));
    assert_eq!(bits(rect.anchor_max), bits([0.75, 0.5]));
    assert_eq!(bits(rect.anchored_position), bits([4.0, -8.0]));
    assert_eq!(
        bits(rect.pivot),
        bits([0.5, 0.5]),
        "absent pivot is centered"
    );
    let CanvasScale::ScaleWithScreenSize {
        reference_resolution,
        screen_match,
        match_width_or_height,
    } = scene.canvas_scaler(id(&scene, 40)).unwrap().scale
    else {
        panic!("screen-size scaling expected");
    };
    assert_eq!(bits(reference_resolution), bits([1920.0, 1080.0]));
    assert_eq!(screen_match, ScreenMatch::Expand);
    assert_eq!(match_width_or_height.to_bits(), 0.5_f64.to_bits());
    let constant = scene.canvas_scaler(id(&scene, 41)).unwrap().scale;
    assert_eq!(
        constant,
        CanvasScale::ConstantPixelSize { scale_factor: 2.0 }
    );
}

#[test]
fn text_reads_its_string_color_and_font_settings() {
    let (_fixture, scene) = scene(&[text(50, ARIAL, "{fileID: 0}")]);
    let label = scene.text(id(&scene, 50)).unwrap();
    assert_eq!(label.text, "Score");
    assert_eq!(bits(label.color), bits([1.0, 0.5, 0.0, 1.0]));
    assert!(!label.raycast_target);
    let font = label.font;
    assert_eq!((font.font, font.size), (PackageFont::Arial, 72));
    assert_eq!(font.alignment, TextAnchor::MiddleCenter);
    assert!(font.best_fit && !font.rich_text);
    assert_eq!(format!("{:?}", font.style), "BoldAndItalic");
    assert_eq!(format!("{:?}", font.horizontal_overflow), "Overflow");
    assert_eq!(font.line_spacing.to_bits(), 1.5_f64.to_bits());
}

#[test]
fn urp_global_lights_and_camera_data_read_their_settings() {
    let (_fixture, scene) = scene(&[
        light(60, 4, "000000000b41e56affffffff"),
        camera_data(61, " []", 1),
    ]);
    let light = scene.light_2d(id(&scene, 60)).unwrap();
    assert_eq!(bits(light.color), bits([1.0, 0.5, 0.25, 1.0]));
    assert_eq!(light.intensity.to_bits(), 2.0_f64.to_bits());
    assert_eq!(
        light.target_sorting_layers,
        [0, 1_793_409_291, -1],
        "little-endian int32 array"
    );
    let camera = scene.urp_camera(id(&scene, 61)).unwrap();
    assert_eq!(camera.render_type, CameraRenderType::Overlay);
    assert_eq!(camera.depth_texture, CameraOverride::On);
    assert_eq!(
        camera.opaque_texture,
        CameraOverride::UsePipelineSettings,
        "absent field default"
    );
    assert!(!camera.render_shadows && camera.clear_depth);
    assert!(camera.post_processing.enabled);
    assert_eq!(format!("{:?}", camera.post_processing.antialiasing), "Taa");
}

#[test]
fn unsupported_ui_and_urp_settings_fail_with_their_object() {
    let other_font = "{fileID: 12800000, guid: 00000000000000000000000000000001, type: 3}";
    let (_f, scene) = scene(&[
        text(50, other_font, "{fileID: 0}"),
        text(
            51,
            ARIAL,
            "{fileID: 2100000, guid: 0000000000000000f000000000000000, type: 0}",
        ),
        scaler(52, 2),
        light(53, 3, "00000000"),
        light(54, 4, "000000"),
        camera_data(55, "\n  - {fileID: 2}", 0),
        camera_data(56, " []", 7),
    ]);
    let error = |file_id: i64, read: &dyn Fn(ObjectId) -> String| read(id(&scene, file_id));
    let font = error(50, &|id| scene.text(id).unwrap_err().to_string());
    assert!(
        font.contains("MonoBehaviour 50") && font.contains("font"),
        "{font}"
    );
    assert!(
        error(51, &|id| scene.text(id).unwrap_err().to_string())
            .contains("custom UI text materials")
    );
    assert!(
        error(52, &|id| scene.canvas_scaler(id).unwrap_err().to_string()).contains("physical size")
    );
    assert!(
        error(53, &|id| scene.light_2d(id).unwrap_err().to_string()).contains("Point 2D lights")
    );
    assert!(
        error(54, &|id| scene.light_2d(id).unwrap_err().to_string())
            .contains("invalid m_ApplyToSortingLayers")
    );
    assert!(
        error(55, &|id| scene.urp_camera(id).unwrap_err().to_string()).contains("camera stacks")
    );
    assert!(
        error(56, &|id| scene.urp_camera(id).unwrap_err().to_string())
            .contains("invalid m_CameraType")
    );
    let wrong = error(52, &|id| scene.text(id).unwrap_err().to_string());
    assert!(wrong.contains("is not a Text"), "{wrong}");
    let not_canvas = error(53, &|id| scene.canvas(id).unwrap_err().to_string());
    assert!(not_canvas.contains("is not a Canvas"), "{not_canvas}");
}
