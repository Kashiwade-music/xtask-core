//! Role: 公開APIとCLIを通した検査、設定切替、SVG出力を検証する。
use std::fs;
use std::process::Command as Process;

use xtask_core::{Command, Config, analyze, run};

fn fixture(source: &str, policy: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::create_dir_all(directory.path().join("dev/xtask/policy")).unwrap();
    fs::write(directory.path().join("Cargo.toml"), "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n[features]\ndefault = ['left']\nleft = []\n").unwrap();
    fs::write(
        directory.path().join("src/lib.rs"),
        format!("//! Role: CLIの検査と図出力を検証する。\n{source}"),
    )
    .unwrap();
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
        "mod a { /*! Role: bへの参照を持つ循環の検査対象。 */ pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aへの参照を持つ循環の検査対象。 */ pub fn b() { crate::a::a(); } }\npub fn root() {\n    a::a();\n}\n",
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
        "mod a { /*! Role: left選択時にbを参照する。 */ pub fn a() { #[cfg(feature = \"left\")] crate::b::b(); } }\nmod b { /*! Role: left未選択時にaを参照する。 */ pub fn b() { #[cfg(not(feature = \"left\"))] crate::a::a(); } }\n#[cfg(test)] mod tests { /*! Role: テスト構成でaの関数を検証する。 */ #[test] fn smoke() { crate::a::a(); } }\n",
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
fn roles_are_mandatory_for_active_modules_and_crate_roots() {
    let directory = fixture("mod active {}\n#[cfg(any())] mod excluded {}", "");
    let config = Config::new(directory.path());
    let report = run(&config, Command::Check).unwrap();
    assert!(!report.is_success());
    assert!(
        report.errors.iter().any(|error| {
            error.contains("crate::active") && error.contains("missing module role")
        })
    );
    assert!(
        !report
            .errors
            .iter()
            .any(|error| error.contains("crate::excluded"))
    );
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--root"])
        .arg(directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    fs::write(
        directory.path().join("src/lib.rs"),
        "mod documented {\n//! Role: 呼び出し対象を提供する。\npub fn run() {}\n}",
    )
    .unwrap();
    let report = run(&config, Command::Check).unwrap();
    assert!(!report.is_success());
    assert!(
        report
            .errors
            .iter()
            .any(|error| { error.contains("(crate)") && error.contains("missing module role") })
    );
    assert!(
        !report
            .errors
            .iter()
            .any(|error| error.contains("crate::documented"))
    );
    assert!(!directory.path().join("architecture").exists());
}

#[test]
fn dependency_export_does_not_require_policy_or_module_expansion() {
    let directory = fixture("include!(\"not-available.rs\");", "");
    fs::remove_file(directory.path().join("dev/xtask/policy/workspace.toml")).unwrap();
    let binary = directory.path().join("missing-node-executable");
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["deps-graph", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", binary)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!directory.path().join("architecture/crates.svg").exists());
    assert!(!directory.path().join("architecture/crates.dot").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Node"));
}

#[test]
fn graph_export_defaults_to_architecture_with_combined_nested_module_diagrams() {
    let node = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !Process::new(&node)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let directory = fixture(
        "mod middle { /*! Role: leafへの公開窓口を提供する。 */ pub mod leaf { /*! Role: 呼び出し対象の関数を提供する。 */ pub fn f() {} } }\npub fn f() { middle::leaf::f(); }",
        "",
    );
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["graphs", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", node)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let architecture = directory.path().join("architecture");
    assert!(architecture.join("crates.svg").exists());
    assert!(!architecture.join("crates.dot").exists());
    assert!(
        fs::read_to_string(architecture.join("crates.svg"))
            .unwrap()
            .contains("CLIの検査と図出力を検証する。")
    );
    for mode in ["normal", "test"] {
        let stem = architecture.join(format!("modules/fixture/fixture-lib-{mode}"));
        assert!(!stem.with_extension("dot").exists());
        let svg = fs::read_to_string(stem.with_extension("svg")).unwrap();
        assert!(svg.contains("leafへの公開窓口を提供する。"));
        assert!(svg.contains("呼び出し対象の関数を提供する。"));
        let document = roxmltree::Document::parse(&svg).unwrap();
        assert_eq!(
            document.root_element().attribute("data-layout-engine"),
            Some("ELK")
        );
        let clusters: Vec<_> = document
            .descendants()
            .filter(|node| node.attribute("class") == Some("cluster"))
            .collect();
        assert_eq!(clusters.len(), 3);
        assert!(clusters.iter().any(|node| {
            node.ancestors()
                .skip(1)
                .filter(|parent| parent.attribute("class") == Some("cluster"))
                .count()
                == 2
        }));
        assert!(
            document
                .descendants()
                .any(|node| node.attribute("class") == Some("edge")
                    && node.attribute("data-from") == Some("crate")
                    && node.attribute("data-to") == Some("crate::middle::leaf"))
        );
    }
    assert_eq!(
        fs::read_dir(architecture.join("modules/fixture"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn proc_macro_targets_include_their_builtin_cfg_in_both_modes() {
    let directory = fixture(
        "#[cfg(proc_macro)] mod a { /*! Role: proc_macro構成でbを参照する。 */ pub fn a() { crate::b::b(); } }\n#[cfg(proc_macro)] mod b { /*! Role: proc_macro構成でaを参照する。 */ pub fn b() { crate::a::a(); } }\n",
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
    let directory = fixture("mod a { /*! Role: edition検査の対象を提供する。 */ }", "");
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
    let directory = fixture(
        "#[cfg(test)] mod tests { /*! Role: テスト構成で生成ソースを読み込む。 */ include!(\"generated.rs\"); }",
        "",
    );
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

fn long_role_fixture(policy: &str) -> tempfile::TempDir {
    let directory = fixture("", policy);
    // Replace the fixture's ordinary root role, rather than adding a duplicate.
    fs::write(
        directory.path().join("src/lib.rs"),
        format!(
            "//! Role: {}\nmod external;\nmod inline {{\n//! Role: {}\npub fn run() {{}}\n}}\n",
            "根".repeat(90),
            "内".repeat(90),
        ),
    )
    .unwrap();
    fs::write(
        directory.path().join("src/external.rs"),
        format!("//! Role: {}\npub fn run() {{}}\n", "外".repeat(90)),
    )
    .unwrap();
    directory
}

fn available_node() -> Option<std::ffi::OsString> {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    Process::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
        .then_some(binary)
}

#[test]
fn policy_role_limit_reaches_checks_analysis_and_combined_graph_exports() {
    let directory = long_role_fixture("");
    let mut config = Config::new(directory.path());
    config.tests = false;
    let report = run(&config, Command::Check).unwrap();
    for module in ["(crate)", "crate::external", "crate::inline"] {
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains(module) && error.contains("80")),
            "{module}: {:?}",
            report.errors
        );
    }
    fs::write(&config.policy, "[limits]\nmax_role_chars = 160\n").unwrap();
    let report = run(&config, Command::Check).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    let analysis = analyze(&config).unwrap();
    assert!(analysis.report.is_success(), "{:?}", analysis.report.errors);
    assert_eq!(analysis.targets.len(), 1);
    for (id, description) in [
        ("crate", "根".repeat(90)),
        ("crate::external", "外".repeat(90)),
        ("crate::inline", "内".repeat(90)),
    ] {
        assert!(analysis.targets[0].graph.modules.iter().any(|module| {
            module.id == id && module.role.as_deref() == Some(description.as_str())
        }));
    }
    let Some(node) = available_node() else {
        return;
    };
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["graphs", "--root"])
        .arg(directory.path())
        .arg("--no-tests")
        .env("XTASK_NODE", node)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let architecture = directory.path().join("architecture");
    let crates = fs::read_to_string(architecture.join("crates.svg")).unwrap();
    assert!(crates.contains(&"根".repeat(90)));
    let modules =
        fs::read_to_string(architecture.join("modules/fixture/fixture-lib-normal.svg")).unwrap();
    for role in ["根".repeat(90), "外".repeat(90), "内".repeat(90)] {
        assert!(modules.contains(&role), "module graph lost role: {role}");
    }
}

#[test]
fn explicit_api_role_limit_overrides_policy_in_checks_and_analysis() {
    let directory = long_role_fixture("[limits]\nmax_role_chars = 160\n");
    let mut config = Config::new(directory.path());
    config.tests = false;
    config.max_role_chars = Some(80);
    let report = run(&config, Command::Check).unwrap();
    assert!(!report.is_success());
    assert!(report.errors.iter().any(|error| error.contains("80")));
    let analysis = analyze(&config).unwrap();
    assert!(!analysis.report.is_success());
    fs::write(&config.policy, "[limits]\nmax_role_chars = 80\n").unwrap();
    config.max_role_chars = Some(160);
    let report = run(&config, Command::Check).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    let analysis = analyze(&config).unwrap();
    assert!(analysis.report.is_success(), "{:?}", analysis.report.errors);
}

#[test]
fn cli_role_limit_can_narrow_and_expand_the_policy_limit() {
    let directory = long_role_fixture("[limits]\nmax_role_chars = 160\n");
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--root"])
        .arg(directory.path())
        .args(["--no-tests", "--max-role-chars", "80"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("80"));
    fs::write(
        directory.path().join("dev/xtask/policy/workspace.toml"),
        "[limits]\nmax_role_chars = 80\n",
    )
    .unwrap();
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--root"])
        .arg(directory.path())
        .args(["--no-tests", "--max-role-chars", "160"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn absent_policy_keeps_analysis_and_graph_defaults_and_allows_overrides() {
    let directory = long_role_fixture("");
    let mut config = Config::new(directory.path());
    config.tests = false;
    fs::remove_file(&config.policy).unwrap();
    let analysis = analyze(&config).unwrap();
    assert!(!analysis.report.is_success());
    assert!(
        analysis
            .report
            .errors
            .iter()
            .any(|error| error.contains("80"))
    );
    config.max_role_chars = Some(160);
    let analysis = analyze(&config).unwrap();
    assert!(analysis.report.is_success(), "{:?}", analysis.report.errors);
    // Checks still require a policy even when the role limit is explicit.
    assert!(run(&config, Command::Check).is_err());
    let Some(node) = available_node() else {
        return;
    };
    for command in ["deps-graph", "modules-graph"] {
        let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
            .args([command, "--root"])
            .arg(directory.path())
            .arg("--no-tests")
            .env("XTASK_NODE", &node)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{command}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("80"),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
            .args([command, "--root"])
            .arg(directory.path())
            .args(["--no-tests", "--max-role-chars", "160"])
            .env("XTASK_NODE", &node)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn invalid_role_limits_are_rejected_at_cli_policy_and_api_boundaries() {
    for value in ["0", "-1", "invalid"] {
        let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
            .args(["check", "--max-role-chars", value])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{value}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("max-role-chars"));
    }
    let directory = long_role_fixture("");
    let mut config = Config::new(directory.path());
    config.tests = false;
    for value in ["0", "-1", "'invalid'"] {
        fs::write(
            &config.policy,
            format!("[limits]\nmax_role_chars = {value}\n"),
        )
        .unwrap();
        assert!(run(&config, Command::Check).is_err(), "{value}");
        assert!(analyze(&config).is_err(), "{value}");
    }
    fs::write(&config.policy, "").unwrap();
    config.max_role_chars = Some(0);
    assert!(run(&config, Command::Check).is_err());
    assert!(analyze(&config).is_err());
}
