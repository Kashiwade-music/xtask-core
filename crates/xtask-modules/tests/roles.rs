//! Role: Verify module role declarations and their scope.

mod common;

use common::{Options, write_fixture};
use std::{collections::BTreeSet, fs};
use xtask_modules::ModuleGraph;

// Role fixtures are written verbatim: annotation injection would mask failures.
fn graph(files: &[(&str, &str)], options: Options) -> ModuleGraph {
    let directory = write_fixture(files);
    options.analyze(&directory.path().join("lib.rs")).unwrap()
}

fn root(source: &str) -> ModuleGraph {
    graph(&[("lib.rs", source)], Options::default())
}

fn root_file(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = write_fixture(&[("lib.rs", source)]);
    let path = directory.path().join("lib.rs");
    (directory, path)
}

#[test]
fn missing_role_is_a_diagnostic_and_keeps_the_module() {
    let graph = root("pub fn run() {}");
    assert_eq!(graph.modules.len(), 1);
    assert_eq!(graph.modules[0].role.as_deref().ok(), None);
    assert_eq!(graph.diagnostics.len(), 1);
    assert!(graph.diagnostics[0].contains("missing module role"));
}

#[test]
fn root_external_inline_and_path_modules_keep_their_own_roles() {
    let graph = graph(
        &[
            (
                "lib.rs",
                "//! Role: Coordinate example modules.\nmod external;\n#[path=\"other/renamed.rs\"] mod path_module;\nmod inline {\n//! Role: Hold inline behavior.\nmod nested {\n//! Role: Hold nested behavior.\npub fn run() {}\n}\n}",
            ),
            (
                "external.rs",
                "//! Role: Provide external behavior.\npub fn run() {}",
            ),
            (
                "other/renamed.rs",
                "//! Role: Provide renamed behavior.\npub fn run() {}",
            ),
        ],
        Options::default(),
    );
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    let roles: BTreeSet<_> = graph
        .modules
        .iter()
        .map(|module| (module.id.as_str(), module.role.as_deref().ok()))
        .collect();
    assert_eq!(
        roles,
        BTreeSet::from([
            ("crate", Some("Coordinate example modules.")),
            ("crate::external", Some("Provide external behavior.")),
            ("crate::inline", Some("Hold inline behavior.")),
            ("crate::inline::nested", Some("Hold nested behavior.")),
            ("crate::path_module", Some("Provide renamed behavior.")),
        ])
    );
}

#[test]
fn duplicate_role_declarations_fail_even_when_identical() {
    for source in [
        "//! Role: Coordinate modules.\n//! Role: Coordinate modules.\nfn run() {}",
        "//! Role: Coordinate modules.\n//! Role: Another responsibility.\nfn run() {}",
    ] {
        let graph = root(source);
        assert_eq!(graph.modules[0].role.as_deref().ok(), None);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|message| message.contains("duplicate")),
            "{:?}",
            graph.diagnostics
        );
    }
}

#[test]
fn empty_or_malformed_role_descriptions_are_errors() {
    for source in [
        "//! Role: \nfn run() {}",
        "//! Role:\nfn run() {}",
        "//! Role:     \nfn run() {}",
    ] {
        let graph = root(source);
        assert_eq!(graph.modules[0].role.as_deref().ok(), None);
        assert!(!graph.diagnostics.is_empty(), "{source:?}");
    }
}

#[test]
fn unicode_role_length_counts_characters_and_rejects_excessive_length() {
    let short = format!("//! Role: {}\nfn run() {{}}", "責".repeat(80));
    let graph = root(&short);
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    assert_eq!(
        graph.modules[0].role.as_deref().ok(),
        Some("責".repeat(80).as_str())
    );
    let long = format!("//! Role: {}\nfn run() {{}}", "責".repeat(81));
    let graph = root(&long);
    assert_eq!(graph.modules[0].role.as_deref().ok(), None);
    assert!(!graph.diagnostics.is_empty());
}

#[test]
fn configured_limit_applies_to_root_external_and_inline_modules() {
    let files = [
        (
            "lib.rs",
            "//! Role: 責任\nmod external; mod inline {\n//! Role: 設計\n}",
        ),
        ("external.rs", "//! Role: 参照\n"),
    ];
    let allowed = graph(
        &files,
        Options {
            max_role_chars: 2,
            ..Default::default()
        },
    );
    assert!(allowed.diagnostics.is_empty(), "{:?}", allowed.diagnostics);
    assert_eq!(allowed.modules.len(), 3);
    assert!(allowed.modules.iter().all(|module| module.role.is_ok()));
    let rejected = graph(
        &files,
        Options {
            max_role_chars: 1,
            ..Default::default()
        },
    );
    assert_eq!(rejected.modules.len(), 3);
    assert!(rejected.modules.iter().all(|module| module.role.is_err()));
    assert_eq!(rejected.diagnostics.len(), 3);
    assert!(
        rejected
            .diagnostics
            .iter()
            .all(|message| message.contains("at most 1 Unicode characters"))
    );
}

#[test]
fn configured_limit_can_exceed_eighty_and_counts_unicode_scalars() {
    let long_role = "責".repeat(90);
    let long_source = format!("//! Role: {long_role}\nmod inline {{\n//! Role: {long_role}\n}}");
    let allowed = graph(
        &[("lib.rs", &long_source)],
        Options {
            max_role_chars: 90,
            ..Default::default()
        },
    );
    assert!(allowed.diagnostics.is_empty(), "{:?}", allowed.diagnostics);
    assert!(
        allowed
            .modules
            .iter()
            .all(|module| module.role.as_deref().ok() == Some(long_role.as_str()))
    );
    let rejected = graph(
        &[("lib.rs", &long_source)],
        Options {
            max_role_chars: 89,
            ..Default::default()
        },
    );
    assert!(rejected.modules.iter().all(|module| module.role.is_err()));
    assert!(
        rejected
            .diagnostics
            .iter()
            .all(|message| message.contains("at most 89 Unicode characters"))
    );

    // This is three Unicode scalar values, two graphemes, and more than three bytes.
    let scalars = "責e\u{301}";
    let source = format!("//! Role: {scalars}\n");
    let allowed = graph(
        &[("lib.rs", &source)],
        Options {
            max_role_chars: 3,
            ..Default::default()
        },
    );
    assert_eq!(allowed.modules[0].role.as_deref().ok(), Some(scalars));
    assert!(allowed.diagnostics.is_empty());
    let rejected = graph(
        &[("lib.rs", &source)],
        Options {
            max_role_chars: 2,
            ..Default::default()
        },
    );
    assert_eq!(rejected.modules[0].role.as_deref().ok(), None);
    assert!(rejected.diagnostics[0].contains("at most 2 Unicode characters"));
}

#[test]
fn root_role_limit_applies_to_the_root_module_alone() {
    let (_directory, path) = root_file("//! Role: 責任\n");
    let role = |limit: usize| {
        Options {
            max_role_chars: limit,
            ..Default::default()
        }
        .analyze(&path)
        .unwrap()
    };
    assert_eq!(role(2).modules[0].role.as_deref().ok(), Some("責任"));
    let rejected = role(1);
    assert_eq!(rejected.modules[0].role.as_deref().ok(), None);
    assert!(rejected.diagnostics[0].contains("at most 1 Unicode characters"));

    let long_role = "責".repeat(81);
    fs::write(&path, format!("//! Role: {long_role}\nfn run() {{}}")).unwrap();
    let default = Options::default().analyze(&path).unwrap();
    assert_eq!(default.modules[0].role.as_deref().ok(), None);
    assert!(default.diagnostics[0].contains("at most 80 Unicode characters"));
    assert_eq!(
        role(81).modules[0].role.as_deref().ok(),
        Some(long_role.as_str())
    );
}

#[test]
fn zero_limit_is_a_configuration_error() {
    let (_directory, path) = root_file("//! Role: Valid role\n");
    let error = Options {
        max_role_chars: 0,
        ..Default::default()
    }
    .analyze(&path)
    .unwrap_err();
    assert!(error.contains("(crate): max_role_chars must be greater than 0"));
    assert!(error.contains("lib.rs"));
}

#[test]
fn control_characters_and_multiline_static_doc_values_are_rejected() {
    for source in [
        "#![doc = \"Role: Has\\tcontrol\"]\nfn run() {}",
        "#![doc = \"Role: Has\\u{7f}control\"]\nfn run() {}",
        "#![doc = \"Role: First line\\nsecond line\"]\nfn run() {}",
    ] {
        let graph = root(source);
        assert_eq!(graph.modules[0].role.as_deref().ok(), None);
        assert!(!graph.diagnostics.is_empty(), "{source:?}");
    }
}

#[test]
fn outer_docs_comments_and_string_contents_cannot_supply_a_role() {
    for source in [
        "/// Role: Describe the following function.\nfn run() {}",
        "// Role: An ordinary comment.\nfn run() {}",
        "/* Role: An ordinary block comment. */\nfn run() {}",
        "const TEXT: &str = \"//! Role: Merely string content.\";",
        "#[doc = \"Role: Describe the following function.\"]\nfn run() {}",
    ] {
        let graph = root(source);
        assert_eq!(graph.modules[0].role.as_deref().ok(), None);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|message| message.contains("missing module role")),
            "{source:?}: {:?}",
            graph.diagnostics
        );
    }
}

#[test]
fn parent_or_declaration_docs_do_not_fill_a_childs_missing_role() {
    let graph = graph(
        &[
            (
                "lib.rs",
                "//! Role: Coordinate modules.\n/// Role: Declaration docs.\nmod child;\nmod inline {}",
            ),
            ("child.rs", "pub fn run() {}"),
        ],
        Options::default(),
    );
    assert_eq!(graph.diagnostics.len(), 2, "{:?}", graph.diagnostics);
    for id in ["crate::child", "crate::inline"] {
        let module = graph.modules.iter().find(|module| module.id == id).unwrap();
        assert_eq!(module.role.as_deref().ok(), None);
        assert!(graph.diagnostics.iter().any(|message| message.contains(id)));
    }
}

#[test]
fn cfg_excluded_modules_do_not_require_roles() {
    let graph = graph(
        &[
            (
                "lib.rs",
                "//! Role: Coordinate selected modules.\n#[cfg(off)] mod missing_file;\n#[cfg(off)] mod inline {}\nmod disabled_file;",
            ),
            ("disabled_file.rs", "#![cfg(off)]\npub fn run() {}"),
        ],
        Options::default(),
    );
    assert_eq!(graph.modules.len(), 1);
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    let graph = root("#![cfg(off)]\nfn run() {}");
    assert!(graph.modules.is_empty());
    assert!(graph.diagnostics.is_empty());
}

#[test]
fn roles_supplied_only_by_cfg_attr_do_not_replace_original_declarations() {
    let graph = graph(
        &[(
            "lib.rs",
            "#![cfg_attr(on, doc = \"Role: Generated documentation.\")]\nfn run() {}",
        )],
        Options {
            cfg: BTreeSet::from(["on".into()]),
            ..Default::default()
        },
    );
    assert_eq!(graph.modules[0].role.as_deref().ok(), None);
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|message| message.contains("missing module role")),
        "{:?}",
        graph.diagnostics
    );
}

#[test]
fn static_inner_doc_attributes_and_block_docs_have_the_same_semantics() {
    for source in [
        "#![doc = \"Role: Coordinate modules.\"]\nfn run() {}",
        "/*! Role: Coordinate modules. */\nfn run() {}",
        "//! Role: Coordinate modules.\r\nfn run() {}\r\n",
    ] {
        let graph = root(source);
        assert_eq!(
            graph.modules[0].role.as_deref().ok(),
            Some("Coordinate modules.")
        );
        assert!(
            graph.diagnostics.is_empty(),
            "{source:?}: {:?}",
            graph.diagnostics
        );
    }
    let graph = root(
        "//! General module documentation.\n//! Role: Coordinate modules.\n//! More details.\nfn run() {}",
    );
    assert!(graph.modules[0].role.is_err());
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|message| message.contains("Role must be the first line of the first inner doc")),
        "{:?}",
        graph.diagnostics
    );
}

#[test]
fn role_must_be_first() {
    let graph = root("//! Doc.\n//! Role: x\nfn run() {}");
    assert!(graph.modules[0].role.is_err());
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|message| message.contains("Role must be the first line of the first inner doc")),
        "{:?}",
        graph.diagnostics
    );
}

#[test]
fn missing_roles_preserve_reference_edges_and_cycle_detection() {
    let graph = graph(
        &[
            (
                "lib.rs",
                "//! Role: Coordinate mutually dependent modules.\nmod a; mod b;",
            ),
            ("a.rs", "pub fn run() { crate::b::run(); }"),
            ("b.rs", "pub fn run() { crate::a::run(); }"),
        ],
        Options::default(),
    );
    assert_eq!(graph.modules.len(), 3);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.cycles, vec![vec!["crate::a", "crate::b"]]);
    assert_eq!(graph.diagnostics.len(), 2, "{:?}", graph.diagnostics);
}
