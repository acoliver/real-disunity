use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};

#[test]
fn original_body_constraints_and_body_or_collider_materials_are_native_typed_data() {
    for (game, scene) in [("breakout", "SampleScene"), ("pong", "Main")] {
        let graph = import_project(
            &worldrunner_original_games::checkout(game.parse().unwrap()).unwrap(),
            format!("Assets/Scenes/{scene}.unity").as_ref(),
            &ImportProfile::default(),
            &ImportSidecar::default(),
        )
        .unwrap();
        for body in graph.components.iter().filter(|c| c.kind == "Rigidbody2D") {
            assert_eq!(
                body.native_fields["constraints"],
                body.fields["m_Constraints"]
            );
            assert_eq!(body.native_fields["angular_drag"].as_f64(), Some(0.));
            assert_eq!(
                body.native_fields["material"]["bounciness"].as_f64(),
                Some(1.)
            );
            assert_eq!(
                body.native_fields["material"]["asset"],
                body.fields["m_Material"]
            );
        }
        for collider in graph.components.iter().filter(|c| {
            matches!(
                c.kind.as_str(),
                "BoxCollider2D" | "CircleCollider2D" | "PolygonCollider2D"
            )
        }) {
            if collider.fields["m_Material"].get("$ref").is_some() {
                assert_eq!(
                    collider.native_fields["material"]["asset"],
                    collider.fields["m_Material"]
                );
            }
        }
    }
}
