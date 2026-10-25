use worldrunner_unity_import::{
    Limits, Node, ObjectRef, ScalarStyle, YamlError,
    yaml::{parse_documents, parse_plain},
};

const SCENE: &str = "\u{feff}%YAML 1.1\r
%TAG !u! tag:unity3d.com,2011:\r
--- !u!1 &100\r
GameObject:\r
  m_ObjectHideFlags: 0\r
  serializedVersion: 6\r
  m_Component:\r
  - component: {fileID: 400}\r
  - component: {fileID: 21200}\r
  m_Layer: 0\r
  m_Name: Main Camera\r
  m_IsActive: 1\r
--- !u!4 &400 stripped\r
Transform:\r
  m_CorrespondingSourceObject: {fileID: 4967012201919193289, guid: 311925a002f4447b3a28927169b83ea6,\r
    type: 3}\r
--- !u!212 &-7845977501651575310\r
SpriteRenderer:\r
  m_Color: {r: 1, g: 0.5, b: 0, a: 1}\r
  m_Materials:\r
  - {fileID: 10754, guid: 0000000000000000f000000000000000, type: 0}\r
  m_SortingOrder: -10\r
  m_Size: {x: 0.32, y: 2.56}\r
  m_Empty: []\r
  m_None: {}\r
";

fn scalar<'a>(node: &'a Node, key: &str) -> &'a str {
    node.get(key).and_then(Node::as_scalar).unwrap().text()
}

#[test]
fn documents_keep_class_file_id_stripped_flag_and_type() {
    let documents = parse_documents(SCENE, Limits::default()).unwrap();
    let headers: Vec<_> = documents
        .iter()
        .map(|d| (d.class_id, d.file_id, d.stripped, d.type_name.as_str()))
        .collect();
    assert_eq!(
        headers,
        [
            (1, 100, false, "GameObject"),
            (4, 400, true, "Transform"),
            (212, -7_845_977_501_651_575_310, false, "SpriteRenderer"),
        ]
    );
}

#[test]
fn compact_sequences_flow_references_and_scalars_decode() {
    let documents = parse_documents(SCENE, Limits::default()).unwrap();
    let object = Node::Mapping(documents[0].body.clone());
    let components = object.get("m_Component").unwrap().as_sequence().unwrap();
    let ids: Vec<_> = components
        .iter()
        .map(|c| {
            ObjectRef::from_node(c.get("component").unwrap())
                .unwrap()
                .file_id
        })
        .collect();
    assert_eq!(ids, [400, 21200]);
    assert_eq!(scalar(&object, "m_Name"), "Main Camera");
    assert_eq!(
        object
            .get("m_IsActive")
            .and_then(Node::as_scalar)
            .unwrap()
            .as_bool(),
        Some(true)
    );
    let source = Node::Mapping(documents[1].body.clone());
    let reference =
        ObjectRef::from_node(source.get("m_CorrespondingSourceObject").unwrap()).unwrap();
    assert_eq!(reference.file_id, 4_967_012_201_919_193_289);
    assert_eq!(
        reference.guid.unwrap().to_string(),
        "311925a002f4447b3a28927169b83ea6"
    );
    assert_eq!(reference.type_id, Some(3));
}

#[test]
fn flow_mappings_numbers_and_empty_collections_decode() {
    let documents = parse_documents(SCENE, Limits::default()).unwrap();
    let renderer = Node::Mapping(documents[2].body.clone());
    let color = renderer.get("m_Color").unwrap();
    assert_eq!(
        color.get("g").and_then(Node::as_scalar).unwrap().as_f64(),
        Some(0.5)
    );
    let material = &renderer.get("m_Materials").unwrap().as_sequence().unwrap()[0];
    let material = ObjectRef::from_node(material).unwrap();
    assert_eq!((material.file_id, material.type_id), (10754, Some(0)));
    assert!(material.guid.is_some());
    assert_eq!(
        renderer
            .get("m_SortingOrder")
            .and_then(Node::as_scalar)
            .unwrap()
            .as_i64(),
        Some(-10)
    );
    assert_eq!(
        renderer
            .get("m_Empty")
            .unwrap()
            .as_sequence()
            .unwrap()
            .len(),
        0
    );
    assert!(
        renderer
            .get("m_None")
            .unwrap()
            .as_mapping()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn nested_sequences_of_mappings_and_legacy_repeated_keys_decode() {
    let text = "--- !u!1001 &1\nPrefab:\n  m_Modification:\n    m_Modifications:\n    - target: {fileID: 4}\n      propertyPath: m_LocalPosition.x\n      value: -10\n      objectReference: {fileID: 0}\n    - target: {fileID: 4}\n      propertyPath: m_Name\n      value: \n      objectReference: {fileID: 0}\n  platformData:\n    data:\n      first:\n        Any: \n    data:\n      first:\n        Editor: Editor\n";
    let document = &parse_documents(text, Limits::default()).unwrap()[0];
    let body = Node::Mapping(document.body.clone());
    let modifications = body
        .get("m_Modification")
        .and_then(|m| m.get("m_Modifications"))
        .and_then(Node::as_sequence)
        .unwrap();
    assert_eq!(modifications.len(), 2);
    assert_eq!(
        scalar(&modifications[0], "propertyPath"),
        "m_LocalPosition.x"
    );
    assert_eq!(scalar(&modifications[0], "value"), "-10");
    assert!(
        modifications[1]
            .get("value")
            .and_then(Node::as_scalar)
            .unwrap()
            .is_empty()
    );
    assert!(
        ObjectRef::from_node(modifications[1].get("objectReference").unwrap())
            .unwrap()
            .is_null()
    );
    let data = body.get("platformData").and_then(Node::as_mapping).unwrap();
    assert_eq!(data.get_all("data").count(), 2);
}

#[test]
fn quoted_and_multi_line_scalars_fold_like_yaml() {
    let text = "a: 'it''s\n  two words'\nb: \"tab\\tline\\nsnow \\u2603 \\\n  joined\"\nc: a long plain\n  value\n\n  with a break\nd: '  leading'\n";
    let mapping = Node::Mapping(parse_plain(text, Limits::default()).unwrap());
    assert_eq!(scalar(&mapping, "a"), "it's two words");
    assert_eq!(scalar(&mapping, "b"), "tab\tline\nsnow \u{2603} joined");
    assert_eq!(scalar(&mapping, "c"), "a long plain value\nwith a break");
    assert_eq!(scalar(&mapping, "d"), "  leading");
    let quoted = mapping.get("a").and_then(Node::as_scalar).unwrap();
    assert_eq!(quoted.style(), ScalarStyle::SingleQuoted);
    assert_eq!(quoted.as_i64(), None);
    let one = parse_plain("x: '1'\n", Limits::default()).unwrap();
    assert_eq!(
        one.get("x").and_then(Node::as_scalar).unwrap().as_i64(),
        None
    );
}

#[test]
fn nested_sequences_and_items_with_block_values_decode() {
    let text = "list:\n- - 1\n  - 2\n- key:\n  - a\n  other: x\n-\n  inner: 3\n- 'quoted'\n";
    let mapping = Node::Mapping(parse_plain(text, Limits::default()).unwrap());
    let list = mapping.get("list").and_then(Node::as_sequence).unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(list[0].as_sequence().unwrap().len(), 2);
    assert_eq!(
        list[1]
            .get("key")
            .and_then(Node::as_sequence)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(scalar(&list[1], "other"), "x");
    assert_eq!(scalar(&list[2], "inner"), "3");
    assert_eq!(list[3].as_scalar().unwrap().text(), "quoted");
}

fn syntax_line(result: Result<impl std::fmt::Debug, YamlError>) -> usize {
    match result.unwrap_err() {
        YamlError::Syntax { line, .. } => line,
        YamlError::Limit(limit) => panic!("expected a syntax error, got the {limit} limit"),
    }
}

#[test]
fn malformed_files_report_the_offending_line() {
    let limits = Limits::default();
    assert_eq!(
        syntax_line(parse_documents("a: 1\n--- !u!1 &1\nX:\n  b: 2\n", limits)),
        1
    );
    assert_eq!(
        syntax_line(parse_documents(
            "--- !u!1 &1\nX: {}\n--- !u!2 &1\nY: {}\n",
            limits
        )),
        3
    );
    assert_eq!(
        syntax_line(parse_documents("--- !u!x &1\nX: {}\n", limits)),
        1
    );
    assert_eq!(
        syntax_line(parse_documents("--- !u!1 &1 extra\nX: {}\n", limits)),
        1
    );
    assert_eq!(
        syntax_line(parse_documents(
            "--- !u!1 &1\nX:\n  a: 1\nY:\n  b: 2\n",
            limits
        )),
        1
    );
    assert_eq!(syntax_line(parse_plain("a:\n\tb: 1\n", limits)), 2);
    assert_eq!(syntax_line(parse_plain("a: {b: 1\nc: 2\n", limits)), 1);
    assert_eq!(syntax_line(parse_plain("a: 'open\nb: 2\n", limits)), 1);
    assert_eq!(syntax_line(parse_plain("a:\n  b: 1\n c: 2\n", limits)), 3);
    assert_eq!(syntax_line(parse_plain("a: \"\\q\"\n", limits)), 1);
    assert_eq!(syntax_line(parse_plain("a: 'x' y\n", limits)), 1);
    assert_eq!(syntax_line(parse_plain("a: {b 1}\n", limits)), 1);
    assert_eq!(syntax_line(parse_plain("just text\n", limits)), 1);
    assert_eq!(syntax_line(parse_documents("", limits)), 1);
}

#[test]
fn limits_bound_document_count_and_nesting() {
    let small = Limits {
        max_documents: 1,
        max_depth: 3,
    };
    assert_eq!(
        parse_documents("--- !u!1 &1\nX: {}\n--- !u!1 &2\nX: {}\n", small).unwrap_err(),
        YamlError::Limit("document count")
    );
    assert_eq!(
        parse_plain("a:\n  b:\n    c:\n      d: 1\n", small).unwrap_err(),
        YamlError::Limit("nesting depth")
    );
    assert_eq!(
        parse_plain("a: [[[[1]]]]\n", small).unwrap_err(),
        YamlError::Limit("nesting depth")
    );
    assert!(parse_plain("a:\n  b: [1]\n", small).is_ok());
}
