//! A host restores an original engine's saved Lua heap through `GameScripts` (#147): the heap
//! is imported with `lua_resume::import`, restored against the chunks the scripts loaded, and
//! its threads continue where the original stopped.

mod world;

use lua_resume::capture::{Permanents, Snapshot, TableData, Value as Saved};
use lua_resume::import::{ImportChunk, ImportSources, PermanentMap, import_threads};
use lua_resume::syntax::parse;
use mlua::{Function, Table, Thread, Value};
use worldrunner_script::{GameScripts, PermanentSpec, ScriptConfig};

use world::{SAVED_SOURCE, SOURCE, World};

const CHUNK: &str = "=cases.lua";

/// A state that ran `cases.lua` as the game's source, with one native and a global table of
/// classes.
fn scripts() -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_native("Ping", |_, ()| Ok("pong".to_owned()))
        .unwrap();
    scripts.add_source("game", &["Ping"]).unwrap();
    scripts.run("game", CHUNK, SOURCE.as_bytes()).unwrap();
    scripts
        .run(
            "game",
            "=classes.lua",
            b"Classes = { Unit = { kind = 'unit' } }",
        )
        .unwrap();
    scripts
}

/// The chunk of `cases.lua` as the saved heap and this state know it.
fn import_sources(scripts: &GameScripts) -> ImportSources {
    let normalized = scripts.restore_sources().get(CHUNK).unwrap().clone();
    let stripped = parse(SOURCE.as_bytes()).unwrap();
    let mut sources = ImportSources::new();
    let chunk = ImportChunk::new(CHUNK, &stripped, normalized).unwrap();
    sources.insert(SAVED_SOURCE, chunk);
    sources
}

fn import(scripts: &GameScripts, world: &World, known: &Permanents) -> Snapshot {
    import_threads(
        std::slice::from_ref(&world.thread),
        &world.heap,
        &import_sources(scripts),
        known,
        &PermanentMap::default(),
    )
    .unwrap()
}

fn text(value: &str) -> Saved {
    Saved::String(value.as_bytes().to_vec())
}

fn root<'a>(roots: &'a [(String, Value)], name: &str) -> &'a Value {
    &roots.iter().find(|(root, _)| root == name).unwrap().1
}

#[test]
fn an_imported_thread_resumes_where_the_original_stopped() {
    let scripts = scripts();
    let world = World::nested_in_leaf();
    let known = scripts.permanents(&PermanentSpec::default()).unwrap();
    let snapshot = import(&scripts, &world, &known);

    let restored = scripts.restore_imported(&snapshot, &known).unwrap();

    assert_eq!(restored.threads.len(), 1);
    // `leaf` returns the resumed value plus one and `nested` doubles it.
    let finished: i64 = restored.threads[0].resume(7).unwrap();
    assert_eq!(finished, 16);
}

#[test]
fn shared_tables_cycles_and_closures_keep_their_identity() {
    let scripts = scripts();
    let mut world = World::nested_in_leaf();
    // `nested_in_leaf` makes `leaf` the first closure of the heap.
    let leaf = 0;
    let entries = |pairs: &[(&str, Saved)]| TableData {
        entries: pairs.iter().map(|(k, v)| (text(k), v.clone())).collect(),
        metatable: None,
    };
    world.heap.tables = vec![
        entries(&[("self", Saved::Table(0)), ("shared", Saved::Table(2))]),
        entries(&[("shared", Saved::Table(2))]),
        entries(&[("count", Saved::Integer(3))]),
    ];
    world.heap.roots = vec![
        ("first".to_owned(), Saved::Table(0)),
        ("second".to_owned(), Saved::Table(1)),
        ("leaf".to_owned(), Saved::Closure(leaf)),
    ];
    let known = scripts.permanents(&PermanentSpec::default()).unwrap();
    let snapshot = import(&scripts, &world, &known);

    let restored = scripts.restore_imported(&snapshot, &known).unwrap();

    let Value::Table(first) = root(&restored.roots, "first").clone() else {
        panic!("first is a table");
    };
    let Value::Table(second) = root(&restored.roots, "second").clone() else {
        panic!("second is a table");
    };
    assert_eq!(first.get::<Table>("self").unwrap(), first);
    let shared: Table = first.get("shared").unwrap();
    assert_eq!(shared, second.get::<Table>("shared").unwrap());
    assert_eq!(shared.get::<i64>("count").unwrap(), 3);
    let Value::Function(leaf) = root(&restored.roots, "leaf").clone() else {
        panic!("leaf is a function");
    };
    let coroutine: Thread = scripts.lua().create_thread(leaf).unwrap();
    let (tag, got): (String, i64) = coroutine.resume(9).unwrap();
    assert_eq!((tag.as_str(), got), ("leaf", 9));
    assert_eq!(coroutine.resume::<i64>(3).unwrap(), 4);
    let finished: i64 = restored.threads[0].resume(0).unwrap();
    assert_eq!(finished, 2);
}

#[test]
fn permanents_resolve_by_native_path_global_path_and_host_supplied_name() {
    let scripts = scripts();
    let label_table = scripts.lua().create_table().unwrap();
    let print: Function = scripts.lua().globals().get("print").unwrap();
    let spec = PermanentSpec {
        paths: vec!["Classes".to_owned()],
        extra: vec![
            (
                "original label".to_owned(),
                Value::Table(label_table.clone()),
            ),
            ("original print".to_owned(), Value::Function(print.clone())),
        ],
    };
    let known = scripts.permanents(&spec).unwrap();
    let mut world = World::nested_in_leaf();
    world.heap.roots = ["label", "alias", "native", "class"]
        .into_iter()
        .zip([
            "original label",
            "original print",
            "native Ping",
            "permanent Classes.Unit",
        ])
        .map(|(root, name)| (root.to_owned(), Saved::Permanent(name.to_owned())))
        .collect();
    let snapshot = import(&scripts, &world, &known);

    let restored = scripts.restore_imported(&snapshot, &known).unwrap();

    assert_eq!(root(&restored.roots, "label"), &Value::Table(label_table));
    assert_eq!(root(&restored.roots, "alias"), &Value::Function(print));
    let Value::Function(ping) = root(&restored.roots, "native") else {
        panic!("a native restores as a function");
    };
    assert_eq!(ping.call::<String>(()).unwrap(), "pong");
    let Value::Table(class) = root(&restored.roots, "class") else {
        panic!("a class restores as a table");
    };
    assert_eq!(class.get::<String>("kind").unwrap(), "unit");
    let classes: Table = scripts.lua().globals().get("Classes").unwrap();
    assert_eq!(&classes.get::<Table>("Unit").unwrap(), class);
}

#[test]
fn a_label_the_host_names_twice_is_refused() {
    let scripts = scripts();
    let table = scripts.lua().create_table().unwrap();
    let spec = PermanentSpec {
        paths: Vec::new(),
        extra: vec![
            ("label".to_owned(), Value::Table(table.clone())),
            (
                "label".to_owned(),
                Value::Table(scripts.lua().create_table().unwrap()),
            ),
        ],
    };
    let error = scripts.permanents(&spec).unwrap_err();
    assert!(error.to_string().contains("label"), "{error}");
}

#[test]
fn check_imported_lists_a_permanent_this_state_lacks_without_restoring() {
    let scripts = scripts();
    let label = Value::Table(scripts.lua().create_table().unwrap());
    let importing = scripts
        .permanents(&PermanentSpec {
            paths: Vec::new(),
            extra: vec![("game label".to_owned(), label)],
        })
        .unwrap();
    let mut world = World::nested_in_leaf();
    world.heap.roots = vec![(
        "label".to_owned(),
        Saved::Permanent("game label".to_owned()),
    )];
    let snapshot = import(&scripts, &world, &importing);
    assert!(scripts.check_imported(&snapshot, &importing).is_empty());

    let without = scripts.permanents(&PermanentSpec::default()).unwrap();
    let refusals = scripts.check_imported(&snapshot, &without);

    assert!(!refusals.is_empty());
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.to_string().contains("game label")),
        "{refusals:?}"
    );
    assert!(scripts.restore_imported(&snapshot, &without).is_err());
}

#[test]
fn scripts_cannot_see_the_debug_library_the_host_restores_with() {
    let mut scripts = scripts();
    let seen = scripts
        .eval_json(
            "game",
            "return { debug = rawget(_G, 'debug') ~= nil, visible = debug ~= nil, \
             package = package ~= nil, restore = restore_imported ~= nil }",
        )
        .unwrap();
    assert_eq!(
        seen,
        serde_json::json!({"debug": false, "visible": false, "package": false, "restore": false})
    );
    let world = World::nested_in_leaf();
    let known = scripts.permanents(&PermanentSpec::default()).unwrap();
    let snapshot = import(&scripts, &world, &known);
    scripts.restore_imported(&snapshot, &known).unwrap();
    let after = scripts
        .eval_json("game", "return debug == nil and rawget(_G, 'debug') == nil")
        .unwrap();
    assert_eq!(after, serde_json::json!(true));
}
