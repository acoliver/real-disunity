use crate::{ContentReceipt, ImportedScene};

pub(super) fn validate(receipt: &ContentReceipt, graph: &ImportedScene) -> Result<(), String> {
    for doc in &graph.documents {
        let supported = match (doc.class_id, doc.kind.as_str()) {
            (1, "GameObject")
            | (4, "Transform")
            | (224, "RectTransform")
            | (20, "Camera")
            | (212, "SpriteRenderer")
            | (50, "Rigidbody2D")
            | (58, "CircleCollider2D")
            | (60, "PolygonCollider2D")
            | (61, "BoxCollider2D")
            | (19_719_996, "TilemapCollider2D")
            | (1_839_735_485, "Tilemap")
            | (21, "Material")
            | (62, "PhysicsMaterial2D")
            | (1001, "PrefabInstance") => true,
            (114, "MonoBehaviour") => graph
                .components
                .iter()
                .any(|c| c.source == doc.source && c.script.is_some()),
            _ => false,
        };
        if !supported {
            return Err(format!(
                "selected unsupported record: {} {}",
                doc.class_id, doc.kind
            ));
        }
    }
    for path in graph
        .metadata_records
        .keys()
        .filter(|p| p.starts_with("ProjectSettings/"))
    {
        let bytes = receipt
            .sources()
            .get(path)
            .ok_or("selected settings source absent")?;
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        for doc in crate::yaml::parse_unity_yaml(text, 100_000)? {
            if !settings(path, doc.class_id, &doc.kind) {
                return Err(format!(
                    "selected unsupported settings record: {path} {} {}",
                    doc.class_id, doc.kind
                ));
            }
        }
    }
    Ok(())
}

fn settings(path: &str, class: i32, kind: &str) -> bool {
    matches!(
        (path, class, kind),
        (
            "ProjectSettings/GraphicsSettings.asset",
            30,
            "GraphicsSettings"
        ) | (
            "ProjectSettings/QualitySettings.asset",
            47,
            "QualitySettings"
        ) | ("ProjectSettings/InputManager.asset", 13, "InputManager")
            | ("ProjectSettings/TagManager.asset", 78, "TagManager")
            | ("ProjectSettings/TimeManager.asset", 5, "TimeManager")
            | (
                "ProjectSettings/Physics2DSettings.asset",
                19,
                "Physics2DSettings"
            )
            | (
                "ProjectSettings/DynamicsManager.asset",
                55,
                "PhysicsManager"
            )
            | ("ProjectSettings/AudioManager.asset", 11, "AudioManager")
            | (
                "ProjectSettings/ProjectSettings.asset",
                129,
                "PlayerSettings"
            )
            | (
                "ProjectSettings/EditorSettings.asset",
                159,
                "EditorSettings"
            )
    )
}
