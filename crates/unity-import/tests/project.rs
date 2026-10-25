use std::{fs, path::Path};

use worldrunner_unity_import::{Guid, ImportError, ObjectRef, Project, ReferenceError, yaml};

const SCENE_GUID: &str = "9fc0d4010bbf28b4594072e72b8655ab";
const TEXTURE_GUID: &str = "e9ba3f3b1d8d1e44c9b1bf4c19b3d0a7";

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    write(
        dir,
        "ProjectSettings/ProjectVersion.txt",
        "m_EditorVersion: 2017.1.0f3\n",
    );
    write(
        dir,
        "Assets/Scenes.meta",
        "fileFormatVersion: 2\nguid: 0123456789abcdef0123456789abcdef\nfolderAsset: yes\nDefaultImporter:\n  userData: \n",
    );
    write(
        dir,
        "Assets/Scenes/Main.unity",
        "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Root\n",
    );
    write(
        dir,
        "Assets/Scenes/Main.unity.meta",
        &format!("fileFormatVersion: 2\nguid: {SCENE_GUID}\nDefaultImporter:\n  userData: \n"),
    );
    write(dir, "Assets/Sprites/Ball.png", "not decoded by the catalog");
    write(
        dir,
        "Assets/Sprites/Ball.png.meta",
        &format!(
            "fileFormatVersion: 2\nguid: {TEXTURE_GUID}\nTextureImporter:\n  spritePixelsToUnits: 100\n"
        ),
    );
    write(
        dir,
        "Assets/Sprites/Gone.png.meta",
        "fileFormatVersion: 2\nguid: ffffffffffffffffffffffffffffffff\n",
    );
    root
}

#[test]
fn catalog_maps_guids_to_assets_importers_and_folders() {
    let root = project();
    let project = Project::open(root.path()).unwrap();
    assert_eq!(project.editor_version(), "2017.1.0f3");
    let scene = project.asset(SCENE_GUID.parse().unwrap()).unwrap();
    assert_eq!(scene.path, Path::new("Assets/Scenes/Main.unity"));
    assert_eq!(scene.importer, "DefaultImporter");
    assert!(!scene.folder);
    let texture = project.asset(TEXTURE_GUID.parse().unwrap()).unwrap();
    assert_eq!(texture.importer, "TextureImporter");
    let folder = project
        .asset("0123456789abcdef0123456789abcdef".parse().unwrap())
        .unwrap();
    assert!(folder.folder);
    assert_eq!(project.assets().count(), 3);
    assert_eq!(
        project.orphan_metas(),
        [Path::new("Assets/Sprites/Gone.png.meta")]
    );
    let documents = project.read_documents(&scene.path).unwrap();
    assert_eq!(documents[0].type_name, "GameObject");
}

#[test]
fn reads_stay_inside_the_project() {
    let root = project();
    let project = Project::open(root.path()).unwrap();
    for path in ["../outside.unity", "/etc/passwd", "Assets/../../x"] {
        assert!(matches!(
            project.read_documents(Path::new(path)),
            Err(ImportError::Escape(_))
        ));
    }
    assert!(matches!(
        project.read_documents(Path::new("Assets/Missing.unity")),
        Err(ImportError::Io { .. })
    ));
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_rejected() {
    let root = project();
    std::os::unix::fs::symlink(
        root.path().join("Assets/Scenes"),
        root.path().join("Assets/Linked"),
    )
    .unwrap();
    assert!(matches!(
        Project::open(root.path()),
        Err(ImportError::Symlink(_))
    ));
    fs::remove_file(root.path().join("Assets/Linked")).unwrap();
    let project = Project::open(root.path()).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", root.path().join("Assets/host.unity")).unwrap();
    assert!(matches!(
        project.read_documents(Path::new("Assets/host.unity")),
        Err(ImportError::Symlink(_))
    ));
}

#[test]
fn invalid_projects_and_metas_are_rejected_with_their_path() {
    let empty = tempfile::tempdir().unwrap();
    assert!(matches!(
        Project::open(empty.path()),
        Err(ImportError::NotProject(_, "Assets"))
    ));
    fs::create_dir(empty.path().join("Assets")).unwrap();
    assert!(matches!(
        Project::open(empty.path()),
        Err(ImportError::NotProject(_, "ProjectVersion.txt"))
    ));
    let root = project();
    write(
        root.path(),
        "Assets/Copy.unity",
        "--- !u!1 &1\nGameObject: {}\n",
    );
    write(
        root.path(),
        "Assets/Copy.unity.meta",
        &format!("guid: {SCENE_GUID}\n"),
    );
    match Project::open(root.path()).unwrap_err() {
        ImportError::DuplicateGuid { first, second, .. } => {
            assert_eq!(first, Path::new("Assets/Copy.unity"));
            assert_eq!(second, Path::new("Assets/Scenes/Main.unity"));
        }
        other => panic!("{other}"),
    }
    write(root.path(), "Assets/Copy.unity.meta", "guid: not-a-guid\n");
    let message = Project::open(root.path()).unwrap_err().to_string();
    assert!(
        message.contains("Copy.unity.meta") && message.contains("not-a-guid"),
        "{message}"
    );
    write(root.path(), "Assets/Copy.unity.meta", "guid: [\n");
    assert!(matches!(
        Project::open(root.path()),
        Err(ImportError::Yaml { .. })
    ));
}

#[test]
fn guids_and_references_validate_their_text() {
    let guid: Guid = "0000000000000000F000000000000000".parse().unwrap();
    assert_eq!(guid.to_string(), "0000000000000000f000000000000000");
    for bad in [
        "",
        "123",
        "0000000000000000g000000000000000",
        "+000000000000000f000000000000000",
    ] {
        assert!(bad.parse::<Guid>().is_err(), "{bad}");
    }
    let parse = |text: &str| {
        let mapping = yaml::parse_plain(&format!("r: {text}\n"), yaml::Limits::default()).unwrap();
        ObjectRef::from_node(mapping.get("r").unwrap())
    };
    let zero = parse("{fileID: 0, guid: 00000000000000000000000000000000, type: 0}").unwrap();
    assert!(zero.is_null() && zero.is_local());
    assert!(!parse("{fileID: 5}").unwrap().is_null());
    assert_eq!(
        parse("{guid: 0123456789abcdef0123456789abcdef}"),
        Err(ReferenceError::Shape)
    );
    assert_eq!(parse("plain"), Err(ReferenceError::Shape));
    assert!(matches!(
        parse("{fileID: 1, guid: xyz}"),
        Err(ReferenceError::Guid(_))
    ));
}
