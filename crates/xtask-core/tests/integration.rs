use std::fs;
use std::path::Path;
use std::process::Command as Process;

use xtask_core::{Command, Config, run};

fn fixture(source: &str, policy: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::create_dir_all(directory.path().join("dev/xtask/policy")).unwrap();
    fs::write(directory.path().join("Cargo.toml"), "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n[features]\ndefault = ['left']\nleft = []\n").unwrap();
    fs::write(directory.path().join("src/lib.rs"), source).unwrap();
    fs::write(
        directory.path().join("dev/xtask/policy/workspace.toml"),
        policy,
    )
    .unwrap();
    assert!(
        Process::new(cargo())
            .args(["generate-lockfile", "--offline"])
            .current_dir(directory.path())
            .status()
            .unwrap()
            .success()
    );
    directory
}

fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

#[test]
fn read_only_check_reports_code_lines_and_cycles_in_separate_configurations() {
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { pub fn b() { crate::a::a(); } }\npub fn root() {\n    a::a();\n}\n",
        "[limits]\nmax_code_lines = 3\n",
    );
    let lock = fs::read(directory.path().join("Cargo.lock")).unwrap();
    let report = run(&Config::new(directory.path()), Command::Check).unwrap();
    assert!(!report.is_success());
    assert_eq!(
        report
            .errors
            .iter()
            .filter(|error| error.contains("reference cycle"))
            .count(),
        2
    );
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("5 code lines (limit 3)"))
    );
    assert!(report.artifacts.is_empty());
    assert!(!directory.path().join("architecture").exists());
    assert_eq!(lock, fs::read(directory.path().join("Cargo.lock")).unwrap());
}

#[test]
fn mutually_exclusive_feature_and_test_edges_do_not_form_cycles() {
    let directory = fixture(
        "mod a { pub fn a() { #[cfg(feature = \"left\")] crate::b::b(); } }\nmod b { pub fn b() { #[cfg(not(feature = \"left\"))] crate::a::a(); } }\n#[cfg(test)] mod tests { #[test] fn smoke() { crate::a::a(); } }\n",
        "[limits]\nmax_code_lines = 1000\n",
    );
    let mut config = Config::new(directory.path());
    let report = run(&config, Command::Check).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    config.default_features = false;
    let report = run(&config, Command::Check).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
}

#[test]
fn tiny_consumer_calls_the_public_entrypoint_and_reports_invalid_arguments() {
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--unknown"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}

#[test]
fn dependency_export_does_not_require_policy_or_module_expansion() {
    let directory = fixture("include!(\"not-available.rs\");", "");
    fs::remove_file(directory.path().join("dev/xtask/policy/workspace.toml")).unwrap();
    let binary = directory.path().join("missing-dot-executable");
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["deps-graph", "--root"])
        .arg(directory.path())
        .env("XTASK_GRAPHVIZ_DOT", binary)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let dot = directory.path().join("architecture/crates.dot");
    assert!(Path::new(&dot).exists());
    assert!(fs::read_to_string(dot).unwrap().contains("fixture"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Graphviz"));
}

#[test]
fn graph_export_defaults_to_architecture_with_combined_nested_module_diagrams() {
    let dot = std::env::var_os("XTASK_GRAPHVIZ_DOT").unwrap_or_else(|| "dot".into());
    if !Process::new(&dot)
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let directory = fixture(
        "mod middle { pub mod leaf { pub fn f() {} } }\npub fn f() { middle::leaf::f(); }",
        "",
    );
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["graphs", "--root"])
        .arg(directory.path())
        .env("XTASK_GRAPHVIZ_DOT", dot)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let architecture = directory.path().join("architecture");
    assert!(architecture.join("crates.svg").exists());
    for mode in ["normal", "test"] {
        let stem = architecture.join(format!("modules/fixture/fixture-lib-{mode}"));
        let dot = fs::read_to_string(stem.with_extension("dot")).unwrap();
        assert_eq!(dot.matches("subgraph").count(), 3);
        assert!(dot.contains("\"crate\" -> \"crate::middle::leaf\""));
        let svg = fs::read_to_string(stem.with_extension("svg")).unwrap();
        assert_eq!(svg.matches("class=\"cluster\"").count(), 3);
        assert!(svg.contains("class=\"edge\""));
    }
    assert_eq!(
        fs::read_dir(architecture.join("modules/fixture"))
            .unwrap()
            .count(),
        4
    );
}

#[test]
fn proc_macro_targets_include_their_builtin_cfg_in_both_modes() {
    let directory = fixture(
        "#[cfg(proc_macro)] mod a { pub fn a() { crate::b::b(); } }\n#[cfg(proc_macro)] mod b { pub fn b() { crate::a::a(); } }\n",
        "[limits]\nmax_code_lines = 1000\n",
    );
    let manifest = directory.path().join("Cargo.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[lib]\nproc-macro = true\n");
    fs::write(manifest, text).unwrap();
    let report = run(&Config::new(directory.path()), Command::Check).unwrap();
    assert_eq!(
        report
            .errors
            .iter()
            .filter(|error| error.contains("reference cycle"))
            .count(),
        2,
        "{:?}",
        report.errors
    );
}

#[test]
fn unsupported_editions_are_reported_instead_of_using_the_wrong_path_rules() {
    let directory = fixture("mod a {}", "");
    let manifest = directory.path().join("Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .unwrap()
        .replace("'2024'", "'2015'");
    fs::write(manifest, text).unwrap();
    let report = run(&Config::new(directory.path()), Command::Check).unwrap();
    assert!(!report.is_success());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("editions") && error.contains("2015"))
    );
}

#[test]
fn disabled_unit_test_targets_do_not_add_test_configuration() {
    let directory = fixture("#[cfg(test)] mod tests { include!(\"generated.rs\"); }", "");
    let manifest = directory.path().join("Cargo.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[lib]\ntest = false\n");
    fs::write(manifest, text).unwrap();
    let report = run(&Config::new(directory.path()), Command::Check).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("1 target/configuration graphs"))
    );
}
