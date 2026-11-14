use std::path::PathBuf;

use archunit::{
    ImportKind, ProjectLocator, SourceOptions, extract_dependencies, extract_graph,
    locate_project_from,
};

#[test]
fn nested_self_crate_alias_resolves_source_references_and_graph_edges() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/self_crate_alias_project");
    let project = locate_project_from(&ProjectLocator::from_path(fixture))
        .expect("self-crate alias fixture should be discoverable");
    let dependencies = extract_dependencies(&project, SourceOptions::default())
        .expect("self-crate alias dependencies should extract");

    for (path, target, kind, count) in [
        ("self", "src/lib.rs", ImportKind::ExternCrate, 1),
        ("local_crate::Item", "src/lib.rs", ImportKind::PathReference, 2),
        (
            "local_crate::api::Model",
            "src/api.rs",
            ImportKind::PathReference,
            1,
        ),
    ] {
        let references = dependencies
            .references()
            .iter()
            .filter(|reference| {
                reference.source() == "src/consumer.rs"
                    && reference.referenced_path() == path
                    && reference.kind() == kind
            })
            .collect::<Vec<_>>();
        assert_eq!(
            references.len(),
            count,
            "expected {count} {kind:?} references to {path}",
        );
        for reference in references {
            assert_eq!(
                reference.internal_target(),
                Some(target),
                "{path} at line {} should resolve to {target}",
                reference.line(),
            );
            assert!(reference.line() > 0);
        }
    }
    assert!(
        dependencies.diagnostics().is_empty(),
        "unexpected dependency diagnostics: {:?}",
        dependencies.diagnostics(),
    );

    let graph = extract_graph(&project, SourceOptions::default())
        .expect("self-crate alias graph should extract");
    assert!(
        graph.diagnostics().is_empty(),
        "unexpected graph diagnostics: {:?}",
        graph.diagnostics(),
    );
    for (target, kinds) in [
        (
            "src/lib.rs",
            vec![ImportKind::ExternCrate, ImportKind::PathReference],
        ),
        ("src/api.rs", vec![ImportKind::PathReference]),
    ] {
        let edge = graph
            .graph()
            .iter()
            .find(|edge| edge.source == "src/consumer.rs" && edge.target == target)
            .unwrap_or_else(|| panic!("expected consumer-to-{target} edge"));
        assert!(!edge.external);
        assert_eq!(edge.import_kinds.iter().collect::<Vec<_>>(), kinds);
    }
}
