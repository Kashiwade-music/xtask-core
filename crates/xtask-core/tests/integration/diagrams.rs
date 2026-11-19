//! Role: 図の対象の選択、解析の失敗、要件の無効な target が図のファイルに記録されることを検証する。
use super::*;
use std::collections::BTreeSet;

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

/// The name of a module diagram, as `target-kind-mode`; `None` for the crate diagram.
fn module_name(diagram: &Diagram) -> Option<String> {
    match &diagram.kind {
        DiagramKind::Modules {
            target, kind, mode, ..
        } => {
            let mode = match mode {
                Mode::Normal => "normal",
                Mode::Test => "test",
            };
            Some(format!("{target}-{kind}-{mode}"))
        }
        DiagramKind::Crates => None,
    }
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
        config.output = Output::Policy;
        let (report, archive) = written(&config);
        assert!(report.is_success(), "{:?}", report.errors);
        let drawn_names: BTreeSet<String> =
            archive.diagrams.iter().filter_map(module_name).collect();
        let names: BTreeSet<String> = expected.iter().map(|name| name.to_string()).collect();
        assert_eq!(drawn_names, names, "{setting}");
    }
    // A target outside the selection is still checked: its missing role is an error.
    let directory = fixture("pub fn run() {}", "[diagrams]\ntargets = 'lib'\n");
    add_bin_and_test_targets(&directory);
    fs::write(
        directory.path().join("tests/it.rs"),
        "#[test]\nfn works() {}\n",
    )
    .unwrap();
    let report = run(&Config::new(directory.path())).unwrap();
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
fn failed_analysis_is_recorded_in_the_archive() {
    for (source, policy) in [
        ("pub fn broken(", ""),
        (
            "pub fn run() {}",
            "[analysis]\nfeatures = ['fixture/missing']\n",
        ),
    ] {
        let directory = fixture(source, policy);
        let mut config = Config::new(directory.path());
        config.output = Output::Policy;
        let (report, archive) = written(&config);
        assert!(!report.is_success(), "{source}, {policy}");
        drawn(&archive.diagrams[0]);
        let modules: Vec<&Diagram> = archive
            .diagrams
            .iter()
            .filter(|diagram| matches!(diagram.kind, DiagramKind::Modules { .. }))
            .collect();
        assert!(!modules.is_empty());
        for diagram in modules {
            match &diagram.content {
                Content::Failed(reason) => {
                    assert!(!reason.is_empty(), "{}", diagram.title);
                    if policy.is_empty() {
                        assert!(reason.contains("lib.rs"), "{reason}");
                    }
                }
                other => panic!("{}: {other:?}", diagram.title),
            }
        }
    }
}

#[test]
fn skipped_targets_are_recorded_in_the_archive() {
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
    let mut config = Config::new(directory.path());
    config.output = Output::Policy;
    let (report, archive) = written(&config);
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("skipped fixture::gated")),
        "{:?}",
        report.notes
    );
    let gated = archive
        .diagrams
        .iter()
        .find(|diagram| {
            matches!(
                &diagram.kind,
                DiagramKind::Modules { target, mode: Mode::Normal, .. } if target == "gated"
            )
        })
        .expect("the gated binary has a diagram");
    assert_eq!(
        gated.content,
        Content::Skipped("required features are inactive".into())
    );
}
