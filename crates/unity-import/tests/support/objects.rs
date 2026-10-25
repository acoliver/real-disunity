//! Serialized scene objects for tests.

use std::{fmt::Write, path::Path};

use worldrunner_unity_import::{Scene, SceneError, import_scene};

use crate::fixture::Fixture;

pub fn import(fixture: &Fixture, scene: &str, text: &str) -> Result<Scene, SceneError> {
    fixture.file(scene, text);
    import_scene(&fixture.project(), Path::new(scene))
}

pub fn game_object(id: u64, name: &str, components: &[u64]) -> String {
    let mut text = format!("--- !u!1 &{id}\nGameObject:\n  m_Component:\n");
    for component in components {
        writeln!(text, "  - component: {{fileID: {component}}}").unwrap();
    }
    text + &format!("  m_Layer: 0\n  m_Name: {name}\n  m_IsActive: 1\n")
}

pub fn transform(id: u64, object: u64, x: f64, father: u64) -> String {
    format!(
        "--- !u!4 &{id}\nTransform:\n  m_GameObject: {{fileID: {object}}}\n  m_LocalRotation: {{x: 0, y: 0, z: 0, w: 1}}\n  m_LocalPosition: {{x: {x}, y: 0, z: 0}}\n  m_LocalScale: {{x: 1, y: 1, z: 1}}\n  m_Children: []\n  m_Father: {{fileID: {father}}}\n"
    )
}
