#[path = "architecture/graph.rs"]
mod graph;
use std::path::Path;

#[test]
fn runtime_loads_the_one_native_image_instead_of_linking_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    graph::assert_runtime_graph(&root);
}

#[test]
fn unity_consumer_has_only_the_engine_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    graph::assert_managed_graph(&root);
}
