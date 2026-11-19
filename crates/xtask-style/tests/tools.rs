//! Role: 外部道具（Clippy、Default probe、rustfmt）を実際に起動し、公開の検査の結果を確かめる。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use xtask_policy::{AnalysisSettings, Policy, StyleRule, StyleSettings};
use xtask_style::{StyleInput, StyleReport, check as check_style};
use xtask_workspace::Workspace;

/// A generated Cargo project, loaded as a workspace for the checks.
struct Project {
    root: PathBuf,
    workspace: Workspace,
}

impl Project {
    fn check(
        &self,
        analysis: &AnalysisSettings,
        style: &StyleSettings,
        files: &[PathBuf],
    ) -> StyleReport {
        let features = self.workspace.features(analysis);
        let modules = BTreeMap::new();
        check_style(&StyleInput {
            root: &self.root,
            workspace: &self.workspace,
            analysis,
            features: &features,
            style,
            files,
            modules: &modules,
        })
    }
}

fn load(directory: &Path) -> Project {
    let root = xtask_source::normalize(directory).unwrap();
    let workspace = Workspace::load(&root).unwrap();
    Project { root, workspace }
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn fixture(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("src")).unwrap();
    std::fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='tool-fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("src/lib.rs"), source).unwrap();
    let output = cargo()
        .args(["generate-lockfile", "--offline"])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    directory
}

/// Style settings with the basic preset, plus the given `[style]` lines.
fn style(lines: &str) -> StyleSettings {
    Policy::from_toml(&format!("[style]\nenabled=true\n{lines}\n"))
        .unwrap()
        .style()
        .unwrap()
        .clone()
}

fn analysis(lines: &str) -> AnalysisSettings {
    Policy::from_toml(&format!("[analysis]\n{lines}\n"))
        .unwrap()
        .analysis()
        .clone()
}

/// Exactly the given rules: the basic preset is disabled except for them, and
/// the extra lines are appended to the `[style]` table.
fn only_with(rules: &[StyleRule], lines: &str) -> StyleSettings {
    let basic = StyleRule::BASIC;
    let extra: Vec<String> = rules
        .iter()
        .filter(|rule| !basic.contains(rule))
        .map(|rule| format!("'{rule}'"))
        .collect();
    let disabled: Vec<String> = basic
        .iter()
        .filter(|rule| !rules.contains(rule))
        .map(|rule| format!("'{rule}'"))
        .collect();
    style(&format!(
        "extra_rules=[{}]\ndisabled_rules=[{}]\n{lines}",
        extra.join(","),
        disabled.join(",")
    ))
}

fn only(rules: &[StyleRule]) -> StyleSettings {
    only_with(rules, "")
}

#[test]
fn real_clippy_distinguishes_standard_aliases_and_same_named_methods() {
    let fixture = fixture(
        "type Maybe = Option<u8>;\nstruct Own;\nimpl Own { fn unwrap(self) -> u8 { 1 } }\npub fn f(value: Maybe) -> u8 { let _ = Own.unwrap(); value.unwrap() }\n",
    );
    let project = load(fixture.path());
    let report = project.check(
        &AnalysisSettings::default(),
        &only(&[StyleRule::Unwrap]),
        &[],
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(report.diagnostics[0].rule, StyleRule::Unwrap);
}

#[test]
fn changed_clippy_configuration_rechecks_cached_sources() {
    let fixture = fixture(
        "pub fn f() { let _ = std::panic::catch_unwind(|| ()); let _ = std::sync::PoisonError::new(1).into_inner(); }\n",
    );
    let project = load(fixture.path());
    let settings = AnalysisSettings::default();
    let first = project.check(&settings, &only(&[StyleRule::CatchUnwind]), &[]);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(first.diagnostics.len(), 1, "{:?}", first.diagnostics);
    assert_eq!(first.diagnostics[0].rule, StyleRule::CatchUnwind);
    let second = project.check(&settings, &only(&[StyleRule::PoisonAccess]), &[]);
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    assert_eq!(second.diagnostics.len(), 1, "{:?}", second.diagnostics);
    assert_eq!(second.diagnostics[0].rule, StyleRule::PoisonAccess);
}

#[test]
fn default_probe_distinguishes_missing_impl_and_invalid_selector_and_preserves_lock() {
    let fixture =
        fixture("pub struct Missing;\n#[derive(Default)] pub struct Good;\nstruct Private;\n");
    let project = load(fixture.path());
    let settings = AnalysisSettings::default();
    let before = std::fs::read(fixture.path().join("Cargo.lock")).unwrap();
    let probe = |selector: &str| {
        only_with(
            &[StyleRule::OptionsDefault],
            &format!("options_structs=['{selector}']"),
        )
    };
    let missing = project.check(&settings, &probe("tool-fixture::Missing"), &[]);
    assert!(missing.errors.is_empty(), "{:?}", missing.errors);
    assert_eq!(missing.diagnostics.len(), 1, "{:?}", missing.diagnostics);
    let good = project.check(&settings, &probe("tool-fixture::Good"), &[]);
    assert!(good.errors.is_empty(), "{:?}", good.errors);
    assert!(good.diagnostics.is_empty(), "{:?}", good.diagnostics);
    let private = project.check(&settings, &probe("tool-fixture::Private"), &[]);
    assert!(!private.errors.is_empty());
    assert!(private.diagnostics.is_empty());
    assert_eq!(
        before,
        std::fs::read(fixture.path().join("Cargo.lock")).unwrap()
    );
}

#[test]
fn cross_package_feature_requests_are_accepted_for_each_package() {
    let fixture = fixture("pub fn f() {}\n");
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers=['left','right']\nresolver='2'\n",
    )
    .unwrap();
    for name in ["left", "right"] {
        std::fs::create_dir_all(fixture.path().join(name).join("src")).unwrap();
        std::fs::write(fixture.path().join(name).join("Cargo.toml"), format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n[features]\nspecial=[]\n")).unwrap();
        std::fs::write(
            fixture.path().join(name).join("src/lib.rs"),
            "#[cfg(feature=\"special\")] pub fn f(value: Option<u8>) -> u8 { value.unwrap() }\n",
        )
        .unwrap();
    }
    let output = cargo()
        .args(["generate-lockfile", "--offline"])
        .current_dir(fixture.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let project = load(fixture.path());
    let report = project.check(
        &analysis("features=['right/special']"),
        &only(&[StyleRule::Unwrap]),
        &[],
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(!report.diagnostics.is_empty());
}

#[test]
fn configured_cfg_activates_compiler_checked_code() {
    let fixture =
        fixture("#[cfg(xtask_style_cfg)] pub fn f(value: Option<u8>) -> u8 { value.unwrap() }\n");
    let project = load(fixture.path());
    let report = project.check(
        &analysis("cfg=['xtask_style_cfg']"),
        &only(&[StyleRule::Unwrap]),
        &[],
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(report.diagnostics[0].rule, StyleRule::Unwrap);
}

#[test]
fn format_uses_project_width_and_rejects_widths_over_the_limit() {
    let fixture = fixture(
        "pub fn function(\n    alpha: u32,\n    beta: u32,\n    gamma: u32,\n    delta: u32,\n    epsilon: u32,\n) -> u32 {\n    alpha + beta + gamma + delta + epsilon\n}\n",
    );
    let project = load(fixture.path());
    let files = [project.root.join("src/lib.rs")];
    let policy = only(&[StyleRule::Format]);
    // Output of the project's own `cargo fmt` under a narrower width passes.
    std::fs::write(fixture.path().join("rustfmt.toml"), "max_width = 80\n").unwrap();
    let narrow = project.check(&AnalysisSettings::default(), &policy, &files);
    assert!(narrow.errors.is_empty(), "{:?}", narrow.errors);
    assert!(narrow.diagnostics.is_empty(), "{:?}", narrow.diagnostics);

    std::fs::write(fixture.path().join("rustfmt.toml"), "max_width = 120\n").unwrap();
    let wide = project.check(&AnalysisSettings::default(), &policy, &files);
    assert!(wide.errors.is_empty(), "{:?}", wide.errors);
    let widths: Vec<_> = wide
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("max_width = 120"))
        .collect();
    assert_eq!(widths.len(), 1, "{:?}", wide.diagnostics);
    assert!(widths[0].path.ends_with("rustfmt.toml"));
}

#[test]
fn flexible_references_include_borrowed_options() {
    let fixture = fixture("pub fn f(value: &Option<String>) -> bool { value.is_some() }\n");
    let project = load(fixture.path());
    let report = project.check(
        &AnalysisSettings::default(),
        &only(&[StyleRule::FlexibleReferences]),
        &[],
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert!(
        !report.diagnostics[0].message.starts_with("warning:"),
        "{}",
        report.diagnostics[0].message
    );
}

#[test]
fn unwrap_is_reported_in_production_code_but_not_in_test_code() {
    let fixture = fixture(concat!(
        "pub fn production() -> u8 { Some(1).unwrap() }\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    fn helper() -> u8 { Some(2).unwrap() }\n",
        "    #[test]\n",
        "    fn unit() { let _ = Some(3).unwrap() + helper(); }\n",
        "}\n",
    ));
    std::fs::create_dir(fixture.path().join("tests")).unwrap();
    std::fs::write(
        fixture.path().join("tests/integration.rs"),
        "fn helper() -> u8 { Some(4).unwrap() }\n#[test]\nfn integration() { let _ = Some(5).unwrap() + helper(); }\n",
    )
    .unwrap();
    let project = load(fixture.path());
    let report = project.check(
        &AnalysisSettings::default(),
        &only(&[StyleRule::Unwrap]),
        &[],
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let locations: Vec<_> = report
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.path.ends_with("src/lib.rs"), diagnostic.line))
        .collect();
    assert_eq!(locations, [(true, 1)], "{:?}", report.diagnostics);
}
