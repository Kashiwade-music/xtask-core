//! Role: 図のtarget選択、古いSVGの清掃と解析失敗時の保護を検証する。
use super::*;

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
fn stale_module_diagrams_are_reported_in_no_write_and_pruned_by_default() {
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
    assert!(stale.iter().chain([&excluded]).all(|path| !path.exists()));

    // The legacy explicit flag still produces the same cleanup.
    seed_stale_diagrams(directory.path());
    fs::write(&excluded, "excluded").unwrap();
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
fn obsolete_diagrams_are_pruned_even_while_the_report_has_rule_errors() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture(
        "mod a { pub fn a() { crate::b::b(); } }\nmod b { /*! Role: aを参照する。 */ pub fn b() { crate::a::a(); } }\n",
        "",
    );
    let stale = seed_stale_diagrams(directory.path());
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", &node)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stale.iter().all(|path| !path.exists()));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .matches("pruned stale module diagram")
            .count(),
        2
    );
}

#[test]
fn failed_target_or_workspace_analysis_keeps_current_diagrams_and_prunes_removed_ones() {
    let node = available_node();
    for (source, policy) in [
        ("pub fn broken(", ""),
        (
            "pub fn run() {}",
            "[analysis]\nfeatures = ['fixture/missing']\n",
        ),
    ] {
        let directory = fixture(source, policy);
        let stale = seed_stale_diagrams(directory.path());
        seed_diagrams(
            directory.path(),
            &["fixture-lib-normal.svg", "fixture-lib-test.svg"],
        );
        let mut config = Config::new(directory.path());
        config.no_write = true;
        let report = run(&config).unwrap();
        assert!(!report.is_success(), "{source}, {policy}");
        assert_eq!(stale_notes(&report).len(), 2, "{:?}", report.notes);
        assert!(stale.iter().all(|path| path.exists()));
        for mode in ["normal", "test"] {
            assert_eq!(module_svg(directory.path(), mode), "existing");
        }
        if let Some(node) = &node {
            let output = check_cli(&directory, node);
            assert_eq!(output.status.code(), Some(1));
            assert!(stale.iter().all(|path| !path.exists()));
            for mode in ["normal", "test"] {
                assert_eq!(module_svg(directory.path(), mode), "existing");
            }
        }
    }
}

fn seed_diagrams(root: &std::path::Path, names: &[&str]) {
    let directory = root.join("architecture/modules/fixture");
    fs::create_dir_all(&directory).unwrap();
    for name in names {
        fs::write(directory.join(name), "existing").unwrap();
    }
}

#[test]
fn default_and_all_targets_never_report_their_own_diagrams_as_stale() {
    for policy in ["", "[diagrams]\ntargets = 'all'\n"] {
        let directory = fixture("pub fn run() {}", policy);
        add_bin_and_test_targets(&directory);
        seed_diagrams(
            directory.path(),
            &[
                "fixture-lib-normal.svg",
                "fixture-lib-test.svg",
                "fixture-bin-normal.svg",
                "fixture-bin-test.svg",
                "it-test-test.svg",
            ],
        );
        let mut config = Config::new(directory.path());
        config.no_write = true;
        let report = run(&config).unwrap();
        assert!(report.is_success(), "{:?}", report.errors);
        assert!(stale_notes(&report).is_empty(), "{:?}", report.notes);
    }
}

#[test]
fn targets_skipped_for_inactive_required_features_keep_their_diagrams() {
    let directory = fixture("pub fn run() {}", "[diagrams]\ntargets = 'lib-bin'\n");
    let manifest = directory.path().join("Cargo.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("extra = []\n[[bin]]\nname = 'gated'\npath = 'src/gated.rs'\nrequired-features = ['extra']\n");
    fs::write(&manifest, text).unwrap();
    fs::write(
        directory.path().join("src/gated.rs"),
        "//! Role: 任意featureの実行ファイルを検証する。\nfn main() {}\n",
    )
    .unwrap();
    seed_diagrams(
        directory.path(),
        &["gated-bin-normal.svg", "gated-bin-test.svg"],
    );
    let kept = [
        directory
            .path()
            .join("architecture/modules/fixture/gated-bin-normal.svg"),
        directory
            .path()
            .join("architecture/modules/fixture/gated-bin-test.svg"),
    ];
    let mut config = Config::new(directory.path());
    config.no_write = true;
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("skipped fixture::gated")),
        "{:?}",
        report.notes
    );
    assert!(stale_notes(&report).is_empty(), "{:?}", report.notes);

    // A skipped target outside the selection is not kept.
    fs::write(
        directory.path().join("dev/xtask/policy/workspace.toml"),
        "[diagrams]\ntargets = 'lib'\n",
    )
    .unwrap();
    let report = run(&config).unwrap();
    assert_eq!(stale_notes(&report).len(), 2, "{:?}", report.notes);
    fs::write(
        directory.path().join("dev/xtask/policy/workspace.toml"),
        "[diagrams]\ntargets = 'lib-bin'\n",
    )
    .unwrap();

    let Some(node) = available_node() else {
        return;
    };
    let output = Process::new(env!("CARGO_BIN_EXE_xtask-core"))
        .args(["check", "--prune", "--root"])
        .arg(directory.path())
        .env("XTASK_NODE", &node)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(kept.iter().all(|path| path.exists()));
}
