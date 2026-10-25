use crate::{
    files::{self, Result, io},
    model::Fixture,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn load(root: &Path, name: &str) -> Result<Fixture> {
    let requested = Path::new(name);
    let path = if requested.extension().is_some_and(|ext| ext == "json") {
        if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            root.join(requested)
        }
    } else {
        files::relative_path(requested)?;
        if requested.components().count() != 1 {
            return Err("fixture name must be one path component".into());
        }
        let path = root.join("fixtures").join(format!("{name}.json"));
        if !path.exists() {
            return catalog(root)?
                .into_iter()
                .find(|fixture| fixture.name == name)
                .ok_or_else(|| format!("unknown fixture: {name}"));
        }
        path
    };
    let fixture: Fixture = files::read_json(&path)?;
    if fixture.schema_version != 1 {
        return Err("unsupported fixture schema".into());
    }
    if fixture.name.is_empty()
        || fixture.name.len() > 128
        || !fixture
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("fixture name must contain letters, digits, hyphens or underscores".into());
    }
    fixture
        .game
        .parse::<worldrunner_original_games::Game>()
        .map_err(io)?;
    fixture.limits.validate()?;
    if fixture.profiles.is_empty() || !fixture.profiles.contains(&fixture.default_profile) {
        return Err("fixture default profile must exist".into());
    }
    crate::reporting::Selection::from_fixture(&fixture)?;
    Ok(fixture)
}
pub fn catalog(root: &Path) -> Result<Vec<Fixture>> {
    let mut fixtures = Vec::new();
    for entry in fs::read_dir(root.join("fixtures")).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            fixtures.push(load(root, path.to_str().ok_or("non UTF8 fixture path")?)?);
        }
    }
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    if fixtures.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err("duplicate fixture display names in catalog".into());
    }
    Ok(fixtures)
}
pub fn source_root(fixture: &Fixture) -> Result<PathBuf> {
    let game = fixture.game.parse().map_err(io)?;
    worldrunner_original_games::checkout(game).map_err(io)
}
pub fn check(fixture: &Fixture) -> Result<serde_json::Value> {
    let source = source_root(fixture)?;
    let editor = fs::read_to_string(source.join("ProjectSettings/ProjectVersion.txt")).ok();
    Ok(
        serde_json::json!({"fixture":fixture.name,"game":fixture.game,"source_root":source,
        "editor":editor,"profiles":fixture.profiles,"metadata":fixture.metadata}),
    )
}
