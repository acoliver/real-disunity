use std::{cell::Cell, fs, path::Path};
use worldrunner_compat_content::{
    ContentSource, FileContentSource, ImportError, ImportProfile, ImportSidecar,
    import_receipt_with_source,
};

struct ChangingSource {
    changed: Cell<bool>,
    change: u8,
}
impl ContentSource for ChangingSource {
    fn read(&self, root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, ImportError> {
        let bytes = FileContentSource.read(root, relative, limit)?;
        if relative == "Assets/Main.unity" && !self.changed.replace(true) {
            if self.change >= 2 {
                fs::write(root.join("Assets/Duplicate.unity"), &bytes).unwrap();
                fs::write(
                    root.join("Assets/Duplicate.unity.meta"),
                    if self.change == 2 {
                        "guid: 11111111111111111111111111111111\n"
                    } else {
                        "guid: 22222222222222222222222222222222\n"
                    },
                )
                .unwrap();
            } else if self.change == 1 {
                fs::create_dir_all(root.join("Assets/Scripts")).unwrap();
                fs::write(root.join("Assets/Scripts/New.cs"), "class New {}\n").unwrap();
            } else {
                fs::write(
                    root.join(relative),
                    String::from_utf8(bytes.clone())
                        .unwrap()
                        .replace("Original", "Modified"),
                )
                .unwrap();
            }
        }
        Ok(bytes)
    }
}

#[test]
fn source_changed_after_content_read_is_rejected_before_receipt_publication() {
    for change in [0, 1, 2] {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("Assets")).unwrap();
        fs::write(
            temp.path().join("Assets/Main.unity"),
            "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("Assets/Main.unity.meta"),
            "guid: 11111111111111111111111111111111\n",
        )
        .unwrap();
        let source = ChangingSource {
            changed: Cell::new(false),
            change,
        };
        let result = import_receipt_with_source(
            temp.path(),
            "Assets/Main.unity".as_ref(),
            &ImportProfile::default(),
            &ImportSidecar::default(),
            &source,
        );
        let Err(error) = result else {
            panic!("changed source was published: change={change}")
        };
        assert!(error.to_string().contains("changed"), "{error}");
    }
}

#[test]
fn unrelated_catalogue_additions_during_read_do_not_freeze_the_project() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("Assets")).unwrap();
    fs::write(
        temp.path().join("Assets/Main.unity"),
        "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("Assets/Main.unity.meta"),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    let source = ChangingSource {
        changed: Cell::new(false),
        change: 3,
    };
    let receipt = import_receipt_with_source(
        temp.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
        &source,
    )
    .unwrap();
    assert_eq!(receipt.graph().nodes[0].name, "Original");
    assert!(!receipt.sources().contains_key("Assets/Duplicate.unity"));
    receipt.verify_current(temp.path()).unwrap();
}
