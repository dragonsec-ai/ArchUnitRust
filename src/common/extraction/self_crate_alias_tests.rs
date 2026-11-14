//! Bounded extraction regressions: not a compiler-complete name resolver.
use std::collections::BTreeMap;

use super::{AliasMap, ResolutionOutcome, collect_internal_aliases, resolve_reference};
use crate::common::extraction::{
    ImportKind,
    cargo_project::CargoDependencyScope,
    dependency::{LogicalModule, RawReference},
};

fn module(segments: &[&str]) -> LogicalModule {
    LogicalModule {
        package: "fixture".to_owned(),
        dependency_scope: CargoDependencyScope::Normal,
        target: "lib".to_owned(),
        segments: segments.iter().map(ToString::to_string).collect(),
    }
}

fn index() -> BTreeMap<LogicalModule, String> {
    [
        (module(&[]), "src/lib.rs"),
        (module(&["consumer"]), "src/consumer.rs"),
        (module(&["sibling"]), "src/sibling.rs"),
        (module(&["api"]), "src/api.rs"),
        (module(&["api", "model"]), "src/api/model.rs"),
        (module(&["facade"]), "src/facade.rs"),
    ]
    .into_iter()
    .map(|(module, source)| (module, source.to_owned()))
    .collect()
}

fn reference(current: &[&str], segments: &[&str], kind: ImportKind) -> RawReference {
    RawReference {
        source: "src/consumer.rs".to_owned(),
        module: module(current),
        segments: segments.iter().map(ToString::to_string).collect(),
        leading_colon: false,
        kind,
        line: 1,
        binding: None,
        declaration: None,
        ignored: false,
    }
}

fn alias(current: &[&str], binding: &str) -> RawReference {
    let mut declaration = reference(current, &["self"], ImportKind::ExternCrate);
    declaration.binding = Some(binding.to_owned());
    declaration
}

fn assert_target(reference: &RawReference, aliases: &AliasMap, expected: &str) {
    let ResolutionOutcome::Found(found) = resolve_reference(reference, &index(), aliases) else {
        panic!("expected unique target {expected} for {}", reference.rendered_path());
    };
    assert_eq!(found.source, expected);
}

#[test]
fn root_self_crate_alias_resolves_root_reexport() {
    let aliases = collect_internal_aliases(&[alias(&[], "this_crate")], &index());
    assert_target(
        &reference(&[], &["this_crate", "Risk"], ImportKind::PathReference),
        &aliases,
        "src/lib.rs",
    );
}

#[test]
fn nested_self_crate_alias_and_its_declaration_resolve_crate_not_module() {
    let declaration = alias(&["consumer"], "this_crate");
    assert_target(&declaration, &AliasMap::new(), "src/lib.rs");
    let aliases = collect_internal_aliases(&[declaration], &index());
    assert_target(
        &reference(&["consumer"], &["this_crate", "Risk"], ImportKind::PathReference),
        &aliases,
        "src/lib.rs",
    );
}

#[test]
fn self_crate_aliases_respect_competing_library_and_binary_roots() {
    let targets = [
        ("lib", "src/lib.rs", "src/consumer.rs"),
        ("bin", "src/main.rs", "src/bin_consumer.rs"),
    ];
    let mut index = BTreeMap::new();
    let mut declarations = Vec::new();
    for (target, root, consumer) in targets {
        let mut root_module = module(&[]);
        root_module.target = target.to_owned();
        index.insert(root_module, root.to_owned());

        let mut declaration = alias(&["consumer"], "local_crate");
        declaration.module.target = target.to_owned();
        declaration.source = consumer.to_owned();
        index.insert(declaration.module.clone(), consumer.to_owned());
        declarations.push(declaration);
    }
    let mut library_only = declarations[0].clone();
    library_only.binding = Some("library_only".to_owned());
    let mut aliases = collect_internal_aliases(&declarations, &index);
    aliases.extend(collect_internal_aliases(&[library_only], &index));

    for (declaration, (target, root, _)) in declarations.iter().zip(targets) {
        let mut qualified = declaration.clone();
        qualified.kind = ImportKind::PathReference;
        qualified.binding = None;
        qualified.segments = vec!["local_crate".to_owned(), "Item".to_owned()];
        for reference in [declaration, &qualified] {
            let ResolutionOutcome::Found(found) = resolve_reference(reference, &index, &aliases)
            else {
                panic!(
                    "expected {} to resolve in target {target}",
                    reference.rendered_path(),
                );
            };
            assert_eq!(found.source, root, "wrong root for target {target}");
        }

        qualified.segments[0] = "library_only".to_owned();
        let outcome = resolve_reference(&qualified, &index, &aliases);
        if target == "lib" {
            assert!(matches!(outcome, ResolutionOutcome::Found(found) if found.source == root));
        } else {
            assert!(matches!(outcome, ResolutionOutcome::Unresolved));
        }
    }
}

#[test]
fn self_crate_alias_keeps_existing_longest_module_prefix_semantics() {
    let aliases = collect_internal_aliases(&[alias(&["consumer"], "this_crate")], &index());
    assert_target(
        &reference(&["consumer"], &["this_crate", "api", "model", "User"], ImportKind::PathReference),
        &aliases,
        "src/api/model.rs",
    );
}

#[test]
fn use_via_renamed_self_crate_alias_resolves_root() {
    let aliases = collect_internal_aliases(&[alias(&["consumer"], "renamed_crate")], &index());
    let mut import = reference(&["consumer"], &["renamed_crate", "Risk"], ImportKind::Use);
    import.binding = Some("RenamedRisk".to_owned());
    assert_target(&import, &aliases, "src/lib.rs");
}

#[test]
fn nested_explicit_self_crate_binding_shadows_bare_root_module() {
    let aliases = collect_internal_aliases(&[alias(&["consumer"], "facade")], &index());
    assert_target(
        &reference(&["consumer"], &["facade", "Risk"], ImportKind::PathReference),
        &aliases,
        "src/lib.rs",
    );
}

#[test]
fn ordinary_self_path_still_means_current_module() {
    assert_target(
        &reference(&["consumer"], &["self", "Risk"], ImportKind::PathReference),
        &AliasMap::new(),
        "src/consumer.rs",
    );
}

#[test]
fn other_extern_crates_do_not_become_internal_aliases() {
    let mut declaration = reference(&["consumer"], &["api"], ImportKind::ExternCrate);
    declaration.binding = Some("other_crate".to_owned());
    let aliases = collect_internal_aliases(&[declaration], &index());
    assert!(aliases.is_empty());
}

#[test]
fn nested_alias_does_not_leak_into_sibling_module() {
    let aliases = collect_internal_aliases(&[alias(&["consumer"], "this_crate")], &index());
    assert!(matches!(
        resolve_reference(
            &reference(&["sibling"], &["this_crate", "Risk"], ImportKind::PathReference),
            &index(),
            &aliases,
        ),
        ResolutionOutcome::Unresolved
    ));
}
