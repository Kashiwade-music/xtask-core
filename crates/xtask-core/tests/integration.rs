//! Role: 単一check、公開API、policy設定、診断と図のファイルの結合を検証する。
use std::{fs, path::Path, process::Command as Process};
use xtask_archive::{Archive, Content, Diagram, DiagramKind, Region, Scene};
use xtask_core::{
    AnalysisSettings, Config, Mode, ModuleGraph, Output, Policy, Report, TargetAnalysis,
    TargetOutcome, analyze, run,
};

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

fn generate_lockfile(root: &Path) {
    assert!(
        Process::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["generate-lockfile", "--offline"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
}

/// Run `check --root <root> <arguments>` with the built binary.
fn check_cli(root: &Path, arguments: &[&str]) -> std::process::Output {
    Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--root"])
        .arg(root)
        .args(arguments)
        .output()
        .unwrap()
}

fn archive_path(root: &Path) -> std::path::PathBuf {
    root.join("target/xtask/architecture.xtaskmap")
}

/// The diagram file at the default path, decoded.
fn read_archive(root: &Path) -> Archive {
    let bytes = fs::read(archive_path(root)).unwrap();
    xtask_archive::decode(&bytes).unwrap()
}

/// Run the check with its configured output, and decode the diagram file it wrote.
fn written(config: &Config) -> (Report, Archive) {
    let report = run(config).unwrap();
    let path = report.archive.clone().expect("the archive was written");
    let archive = xtask_archive::decode(&fs::read(path).unwrap()).unwrap();
    (report, archive)
}

/// The module graph of every analyzed target and configuration.
fn graphs(analysis: &[TargetAnalysis]) -> Vec<&ModuleGraph> {
    analysis
        .iter()
        .flat_map(|target| &target.outcomes)
        .filter_map(|(_, outcome)| match outcome {
            TargetOutcome::Analyzed(graph) => Some(graph),
            _ => None,
        })
        .collect()
}

fn drawn(diagram: &Diagram) -> &Scene {
    match &diagram.content {
        Content::Drawn(scene) => scene,
        other => panic!("{}: {other:?}", diagram.title),
    }
}

/// The module diagram of the fixture's library target, in one configuration.
fn module_diagram(archive: &Archive, mode: Mode) -> &Diagram {
    archive
        .diagrams
        .iter()
        .find(|diagram| {
            matches!(
                &diagram.kind,
                DiagramKind::Modules { package, target, mode: found, .. }
                    if package == "fixture" && target == "fixture" && *found == mode
            )
        })
        .expect("module diagram of the fixture library")
}

fn item_role<'a>(scene: &'a Scene, name: &str) -> Option<&'a str> {
    scene
        .items
        .iter()
        .find(|item| item.name == name)
        .and_then(|item| item.role.as_deref())
}

fn has_item(scene: &Scene, name: &str) -> bool {
    scene.items.iter().any(|item| item.name == name)
}

fn node_name(scene: &Scene, node: u32) -> &str {
    &scene.items[scene.nodes[node as usize].item as usize].name
}

fn has_edge(scene: &Scene, from: &str, to: &str) -> bool {
    scene
        .edges
        .iter()
        .any(|edge| node_name(scene, edge.from) == from && node_name(scene, edge.to) == to)
}

/// The number of enclosing regions of `region`.
fn region_depth(scene: &Scene, region: &Region) -> usize {
    let mut depth = 0;
    let mut parent = region.parent;
    while let Some(index) = parent {
        depth += 1;
        parent = scene.regions[index as usize].parent;
    }
    depth
}

#[test]
fn check_writes_nothing_by_default() {
    let directory = fixture("pub fn run() {}", "");
    let output = check_cli(directory.path(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.path().join("target/xtask").exists());
}

#[test]
fn write_creates_archive() {
    let directory = fixture("pub fn run() {}", "");
    let output = check_cli(directory.path(), &["--write"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let archive = read_archive(directory.path());
    assert_eq!(archive.diagrams[0].kind, DiagramKind::Crates);
}

#[test]
fn old_flags_are_rejected() {
    for option in ["--no-write", "--prune"] {
        let output = check_cli(Path::new("."), &[option]);
        assert_eq!(output.status.code(), Some(2), "{option}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
    }
}

#[test]
fn root_must_be_the_workspace_root() {
    let directory = fixture("pub fn run() {}", "");
    let output = check_cli(&directory.path().join("src"), &[]);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    let name = directory.path().file_name().unwrap().to_string_lossy();
    assert!(
        errors.contains("--root must name the workspace root") && errors.contains(name.as_ref()),
        "{errors}"
    );
}

#[test]
fn missing_policy_uses_default_in_run_and_analyze() {
    let directory = fixture("pub fn run() {}", "");
    fs::remove_file(directory.path().join("dev/xtask/policy/workspace.toml")).unwrap();
    let config = Config::new(directory.path());
    let (_, report) = analyze(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
}

#[test]
fn successful_check_alone_generates_crate_and_nested_module_diagrams() {
    let directory = fixture(
        "mod middle { /*! Role: leafへの公開窓口を提供する。 */ pub mod leaf { /*! Role: 呼び出し対象を提供する。 */ pub fn f() {} } }\npub fn f() { middle::leaf::f(); }",
        "",
    );
    let lock = fs::read(directory.path().join("Cargo.lock")).unwrap();
    let output = check_cli(directory.path(), &["--write"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let archive = read_archive(directory.path());
    assert_eq!(archive.diagrams.len(), 3);
    assert_eq!(archive.diagrams[0].kind, DiagramKind::Crates);
    assert_eq!(
        item_role(drawn(&archive.diagrams[0]), "fixture"),
        Some("CLIの検査と図出力を検証する。")
    );
    for mode in [Mode::Normal, Mode::Test] {
        let scene = drawn(module_diagram(&archive, mode));
        assert_eq!(
            item_role(scene, "crate::middle"),
            Some("leafへの公開窓口を提供する。")
        );
        assert_eq!(
            item_role(scene, "crate::middle::leaf"),
            Some("呼び出し対象を提供する。")
        );
        assert_eq!(scene.regions.len(), 3);
        assert!(
            scene
                .regions
                .iter()
                .any(|region| region_depth(scene, region) == 2)
        );
        assert!(has_edge(scene, "crate", "crate::middle::leaf"));
    }
    assert_eq!(lock, fs::read(directory.path().join("Cargo.lock")).unwrap());
}

#[test]
fn failed_check_keeps_line_cycle_and_role_diagnostics_and_generates_diagrams() {
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aへの参照を持つ循環の検査対象。 */ pub fn b() { crate::a::a(); } }\npub fn root() {\n    a::a();\n}\n",
        "[limits]\nmax_code_lines = 3\n",
    );
    let output = check_cli(directory.path(), &["--write"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    assert_eq!(errors.matches("reference cycle").count(), 2, "{errors}");
    assert!(errors.contains("5 code lines (limit 3)"), "{errors}");
    assert!(
        errors.contains("crate::a") && errors.contains("missing module role"),
        "{errors}"
    );
    let archive = read_archive(directory.path());
    drawn(&archive.diagrams[0]);
    for mode in [Mode::Normal, Mode::Test] {
        let scene = drawn(module_diagram(&archive, mode));
        assert!(has_item(scene, "crate::a") && has_item(scene, "crate::b"));
    }
    // An invalid crate-root role must also preserve the drawable dependency DAG.
    let root = directory.path().join("src/lib.rs");
    let source = fs::read_to_string(&root).unwrap();
    fs::write(root, source.split_once('\n').unwrap().1).unwrap();
    let output = check_cli(directory.path(), &["--write"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("(crate)"));
    let archive = read_archive(directory.path());
    drawn(&archive.diagrams[0]);
}

#[test]
fn visibility_errors_do_not_prevent_the_dependency_diagram() {
    let directory = fixture(
        "pub fn run() { provider::run(); }",
        "[visible_to]\nprovider = []\n",
    );
    add_provider(&directory);
    let output = check_cli(directory.path(), &["--write"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("fixture")
            && errors.contains("provider")
            && errors.contains("visible only"),
        "{errors}"
    );
    let archive = read_archive(directory.path());
    assert!(has_edge(drawn(&archive.diagrams[0]), "fixture", "provider"));
    drawn(module_diagram(&archive, Mode::Normal));
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
    let output = check_cli(directory.path(), &["--write"]);
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
    let archive = read_archive(directory.path());
    let crates = drawn(&archive.diagrams[0]);
    assert!(has_item(crates, "fixture") && has_item(crates, "provider"));
    let module = module_diagram(&archive, Mode::Normal);
    assert!(matches!(module.content, Content::Failed(_)));
}

#[test]
fn archive_write_failure_keeps_check_diagnostics() {
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aを参照する。 */ pub fn b() { crate::a::a(); } }\npub fn root() {\na::a();\n}\n",
        "[limits]\nmax_code_lines = 3\n",
    );
    // A directory where the diagram file must go makes the replacement fail.
    fs::create_dir_all(archive_path(directory.path())).unwrap();
    let output = check_cli(directory.path(), &["--write"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    for expected in [
        "reference cycle",
        "5 code lines (limit 3)",
        "missing module role",
        "cannot write",
        "architecture.xtaskmap",
    ] {
        assert!(errors.contains(expected), "{expected}: {errors}");
    }
}

#[test]
fn custom_target_roots_are_counted() {
    let directory = fixture("pub fn run() {}", "[limits]\nmax_code_lines = 3\n");
    let manifest = directory.path().join("Cargo.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str(
        "off = []\n[[bin]]\nname = 'tool'\npath = 'tools/custom.rs'\nrequired-features = ['off']\n",
    );
    fs::write(&manifest, text).unwrap();
    fs::create_dir_all(directory.path().join("tools")).unwrap();
    let custom = directory.path().join("tools/custom.rs");
    fs::write(
        &custom,
        "fn one() {}\nfn two() {}\nfn three() {}\nfn main() {}\n",
    )
    .unwrap();
    let output = check_cli(directory.path(), &[]);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("custom.rs") && errors.contains("4 code lines (limit 3)"),
        "{errors}"
    );
    fs::write(
        &custom,
        "fn one() {}\nfn two() {}\nfn three() {}\nfn main() {}\nfn bad() { let = ; }\n",
    )
    .unwrap();
    let output = check_cli(directory.path(), &[]);
    assert_eq!(output.status.code(), Some(1));
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("custom.rs") && errors.contains("code lines (limit 3)"),
        "{errors}"
    );
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
        vec!["check", "--no-write"],
        vec!["check", "--prune"],
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
    let (analysis, report) = analyze(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    let found = graphs(&analysis);
    assert_eq!(found.len(), 2);
    assert!(
        found[0]
            .modules
            .iter()
            .any(|module| module.id == "crate::selected")
    );
    assert!(
        found[0]
            .edges
            .iter()
            .any(|edge| edge.from == "crate::b" && edge.to == "crate::a")
    );
    assert!(!directory.path().join("target/xtask").exists());
    config.analysis = Some(AnalysisSettings::default());
    let (analysis, overridden) = analyze(&config).unwrap();
    assert!(
        overridden
            .errors
            .iter()
            .any(|error| error.contains("preserve"))
    );
    let found = graphs(&analysis);
    assert!(found.iter().all(|graph| {
        graph
            .modules
            .iter()
            .all(|module| module.id != "crate::selected")
    }));
    assert!(
        found[0]
            .edges
            .iter()
            .any(|edge| edge.from == "crate::a" && edge.to == "crate::b")
    );
    config.analysis = Some(
        Policy::from_toml("[analysis]\ntrusted_attributes = ['preserve']\n")
            .unwrap()
            .analysis()
            .clone(),
    );
    let (_, overridden) = analyze(&config).unwrap();
    assert!(overridden.is_success(), "{:?}", overridden.errors);
    config.output = Output::Policy;
    let (report, archive) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert_eq!(archive.diagrams.len(), 3);
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
    let (_, default) = analyze(&config).unwrap();
    for id in ["(crate)", "crate::external", "crate::inline"] {
        assert!(
            default
                .errors
                .iter()
                .any(|error| error.contains(id) && error.contains("80")),
            "{id}: {:?}",
            default.errors
        );
    }
    fs::write(&config.policy, "[limits]\nmax_role_chars = 160\n").unwrap();
    let (analysis, expanded) = analyze(&config).unwrap();
    assert!(expanded.is_success(), "{:?}", expanded.errors);
    for graph in graphs(&analysis) {
        for (id, role) in [
            ("crate", "根".repeat(90)),
            ("crate::external", "外".repeat(90)),
            ("crate::inline", "内".repeat(90)),
        ] {
            assert!(
                graph
                    .modules
                    .iter()
                    .any(|module| module.id == id && module.role.as_deref() == Ok(role.as_str()))
            );
        }
    }
    config.output = Output::Policy;
    let (report, archive) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert_eq!(
        item_role(drawn(&archive.diagrams[0]), "fixture"),
        Some("根".repeat(90).as_str())
    );
    for mode in [Mode::Normal, Mode::Test] {
        let scene = drawn(module_diagram(&archive, mode));
        for role in ["根".repeat(90), "外".repeat(90), "内".repeat(90)] {
            assert!(
                scene
                    .items
                    .iter()
                    .any(|item| item.role.as_deref() == Some(role.as_str())),
                "missing role {role}"
            );
        }
    }
    fs::write(&config.policy, "[limits]\nmax_role_chars = 80\n").unwrap();
    let (_, limited) = analyze(&config).unwrap();
    assert!(!limited.is_success());
}

#[test]
fn analysis_is_output_free_and_policy_optional_but_check_requires_policy() {
    let directory = long_role_fixture("");
    let config = Config::new(directory.path());
    fs::remove_file(&config.policy).unwrap();
    let (_, report) = analyze(&config).unwrap();
    assert!(!report.is_success());
    assert!(!directory.path().join("target/xtask").exists());
    // Without a policy file, check runs with the default policy as well.
    let report = run(&config).expect("check runs with the default policy");
    assert!(!report.is_success(), "the default role limit applies");
    assert!(report.archive.is_none());
    assert!(!directory.path().join("target/xtask").exists());
    fs::write(&config.policy, "[limits]\nmax_role_chars = 160\n").unwrap();
    let (_, report) = analyze(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(!directory.path().join("target/xtask").exists());
    for value in ["0", "-1", "'invalid'"] {
        fs::write(
            &config.policy,
            format!("[limits]\nmax_role_chars = {value}\n"),
        )
        .unwrap();
        assert!(run(&config).is_err(), "{value}");
        assert!(analyze(&config).is_err(), "{value}");
    }
}

#[test]
fn active_modules_require_their_own_roles_and_cfg_excluded_modules_do_not() {
    let directory = fixture("mod active {}\n#[cfg(any())] mod excluded {}", "");
    let config = Config::new(directory.path());
    let (_, report) = analyze(&config).unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("crate::active") && error.contains("missing module role"))
    );
    assert!(
        report
            .errors
            .iter()
            .all(|error| !error.contains("crate::excluded"))
    );
    fs::write(
        directory.path().join("src/lib.rs"),
        "mod documented {\n//! Role: 呼び出し対象を提供する。\npub fn run() {}\n}",
    )
    .unwrap();
    let (_, report) = analyze(&config).unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("(crate)") && error.contains("missing module role"))
    );
    assert!(
        report
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
    let (_, report) = analyze(&Config::new(directory.path())).unwrap();
    assert_eq!(
        report
            .errors
            .iter()
            .filter(|error| error.contains("reference cycle"))
            .count(),
        2
    );
    fs::write(&manifest, format!("{original}\n[lib]\ntest = false\n")).unwrap();
    fs::write(directory.path().join("src/lib.rs"), "//! Role: 通常構成の解析を検証する。\n#[cfg(test)] mod tests { /*! Role: テスト構成で生成ソースを読み込む。 */ include!(\"generated.rs\"); }").unwrap();
    let (analysis, report) = analyze(&Config::new(directory.path())).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert_eq!(analysis.len(), 1);
    assert!(
        analysis[0]
            .outcomes
            .iter()
            .all(|(mode, _)| *mode == Mode::Normal)
    );
}

#[test]
fn unsupported_editions_are_reported_instead_of_using_wrong_path_rules() {
    let directory = fixture("", "");
    let manifest = directory.path().join("Cargo.toml");
    let source = fs::read_to_string(&manifest)
        .unwrap()
        .replace("'2024'", "'2015'");
    fs::write(manifest, source).unwrap();
    let (_, report) = analyze(&Config::new(directory.path())).unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("editions") && error.contains("2015"))
    );
}

#[test]
fn policy_output_paths_and_api_overrides_resolve_from_the_repository_root() {
    let directory = fixture(
        "pub fn run() {}",
        "[diagrams]\noutput = 'docs/architecture diagrams.xtaskmap'\n",
    );
    let mut config = Config::new(directory.path());
    assert_eq!(config.output, Output::None);
    config.output = Output::Policy;
    let (report, _) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        directory
            .path()
            .join("docs/architecture diagrams.xtaskmap")
            .exists()
    );
    assert!(!directory.path().join("target/xtask").exists());
    let absolute = directory.path().join("absolute policy diagrams.xtaskmap");
    let value = absolute
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"");
    fs::write(
        &config.policy,
        format!("[diagrams]\noutput = \"{value}\"\n"),
    )
    .unwrap();
    let (report, _) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(absolute.exists());
    config.output = Output::Path("api relative diagrams.xtaskmap".into());
    let (report, _) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        directory
            .path()
            .join("api relative diagrams.xtaskmap")
            .exists()
    );
    let override_absolute = directory.path().join("api absolute diagrams.xtaskmap");
    config.output = Output::Path(override_absolute.clone());
    let (report, _) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(override_absolute.exists());
}

fn immutable_snapshot(
    root: &Path,
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
            "[diagrams]\noutput = 'never created/output.xtaskmap'\n[limits]\nmax_code_lines = 3\n"
                .to_string();
        if violation {
            policy.push_str("[visible_to]\nprovider = []\n");
        }
        let directory = fixture(source, &policy);
        if violation {
            add_provider(&directory);
        }
        let mut files = vec![
            "src/lib.rs",
            "Cargo.toml",
            "Cargo.lock",
            "dev/xtask/policy/workspace.toml",
        ];
        if violation {
            files.extend(["provider/Cargo.toml", "provider/src/lib.rs"]);
        }
        let before = immutable_snapshot(directory.path(), &files);
        let report = run(&Config::new(directory.path())).unwrap();
        assert_eq!(report.is_success(), !violation, "{:?}", report.errors);
        assert!(report.archive.is_none());
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
        let output = check_cli(directory.path(), &[]);
        assert_eq!(
            output.status.code(),
            Some(if violation { 1 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Node"));
        assert_eq!(before, immutable_snapshot(directory.path(), &files));
        assert!(!directory.path().join("never created").exists());
        assert!(!directory.path().join("target/xtask").exists());
    }
}

#[path = "integration/diagrams.rs"]
mod diagrams;
