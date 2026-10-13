//! Role: 単一check、公開API、policy設定、診断とSVG出力の結合を検証する。
use std::{collections::BTreeSet, fs, process::Command as Process};
use xtask_core::{Config, analyze, run};
use xtask_workspace::AnalysisSettings;

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
    generate_lockfile(directory.path());
    directory
}

fn generate_lockfile(root: &std::path::Path) {
    assert!(
        Process::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["generate-lockfile", "--offline"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
}

fn available_node() -> Option<std::ffi::OsString> {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    Process::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
        .then_some(binary)
}

fn check_cli(directory: &tempfile::TempDir, node: &std::ffi::OsStr) -> std::process::Output {
    Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", node)
        .output()
        .unwrap()
}

fn module_svg(root: &std::path::Path, mode: &str) -> String {
    fs::read_to_string(root.join(format!(
        "architecture/modules/fixture/fixture-lib-{mode}.svg"
    )))
    .unwrap()
}

#[test]
fn successful_check_alone_generates_crate_and_nested_module_diagrams() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture(
        "mod middle { /*! Role: leafへの公開窓口を提供する。 */ pub mod leaf { /*! Role: 呼び出し対象を提供する。 */ pub fn f() {} } }\npub fn f() { middle::leaf::f(); }",
        "",
    );
    let lock = fs::read(directory.path().join("Cargo.lock")).unwrap();
    let output = check_cli(&directory, &node);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let architecture = directory.path().join("architecture");
    let crates = fs::read_to_string(architecture.join("crates.svg")).unwrap();
    assert!(crates.contains("CLIの検査と図出力を検証する。"));
    assert!(!architecture.join("crates.dot").exists());
    for mode in ["normal", "test"] {
        let svg = module_svg(directory.path(), mode);
        assert!(svg.contains("leafへの公開窓口を提供する。"));
        assert!(svg.contains("呼び出し対象を提供する。"));
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
    assert_eq!(lock, fs::read(directory.path().join("Cargo.lock")).unwrap());
}

#[test]
fn failed_check_keeps_line_cycle_and_role_diagnostics_and_generates_diagrams() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aへの参照を持つ循環の検査対象。 */ pub fn b() { crate::a::a(); } }\npub fn root() {\n    a::a();\n}\n",
        "[limits]\nmax_code_lines = 3\n",
    );
    let output = check_cli(&directory, &node);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    assert_eq!(errors.matches("reference cycle").count(), 2, "{errors}");
    assert!(errors.contains("5 code lines (limit 3)"), "{errors}");
    assert!(
        errors.contains("crate::a") && errors.contains("missing module role"),
        "{errors}"
    );
    assert!(directory.path().join("architecture/crates.svg").exists());
    for mode in ["normal", "test"] {
        let svg = module_svg(directory.path(), mode);
        assert!(svg.contains("crate::a"));
        assert!(svg.contains("crate::b"));
    }
    // An invalid crate-root role must also preserve the drawable dependency DAG.
    let root = directory.path().join("src/lib.rs");
    let source = fs::read_to_string(&root).unwrap();
    fs::write(root, source.split_once('\n').unwrap().1).unwrap();
    fs::remove_file(directory.path().join("architecture/crates.svg")).unwrap();
    for mode in ["normal", "test"] {
        fs::remove_file(directory.path().join(format!(
            "architecture/modules/fixture/fixture-lib-{mode}.svg"
        )))
        .unwrap();
    }
    let output = check_cli(&directory, &node);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("(crate)"));
    assert!(directory.path().join("architecture/crates.svg").exists());
    assert!(!module_svg(directory.path(), "normal").is_empty());
}

#[test]
fn visibility_errors_do_not_prevent_the_dependency_diagram() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture(
        "pub fn run() { provider::run(); }",
        "[visible_to]\nprovider = []\n",
    );
    add_provider(&directory);
    let output = check_cli(&directory, &node);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("fixture")
            && errors.contains("provider")
            && errors.contains("visible only"),
        "{errors}"
    );
    let svg = fs::read_to_string(directory.path().join("architecture/crates.svg")).unwrap();
    let document = roxmltree::Document::parse(&svg).unwrap();
    assert!(
        document
            .descendants()
            .any(|node| node.attribute("class") == Some("edge")
                && node.attribute("data-from") == Some("fixture")
                && node.attribute("data-to") == Some("provider"))
    );
    assert!(!module_svg(directory.path(), "normal").is_empty());
}

fn add_provider(directory: &tempfile::TempDir) {
    let manifest = directory.path().join("Cargo.toml");
    let mut source = fs::read_to_string(&manifest)
        .unwrap()
        .replace("[workspace]", "[workspace]\nmembers = ['provider']");
    source.push_str("\n[dependencies]\nprovider = { path = 'provider' }\n");
    fs::write(manifest, source).unwrap();
    fs::create_dir_all(directory.path().join("provider/src")).unwrap();
    fs::write(
        directory.path().join("provider/Cargo.toml"),
        "[package]\nname = 'provider'\nversion = '0.1.0'\nedition = '2024'\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("provider/src/lib.rs"),
        "//! Role: 他crateへの公開関数を提供する。\npub fn run() {}\n",
    )
    .unwrap();
    generate_lockfile(directory.path());
}

#[test]
fn early_analysis_and_file_errors_do_not_hide_other_checks_or_the_crate_diagram() {
    let directory = fixture(
        "pub fn run() { provider::run(); }",
        "[limits]\nmax_code_lines = 3\n[analysis]\ntarget = 'nonexistent-xtask-target'\n[visible_to]\nprovider = []\n",
    );
    add_provider(&directory);
    // Both files are deliberately undeclared: conventional source discovery
    // must check them even though module analysis cannot reach either one.
    fs::write(directory.path().join("src/a.rs"), "pub fn bad( {\n").unwrap();
    fs::write(
        directory.path().join("src/z.rs"),
        "pub fn one() {}\npub fn two() {}\npub fn three() {}\npub fn four() {}\n",
    )
    .unwrap();
    let node = available_node();
    let binary = node.clone().unwrap_or_else(|| {
        directory
            .path()
            .join("missing-node-executable")
            .into_os_string()
    });
    let output = check_cli(&directory, &binary);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    for expected in [
        "module analysis",
        "nonexistent-xtask-target",
        "a.rs",
        "cannot tokenize Rust source",
        "z.rs",
        "4 code lines (limit 3)",
        "visible only",
    ] {
        assert!(errors.contains(expected), "{expected}: {errors}");
    }
    if node.is_some() {
        let svg = fs::read_to_string(directory.path().join("architecture/crates.svg")).unwrap();
        assert!(svg.contains("fixture"));
        assert!(svg.contains("provider"));
    }
}

#[test]
fn diagram_runtime_failure_does_not_discard_check_diagnostics() {
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aを参照する。 */ pub fn b() { crate::a::a(); } }\npub fn root() {\na::a();\n}\n",
        "[limits]\nmax_code_lines = 3\n",
    );
    let output = check_cli(
        &directory,
        directory.path().join("missing-node-executable").as_os_str(),
    );
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    for expected in [
        "reference cycle",
        "5 code lines (limit 3)",
        "missing module role",
        "Node",
    ] {
        assert!(errors.contains(expected), "{expected}: {errors}");
    }
    assert!(!directory.path().join("architecture/crates.svg").exists());
}

#[test]
fn help_succeeds_and_old_commands_and_flags_are_rejected() {
    for arguments in [vec!["--help"], vec!["-h"], vec!["check", "--help"]] {
        let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
            .args(&arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("check"));
    }
    for args in [
        vec!["help"],
        vec!["deps-graph"],
        vec!["modules-graph"],
        vec!["graphs"],
        vec!["check", "--features", "left"],
        vec!["check", "--cfg", "custom"],
        vec!["check", "--max-role-chars", "160"],
        vec!["check", "--no-tests"],
        vec!["check", "--policy", "policy.toml"],
        vec!["check", "--output", "diagrams"],
        vec!["check", "--target", "target"],
        vec!["check", "--all-features"],
        vec!["check", "--no-default-features"],
        vec!["check", "--trust-attribute", "preserve"],
        vec!["check", "--trust-derive", "Custom"],
        vec!["check", "--root"],
    ] {
        let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
    }
}

#[test]
fn policy_analysis_is_used_and_api_analysis_replaces_all_of_it() {
    let directory = fixture(
        "#[preserve] struct S;\nmod a { /*! Role: left構成でbを参照する。 */ pub fn a() { #[cfg(feature=\"left\")] crate::b::b(); } }\nmod b { /*! Role: left未選択時にaを参照する。 */ pub fn b() { #[cfg(not(feature=\"left\"))] crate::a::a(); } }\n#[cfg(custom)] mod selected { /*! Role: policyで選択される。 */ }",
        "[analysis]\ndefault_features = false\ncfg = ['custom']\ntrusted_attributes = ['preserve']\n",
    );
    let mut config = Config::new(directory.path());
    let policy = analyze(&config).unwrap();
    assert!(policy.report.is_success(), "{:?}", policy.report.errors);
    assert_eq!(policy.targets.len(), 2);
    assert!(
        policy.targets[0]
            .graph
            .modules
            .iter()
            .any(|module| module.id == "crate::selected")
    );
    assert!(
        policy.targets[0]
            .graph
            .edges
            .iter()
            .any(|edge| edge.from == "crate::b" && edge.to == "crate::a")
    );
    assert!(!directory.path().join("architecture").exists());
    config.analysis = Some(AnalysisSettings::default());
    let overridden = analyze(&config).unwrap();
    assert!(
        overridden
            .report
            .errors
            .iter()
            .any(|error| error.contains("preserve"))
    );
    assert!(overridden.targets.iter().all(|target| {
        target
            .graph
            .modules
            .iter()
            .all(|module| module.id != "crate::selected")
    }));
    assert!(
        overridden.targets[0]
            .graph
            .edges
            .iter()
            .any(|edge| edge.from == "crate::a" && edge.to == "crate::b")
    );
    config.analysis = Some(AnalysisSettings {
        trusted_attributes: BTreeSet::from(["preserve".into()]),
        ..Default::default()
    });
    let overridden = analyze(&config).unwrap();
    assert!(
        overridden.report.is_success(),
        "{:?}",
        overridden.report.errors
    );
    if available_node().is_some() {
        let report = run(&config).unwrap();
        assert!(report.is_success(), "{:?}", report.errors);
        assert_eq!(report.artifacts.len(), 3);
    }
}

fn long_role_fixture(policy: &str) -> tempfile::TempDir {
    let directory = fixture("", policy);
    // Replace the fixture's root Role rather than adding a second declaration.
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
        format!("//! Role: {}\npub fn run() {{}}\n", "外".repeat(90),),
    )
    .unwrap();
    directory
}

#[test]
fn role_limits_reach_analysis_and_all_check_diagrams_with_api_precedence() {
    let directory = long_role_fixture("");
    let mut config = Config::new(directory.path());
    let default = analyze(&config).unwrap();
    for id in ["(crate)", "crate::external", "crate::inline"] {
        assert!(
            default
                .report
                .errors
                .iter()
                .any(|error| error.contains(id) && error.contains("80")),
            "{id}: {:?}",
            default.report.errors
        );
    }
    fs::write(&config.policy, "[limits]\nmax_role_chars = 160\n").unwrap();
    let expanded = analyze(&config).unwrap();
    assert!(expanded.report.is_success(), "{:?}", expanded.report.errors);
    for target in &expanded.targets {
        for (id, role) in [
            ("crate", "根".repeat(90)),
            ("crate::external", "外".repeat(90)),
            ("crate::inline", "内".repeat(90)),
        ] {
            assert!(
                target
                    .graph
                    .modules
                    .iter()
                    .any(|module| module.id == id && module.role.as_deref() == Some(role.as_str()))
            );
        }
    }
    config.max_role_chars = Some(80);
    assert!(!analyze(&config).unwrap().report.is_success());
    if available_node().is_some() {
        config.max_role_chars = None;
        let report = run(&config).unwrap();
        assert!(report.is_success(), "{:?}", report.errors);
        assert_eq!(report.artifacts.len(), 3);
        let crates = fs::read_to_string(directory.path().join("architecture/crates.svg")).unwrap();
        assert!(crates.contains(&"根".repeat(90)));
        for mode in ["normal", "test"] {
            let svg = module_svg(directory.path(), mode);
            for role in ["根".repeat(90), "外".repeat(90), "内".repeat(90)] {
                assert!(svg.contains(&role), "missing role {role}");
            }
        }
    }
    fs::write(&config.policy, "[limits]\nmax_role_chars = 80\n").unwrap();
    config.max_role_chars = Some(160);
    assert!(analyze(&config).unwrap().report.is_success());
    if available_node().is_some() {
        let report = run(&config).unwrap();
        assert!(report.is_success(), "{:?}", report.errors);
    }
}

#[test]
fn analysis_is_output_free_and_policy_optional_but_check_requires_policy() {
    let directory = long_role_fixture("");
    let mut config = Config::new(directory.path());
    fs::remove_file(&config.policy).unwrap();
    assert!(!analyze(&config).unwrap().report.is_success());
    config.max_role_chars = Some(160);
    assert!(analyze(&config).unwrap().report.is_success());
    assert!(!directory.path().join("architecture").exists());
    assert!(run(&config).is_err());
    for value in ["0", "-1", "'invalid'"] {
        fs::write(
            &config.policy,
            format!("[limits]\nmax_role_chars = {value}\n"),
        )
        .unwrap();
        assert!(run(&config).is_err(), "{value}");
        assert!(analyze(&config).is_err(), "{value}");
    }
    fs::write(&config.policy, "").unwrap();
    config.max_role_chars = Some(0);
    assert!(run(&config).is_err());
    assert!(analyze(&config).is_err());
}

#[test]
fn active_modules_require_their_own_roles_and_cfg_excluded_modules_do_not() {
    let directory = fixture("mod active {}\n#[cfg(any())] mod excluded {}", "");
    let config = Config::new(directory.path());
    let analysis = analyze(&config).unwrap();
    assert!(
        analysis
            .report
            .errors
            .iter()
            .any(|error| error.contains("crate::active") && error.contains("missing module role"))
    );
    assert!(
        analysis
            .report
            .errors
            .iter()
            .all(|error| !error.contains("crate::excluded"))
    );
    fs::write(
        directory.path().join("src/lib.rs"),
        "mod documented {\n//! Role: 呼び出し対象を提供する。\npub fn run() {}\n}",
    )
    .unwrap();
    let analysis = analyze(&config).unwrap();
    assert!(
        analysis
            .report
            .errors
            .iter()
            .any(|error| error.contains("(crate)") && error.contains("missing module role"))
    );
    assert!(
        analysis
            .report
            .errors
            .iter()
            .all(|error| !error.contains("crate::documented"))
    );
}

#[test]
fn proc_macro_cfg_is_present_in_both_modes_and_cargo_test_false_is_respected() {
    let directory = fixture(
        "#[cfg(proc_macro)] mod a { /*! Role: bを参照する。 */ pub fn a() { crate::b::b(); } }\n#[cfg(proc_macro)] mod b { /*! Role: aを参照する。 */ pub fn b() { crate::a::a(); } }",
        "",
    );
    let manifest = directory.path().join("Cargo.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{original}\n[lib]\nproc-macro = true\n")).unwrap();
    let analysis = analyze(&Config::new(directory.path())).unwrap();
    assert_eq!(
        analysis
            .report
            .errors
            .iter()
            .filter(|error| error.contains("reference cycle"))
            .count(),
        2
    );
    fs::write(&manifest, format!("{original}\n[lib]\ntest = false\n")).unwrap();
    fs::write(directory.path().join("src/lib.rs"), "//! Role: 通常構成の解析を検証する。\n#[cfg(test)] mod tests { /*! Role: テスト構成で生成ソースを読み込む。 */ include!(\"generated.rs\"); }").unwrap();
    let analysis = analyze(&Config::new(directory.path())).unwrap();
    assert!(analysis.report.is_success(), "{:?}", analysis.report.errors);
    assert_eq!(analysis.targets.len(), 1);
    assert!(!analysis.targets[0].test);
}

#[test]
fn unsupported_editions_are_reported_instead_of_using_wrong_path_rules() {
    let directory = fixture("", "");
    let manifest = directory.path().join("Cargo.toml");
    let source = fs::read_to_string(&manifest)
        .unwrap()
        .replace("'2024'", "'2015'");
    fs::write(manifest, source).unwrap();
    let analysis = analyze(&Config::new(directory.path())).unwrap();
    assert!(
        analysis
            .report
            .errors
            .iter()
            .any(|error| error.contains("editions") && error.contains("2015"))
    );
}

#[test]
fn policy_output_paths_and_api_overrides_resolve_from_the_repository_root() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture("pub fn run() {}", "output = 'docs/architecture diagrams'\n");
    let mut config = Config::new(directory.path());
    assert_eq!(config.output, None);
    let output = check_cli(&directory, &node);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let relative = directory.path().join("docs/architecture diagrams");
    for file in [
        "crates.svg",
        "modules/fixture/fixture-lib-normal.svg",
        "modules/fixture/fixture-lib-test.svg",
    ] {
        assert!(relative.join(file).exists(), "{file}");
    }
    assert!(!directory.path().join("architecture").exists());
    let absolute = directory.path().join("absolute policy diagrams");
    let value = absolute
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"");
    fs::write(&config.policy, format!("output = \"{value}\"\n")).unwrap();
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert_eq!(report.artifacts.len(), 3);
    assert!(
        report
            .artifacts
            .iter()
            .all(|path| path.starts_with(&absolute))
    );
    config.output = Some("api relative diagrams".into());
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        report
            .artifacts
            .iter()
            .all(|path| path.starts_with(directory.path().join("api relative diagrams")))
    );
    let override_absolute = directory.path().join("api absolute diagrams");
    config.output = Some(override_absolute.clone());
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        report
            .artifacts
            .iter()
            .all(|path| path.starts_with(&override_absolute))
    );
}

fn immutable_snapshot(
    root: &std::path::Path,
    files: &[&str],
) -> Vec<(std::path::PathBuf, Vec<u8>, std::time::SystemTime)> {
    files
        .iter()
        .map(|name| {
            let path = root.join(name);
            let bytes = fs::read(&path).unwrap();
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            (path, bytes, modified)
        })
        .collect()
}

#[test]
fn no_write_runs_every_rule_without_node_or_filesystem_mutation() {
    for violation in [false, true] {
        let source = if violation {
            "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aを参照する。 */ pub fn b() { crate::a::a(); } }\npub fn root() {\na::a(); provider::run();\n}\n"
        } else {
            "pub fn run() {}"
        };
        let mut policy =
            "output = 'never created/output dir'\n[limits]\nmax_code_lines = 3\n".to_string();
        if violation {
            policy.push_str("[visible_to]\nprovider = []\n");
        }
        let directory = fixture(source, &policy);
        if violation {
            add_provider(&directory);
        }
        fs::create_dir_all(directory.path().join("architecture/modules/fixture")).unwrap();
        fs::write(
            directory.path().join("architecture/crates.svg"),
            "existing crate SVG sentinel",
        )
        .unwrap();
        fs::write(
            directory
                .path()
                .join("architecture/modules/fixture/fixture-lib-normal.svg"),
            "existing module SVG sentinel",
        )
        .unwrap();
        let mut files = vec![
            "src/lib.rs",
            "Cargo.toml",
            "Cargo.lock",
            "dev/xtask/policy/workspace.toml",
            "architecture/crates.svg",
            "architecture/modules/fixture/fixture-lib-normal.svg",
        ];
        if violation {
            files.extend(["provider/Cargo.toml", "provider/src/lib.rs"]);
        }
        let before = immutable_snapshot(directory.path(), &files);
        let mut config = Config::new(directory.path());
        config.no_write = true;
        let report = run(&config).unwrap();
        assert_eq!(report.is_success(), !violation, "{:?}", report.errors);
        assert!(report.artifacts.is_empty());
        if violation {
            for expected in [
                "reference cycle",
                "missing module role",
                "5 code lines (limit 3)",
                "visible only",
            ] {
                assert!(
                    report.errors.iter().any(|error| error.contains(expected)),
                    "{expected}: {:?}",
                    report.errors
                );
            }
        }
        let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
            .args(["check", "--root"])
            .arg(directory.path())
            .arg("--no-write")
            .env(
                "XTASK_NODE",
                directory
                    .path()
                    .join("nonexistent-node-must-not-be-launched"),
            )
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(if violation { 1 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Node"));
        assert_eq!(before, immutable_snapshot(directory.path(), &files));
        assert!(!directory.path().join("never created").exists());
        assert!(
            !directory
                .path()
                .join("architecture/modules/fixture/fixture-lib-test.svg")
                .exists()
        );
        assert_eq!(
            fs::read_dir(directory.path().join("architecture/modules/fixture"))
                .unwrap()
                .count(),
            1
        );
    }
}

fn add_bin_and_test_targets(directory: &tempfile::TempDir) {
    fs::write(
        directory.path().join("src/main.rs"),
        "//! Role: 実行ファイルの図を検証する。\nfn main() {}\n",
    )
    .unwrap();
    fs::create_dir_all(directory.path().join("tests")).unwrap();
    fs::write(
        directory.path().join("tests/it.rs"),
        "//! Role: 結合試験の図を検証する。\n#[test]\nfn works() {}\n",
    )
    .unwrap();
}

fn seed_stale_diagrams(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let modules = root.join("architecture/modules");
    fs::create_dir_all(modules.join("fixture")).unwrap();
    fs::create_dir_all(modules.join("removed")).unwrap();
    let stale = vec![
        modules.join("fixture/gone-lib-normal.svg"),
        modules.join("removed/only-lib-normal.svg"),
    ];
    for path in &stale {
        fs::write(path, "stale").unwrap();
    }
    fs::write(modules.join("fixture/notes.txt"), "not an svg").unwrap();
    stale
}

fn stale_notes(report: &xtask_core::Report) -> Vec<&String> {
    report
        .notes
        .iter()
        .filter(|note| note.contains("stale module diagram"))
        .collect()
}

fn diagram_names(root: &std::path::Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let modules = root.join("architecture/modules");
    for package in fs::read_dir(modules).unwrap() {
        let package = package.unwrap().path();
        for file in fs::read_dir(&package).unwrap() {
            names.insert(format!(
                "{}/{}",
                package.file_name().unwrap().to_string_lossy(),
                file.unwrap().file_name().to_string_lossy()
            ));
        }
    }
    names
}

#[test]
fn diagram_targets_select_generated_diagrams_but_not_checked_targets() {
    let lib = ["fixture-lib-normal", "fixture-lib-test"];
    let bin = ["fixture-bin-normal", "fixture-bin-test"];
    let test = ["it-test-test"];
    for (setting, expected) in [
        ("", [&lib[..], &bin[..], &test[..]].concat()),
        (
            "[diagrams]\ntargets = 'all'\n",
            [&lib[..], &bin[..], &test[..]].concat(),
        ),
        (
            "[diagrams]\ntargets = 'lib-bin'\n",
            [&lib[..], &bin[..]].concat(),
        ),
        ("[diagrams]\ntargets = 'lib'\n", lib.to_vec()),
    ] {
        let directory = fixture("pub fn run() {}", setting);
        add_bin_and_test_targets(&directory);
        let mut config = Config::new(directory.path());
        config.no_write = true;
        let report = run(&config).unwrap();
        assert!(report.is_success(), "{:?}", report.errors);
        let validated: BTreeSet<_> = report
            .notes
            .iter()
            .filter_map(|note| note.strip_prefix("validated "))
            .filter_map(|note| note.strip_suffix(" (no-write)"))
            .map(|path| {
                std::path::Path::new(path)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name != "crates")
            .collect();
        let names: BTreeSet<_> = expected.iter().map(|name| name.to_string()).collect();
        assert_eq!(validated, names, "{setting}");
    }
    // A target outside the selection is still checked: its missing role is an error.
    let directory = fixture("pub fn run() {}", "[diagrams]\ntargets = 'lib'\n");
    add_bin_and_test_targets(&directory);
    fs::write(
        directory.path().join("tests/it.rs"),
        "#[test]\nfn works() {}\n",
    )
    .unwrap();
    let mut config = Config::new(directory.path());
    config.no_write = true;
    let report = run(&config).unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("fixture::it")),
        "{:?}",
        report.errors
    );
}

#[test]
fn stale_module_diagrams_are_reported_without_deleting_and_pruned_on_request() {
    let directory = fixture("pub fn run() {}", "[diagrams]\ntargets = 'lib'\n");
    add_bin_and_test_targets(&directory);
    let stale = seed_stale_diagrams(directory.path());
    let excluded = directory
        .path()
        .join("architecture/modules/fixture/fixture-bin-normal.svg");
    fs::write(&excluded, "excluded").unwrap();

    // no_write reports the same set and never deletes, even if prune is requested via the API.
    let mut config = Config::new(directory.path());
    config.no_write = true;
    let report = run(&config).unwrap();
    assert_eq!(stale_notes(&report).len(), 3, "{:?}", report.notes);
    assert!(stale.iter().chain([&excluded]).all(|path| path.exists()));
    config.prune = true;
    assert!(run(&config).is_err());

    let Some(node) = available_node() else {
        return;
    };
    let output = check_cli(&directory, &node);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.matches("stale module diagram").count(),
        3,
        "{stdout}"
    );
    assert!(stale.iter().chain([&excluded]).all(|path| path.exists()));
    assert_eq!(
        fs::read_to_string(&excluded).unwrap(),
        "excluded",
        "an excluded diagram is neither regenerated nor deleted"
    );

    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--prune", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", &node)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        diagram_names(directory.path()),
        BTreeSet::from([
            "fixture/fixture-lib-normal.svg".to_string(),
            "fixture/fixture-lib-test.svg".to_string(),
            "fixture/notes.txt".to_string(),
        ])
    );
    assert!(
        !directory
            .path()
            .join("architecture/modules/removed")
            .exists()
    );
    assert!(directory.path().join("architecture/crates.svg").exists());
}

#[test]
fn prune_is_skipped_while_the_report_has_errors() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aを参照する。 */ pub fn b() { crate::a::a(); } }\n",
        "",
    );
    let stale = seed_stale_diagrams(directory.path());
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--prune", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", &node)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stale.iter().all(|path| path.exists()));
    assert!(String::from_utf8_lossy(&output.stdout).contains("stale module diagram prune skipped"));
}
