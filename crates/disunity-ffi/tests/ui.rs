use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut h = 0;
        assert_eq!(unsafe { disunity_world_create(&mut h) }.code, 0);
        Self(h)
    }
    fn req(&self, v: Value) -> Value {
        let r = request_value(self.0, v).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        r["data"].clone()
    }
    fn create(&self) -> u64 {
        self.req(json!({"op":"object.create"}))["handle"]
            .as_u64()
            .unwrap()
    }
    fn add(&self, o: u64, k: &str, f: Value) -> u64 {
        self.req(json!({"op":"component.add","object":o,"kind":k,"fields":f}))["handle"]
            .as_u64()
            .unwrap()
    }
    fn child(&self, p: u64, f: Value) -> u64 {
        let o = self.create();
        self.add(o, "RectTransform", f);
        self.req(
            json!({"op":"transform.parent","object":o,"parent":p,"world_position_stays":false}),
        );
        o
    }
    fn font(&self) {
        let path = std::env::var("DISUNITY_TEST_FONT")
            .unwrap_or_else(|_| "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
        let bytes = std::fs::read(path)
            .expect("set DISUNITY_TEST_FONT to an independently licensed TrueType font");
        self.req(
            json!({"op":"font.register","name":"default","data_base64":STANDARD.encode(bytes)}),
        );
    }
}
impl Drop for World {
    fn drop(&mut self) {
        unsafe {
            disunity_world_destroy(self.0);
        }
    }
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 0.002, "{a} != {b}");
}

#[test]
fn rect_transform_upgrades_identity_and_resolves_stretch_offsets_and_world_pose() {
    let w = World::new();
    let root = w.create();
    let original =
        w.req(json!({"op":"component.list","object":root,"kind":"Transform"}))[0]["handle"].clone();
    let rect = w.add(root, "RectTransform", json!({"size_delta":[400,200]}));
    assert_eq!(original, rect);
    let child = w.child(root,json!({"anchor_min":[0,0],"anchor_max":[1,1],"pivot":[0.5,0.5],"offset_min":[10,20],"offset_max":[-30,-40]}));
    let r = w.req(json!({"op":"ui.rect","object":child}));
    assert_eq!(r["size"], json!([360.0, 140.0]));
    assert_eq!(r["local_rect"], json!([-180.0, -70.0, 180.0, 70.0]));
    assert_eq!(r["world_corners"][0], json!([-190.0, -80.0, 0.0]));
    assert_eq!(
        w.req(json!({"op":"transform.get","object":child}))["position"],
        json!([-10.0, -10.0, 0.0])
    );
    let c = w.req(json!({"op":"component.list","object":child,"kind":"Transform"}))[0]["handle"]
        .clone();
    w.req(json!({"op":"component.set","component":c,"fields":{"offset_min":[20,30]}}));
    assert_eq!(
        w.req(json!({"op":"ui.rect","object":child}))["size"],
        json!([350.0, 130.0])
    );
    let clone = w.req(json!({"op":"object.clone","object":root}));
    let cloned_child = clone["remap"][child.to_string()].as_u64().unwrap();
    assert_eq!(
        w.req(json!({"op":"ui.rect","object":cloned_child}))["size"],
        json!([350.0, 130.0])
    );
    assert_eq!(
        w.req(json!({"op":"component.list","object":clone["object"]["handle"],"kind":"Transform"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn original_camera_canvas_scaler_places_scores_in_screen_and_world_coordinates() {
    let w = World::new();
    let camera = w.create();
    w.req(json!({"op":"transform.set","object":camera,"position":[0,0,-10]}));
    let camera_c = w.add(camera, "Camera", json!({"orthographic_size":8}));
    let canvas = w.create();
    w.add(canvas, "RectTransform", json!({"pivot":[0,0]}));
    w.add(canvas,"Canvas",json!({"render_mode":"screen_space_camera","camera":{"$ref":camera_c},"plane_distance":100}));
    w.add(canvas,"CanvasScaler",json!({"scale_mode":"scale_with_screen_size","reference_resolution":[1920,1080],"match_width_or_height":0}));
    w.req(json!({"op":"ui.configure","viewport":[960,540]}));
    let scores = w.child(
        canvas,
        json!({"anchor_min":[0.5,0.93],"anchor_max":[0.5,0.93],"size_delta":[1920,160]}),
    );
    let score = w.child(
        scores,
        json!({"anchor_min":[0.25,0.5],"anchor_max":[0.25,0.5],"size_delta":[100,100]}),
    );
    let r = w.req(json!({"op":"ui.rect","object":score}));
    near(n(&r["screen_corners"][0][0]), 215.0);
    near(n(&r["screen_corners"][0][1]), 477.2);
    near(n(&r["world_corners"][0][0]), -7.851852);
    near(n(&r["world_corners"][0][1]), 6.139259);
    near(n(&r["world_corners"][0][2]), 90.0);
    w.req(json!({"op":"ui.configure","viewport":[1920,1080]}));
    let large = w.req(json!({"op":"ui.rect","object":score}));
    near(n(&large["screen_corners"][0][0]), 430.0);
    near(
        n(&large["world_corners"][0][0]),
        n(&r["world_corners"][0][0]),
    );
}

#[test]
fn font_backed_text_emits_readable_non_digit_glyph_masks_and_updates_from_native_text() {
    let w = World::new();
    w.font();
    let root = w.create();
    w.add(root, "RectTransform", json!({"size_delta":[400,200]}));
    w.add(root, "Canvas", json!({"render_mode":"world_space"}));
    let score = w.child(root, json!({"size_delta":[200,100]}));
    let text = w.add(
        score,
        "Text",
        json!({"text":"0","font_size":72,"alignment":4}),
    );
    let first = w.req(json!({"op":"ui.layout"}));
    let g = &first["texts"][0]["glyphs"][0];
    assert_eq!(g["character"], "0");
    let mask = STANDARD
        .decode(g["alpha_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        mask.len(),
        g["width"].as_u64().unwrap() as usize * g["height"].as_u64().unwrap() as usize
    );
    assert!(mask.contains(&0));
    assert!(mask.iter().any(|p| *p > 240));
    assert!(mask.iter().any(|p| *p > 0 && *p < 240));
    assert!(n(&g["rect"][2]) > n(&g["rect"][0]));
    assert_eq!(g["world_corners"].as_array().unwrap().len(), 4);
    w.req(
        json!({"op":"component.set","component":text,"fields":{"text":"AVé\n12","font_size":32}}),
    );
    let updated = w.req(json!({"op":"ui.layout"}));
    assert_eq!(updated["texts"][0]["glyphs"].as_array().unwrap().len(), 5);
    assert_eq!(updated["texts"][0]["glyphs"][2]["character"], "é");
    assert_ne!(
        updated["texts"][0]["glyphs"][0]["alpha_base64"],
        g["alpha_base64"]
    );
    assert!(
        n(&updated["texts"][0]["glyphs"][3]["rect"][1])
            < n(&updated["texts"][0]["glyphs"][0]["rect"][1])
    );
    w.req(json!({"op":"component.set","component":text,"enabled":false}));
    assert_eq!(w.req(json!({"op":"ui.layout"}))["texts"], json!([]));
}

#[test]
fn ui_invalid_updates_missing_fonts_and_removed_owners_fail_without_ghost_geometry() {
    let w = World::new();
    let o = w.create();
    let r = w.add(o, "RectTransform", json!({"size_delta":[100,100]}));
    w.add(o, "Canvas", json!({"render_mode":"world_space"}));
    w.add(o, "Text", json!({"text":"score"}));
    assert_eq!(
        request_value(w.0, json!({"op":"ui.layout"})).unwrap()["ok"],
        false
    );
    let before = w.req(json!({"op":"component.get","component":r}));
    assert_eq!(
        request_value(
            w.0,
            json!({"op":"component.set","component":r,"fields":{"pivot":[2,0]}})
        )
        .unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"component.get","component":r})), before);
    assert_eq!(
        request_value(
            w.0,
            json!({"op":"font.register","name":"default","data_base64":"invalid"})
        )
        .unwrap()["ok"],
        false
    );
    w.req(json!({"op":"object.destroy","object":o}));
    assert_eq!(w.req(json!({"op":"ui.layout"}))["texts"], json!([]));
}

#[test]
fn screen_canvas_owns_derived_pose_even_when_source_scale_is_zero() {
    let w = World::new();
    let canvas = w.create();
    w.add(canvas, "RectTransform", json!({"pivot":[0,0]}));
    w.req(json!({"op":"transform.set","object":canvas,"scale":[0,0,0]}));
    w.add(
        canvas,
        "Canvas",
        json!({"render_mode":"screen_space_overlay"}),
    );
    w.req(json!({"op":"ui.configure","viewport":[800,600]}));
    let child = w.child(canvas, json!({"size_delta":[100,50]}));
    let rect = w.req(json!({"op":"ui.rect","object":child}));
    assert_eq!(rect["screen_corners"][0], json!([350.0, 275.0]));
    assert_eq!(rect["screen_corners"][2], json!([450.0, 325.0]));
}

#[test]
fn singular_parent_rejects_world_inverse_without_mutating_child() {
    let w = World::new();
    let parent = w.create();
    let child = w.create();
    w.req(json!({"op":"transform.set","object":parent,"scale":[0,1,1]}));
    w.req(json!({"op":"transform.parent","object":child,"parent":parent,"world_position_stays":false}));
    let before = w.req(json!({"op":"transform.get","object":child}));
    assert_eq!(
        request_value(
            w.0,
            json!({"op":"transform.set","object":child,"position":[1,2,0],"space":"world"})
        )
        .unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"transform.get","object":child})), before);
}

#[test]
fn score_mask_changes_at_same_font_size_and_parent_visibility_controls_geometry() {
    let w = World::new();
    w.font();
    let canvas = w.create();
    w.add(canvas, "RectTransform", json!({}));
    let canvas_c = w.add(canvas, "Canvas", json!({}));
    let parent = w.child(canvas, json!({"size_delta":[400,200]}));
    let label = w.child(parent, json!({"size_delta":[200,100]}));
    let text = w.add(
        label,
        "Text",
        json!({"text":"0","font_size":72,"alignment":4}),
    );
    let first = w.req(json!({"op":"ui.layout"}));
    w.req(json!({"op":"component.set","component":text,"fields":{"text":"1"}}));
    let next = w.req(json!({"op":"ui.layout"}));
    assert_eq!(next["texts"][0]["text"], "1");
    assert_eq!(next["texts"][0]["rect"], first["texts"][0]["rect"]);
    assert_ne!(
        next["texts"][0]["glyphs"][0]["alpha_base64"],
        first["texts"][0]["glyphs"][0]["alpha_base64"]
    );
    w.req(json!({"op":"object.set","object":parent,"active":false}));
    assert_eq!(w.req(json!({"op":"ui.layout"}))["texts"], json!([]));
    w.req(json!({"op":"object.set","object":parent,"active":true}));
    assert_eq!(w.req(json!({"op":"ui.layout"}))["texts"], next["texts"]);
    w.req(json!({"op":"component.set","component":canvas_c,"enabled":false}));
    assert_eq!(w.req(json!({"op":"ui.layout"}))["texts"], json!([]));
}

#[test]
fn resize_matches_canvas_height_without_moving_non_ui_world_objects() {
    let w = World::new();
    let regular = w.create();
    w.req(json!({"op":"transform.set","object":regular,"position":[3,4,5],"scale":[2,3,4]}));
    let before = w.req(json!({"op":"transform.get","object":regular}));
    let canvas = w.create();
    w.add(canvas, "RectTransform", json!({}));
    w.add(canvas, "Canvas", json!({}));
    w.add(canvas, "CanvasScaler", json!({"scale_mode":"scale_with_screen_size","reference_resolution":[800,600],"match_width_or_height":1}));
    let label = w.child(
        canvas,
        json!({"anchor_min":[1,1],"anchor_max":[1,1],"pivot":[1,1],"size_delta":[100,50]}),
    );
    w.req(json!({"op":"ui.configure","viewport":[800,600]}));
    let small = w.req(json!({"op":"ui.rect","object":label}));
    assert_eq!(small["screen_corners"][0], json!([700.0, 550.0]));
    w.req(json!({"op":"ui.configure","viewport":[1600,900]}));
    let large = w.req(json!({"op":"ui.rect","object":label}));
    near(n(&large["screen_corners"][0][0]), 1450.0);
    near(n(&large["screen_corners"][0][1]), 825.0);
    assert_eq!(
        w.req(json!({"op":"transform.get","object":regular})),
        before
    );
}

#[test]
fn font_registration_is_world_owned_survives_subtree_clone_and_invalid_replacement() {
    let w = World::new();
    w.font();
    let canvas = w.create();
    w.add(canvas, "RectTransform", json!({"size_delta":[200,100]}));
    w.add(canvas, "Canvas", json!({"render_mode":"world_space"}));
    w.add(canvas, "Text", json!({"text":"Font Ω","font_size":24}));
    let initial = w.req(json!({"op":"ui.layout"}))["texts"][0]["glyphs"].clone();
    let invalid = request_value(w.0,json!({"op":"font.register","name":"default","data_base64":STANDARD.encode(b"invalid-font")})).unwrap();
    assert_eq!(invalid["ok"], false);
    assert_eq!(
        w.req(json!({"op":"ui.layout"}))["texts"][0]["glyphs"],
        initial
    );
    w.req(json!({"op":"object.clone","object":canvas}));
    w.req(json!({"op":"object.destroy","object":canvas}));
    assert_eq!(
        w.req(json!({"op":"ui.layout"}))["texts"][0]["glyphs"],
        initial
    );
    drop(w);
    let fresh = World::new();
    let canvas = fresh.create();
    fresh.add(canvas, "RectTransform", json!({}));
    fresh.add(canvas, "Canvas", json!({}));
    fresh.add(canvas, "Text", json!({"text":"Font Ω"}));
    let error = request_value(fresh.0, json!({"op":"ui.layout"})).unwrap();
    assert_eq!(error["ok"], false);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("font is not registered")
    );
}
