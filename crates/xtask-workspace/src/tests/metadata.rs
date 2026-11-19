//! Role: Cargo metadata由来のworkspace構造、依存の同一性、feature解決を検証する。

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use xtask_policy::{AnalysisSettings, Features};

use crate::needs_selection_run;
use crate::{DependencyKind, TargetKind, Workspace};

fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn lock(directory: &Path) {
    let status = Command::new(cargo())
        .args(["generate-lockfile", "--offline"])
        .current_dir(directory)
        .status()
        .unwrap();
    assert!(status.success());
}

fn fixture() -> tempfile::TempDir {
    let fixture_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/workspace-fixtures");
    fs::create_dir_all(&fixture_root).unwrap();
    let directory = tempfile::tempdir_in(&fixture_root).unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nmembers=['core', 'app']\nresolver='3'\n",
    )
    .unwrap();
    for package in ["core", "app"] {
        fs::create_dir_all(directory.path().join(package).join("src")).unwrap();
        fs::write(directory.path().join(package).join("src/lib.rs"), "").unwrap();
        fs::write(
            directory.path().join(package).join("Cargo.toml"),
            format!("[package]\nname='{package}'\nversion='0.1.0'\nedition='2024'\n"),
        )
        .unwrap();
    }
    lock(directory.path());
    directory
}

fn settings(features: Features, target: Option<&str>) -> AnalysisSettings {
    AnalysisSettings::new(
        target.map(String::from),
        features,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap()
}

fn selected(features: &[&str], default_features: bool) -> Features {
    Features::Selected {
        features: features
            .iter()
            .map(|feature| (*feature).to_string())
            .collect(),
        default_features,
    }
}

#[test]
fn real_cargo_metadata_retains_all_dependency_kinds_and_target_features() {
    let directory = fixture();
    let manifest = directory.path().join("app/Cargo.toml");
    fs::write(
        &manifest,
        concat!(
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
            "[lib]\ntest=false\n",
            "[dependencies]\ncore_alias={package='core',path='../core'}\n",
            "[dev-dependencies]\ncore_alias={package='core',path='../core'}\n",
            "[build-dependencies]\ncore_alias={package='core',path='../core'}\n",
            "[target.'cfg(windows)'.dependencies]\ncore_alias={package='core',path='../core'}\n",
            "[features]\ndefault=['first']\nfirst=['second']\nsecond=[]\n",
            "[[example]]\nname='feature-example'\nrequired-features=['second']\n"
        ),
    )
    .unwrap();
    fs::create_dir_all(directory.path().join("app/examples")).unwrap();
    fs::write(
        directory.path().join("app/examples/feature-example.rs"),
        "fn main() {}\n",
    )
    .unwrap();
    lock(directory.path());
    let workspace = Workspace::load(directory.path()).unwrap();
    assert_eq!(workspace.packages.len(), 2);
    assert_eq!(workspace.edges.len(), 4);
    assert!(
        workspace
            .edges
            .iter()
            .all(|edge| edge.from == "app" && edge.to == "core")
    );
    for kind in [
        DependencyKind::Normal,
        DependencyKind::Dev,
        DependencyKind::Build,
    ] {
        assert!(workspace.edges.iter().any(|edge| edge.kind == kind));
    }
    assert!(
        workspace
            .edges
            .iter()
            .any(|edge| edge.target.as_deref() == Some("cfg(windows)"))
    );
    let app = workspace
        .packages
        .iter()
        .find(|package| package.name == "app")
        .unwrap();
    assert!(
        !app.targets
            .iter()
            .find(|target| target.kinds.contains(&TargetKind::Lib))
            .unwrap()
            .test
    );
    let features = workspace
        .features(&settings(selected(&[], true), None))
        .unwrap();
    assert_eq!(
        features["app"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["default", "first", "second"]
    );
    assert_eq!(
        app.targets
            .iter()
            .find(|target| target.name == "feature-example")
            .unwrap()
            .required_features,
        ["second"]
    );
}

#[test]
fn metadata_filters_members_and_does_not_confuse_external_names_or_aliases() {
    let directory = fixture();
    let root = directory.path();
    let package = |name: &str, id: &str, dependencies| {
        json!({
            "id": id, "name": name, "manifest_path": root.join(name).join("Cargo.toml"),
            "targets": [], "features": {}, "dependencies": dependencies,
        })
    };
    let data = json!({
        "workspace_root": root,
        "workspace_members": ["local-core", "local-app"],
        "packages": [
            package("core", "local-core", json!([])),
            package("app", "local-app", json!([
                {"name":"core", "rename":"alias", "kind":null, "target":null, "path":root.join("core")},
                {"name":"core", "rename":"registry_alias", "kind":"dev", "target":null, "path":null},
            ])),
            {"id":"registry-core", "name":"core", "manifest_path":"nonexistent", "targets":[], "features":{}, "dependencies":[]}
        ],
        "resolve":{"nodes":[
            {"id":"local-core","features":[],"deps":[]},
            {"id":"local-app","features":[],"deps":[
                {"name":"registry_alias","pkg":"registry-core","dep_kinds":[{"kind":"dev","target":null}]}
            ]}
        ]}
    });
    let workspace = Workspace::from_json(&data.to_string()).unwrap();
    assert_eq!(workspace.packages.len(), 2);
    assert_eq!(workspace.edges.len(), 1);
    assert_eq!(workspace.edges[0].to, "core");
    assert_eq!(workspace.edges[0].kind, DependencyKind::Normal);
}

#[test]
fn malformed_metadata_and_missing_paths_are_errors() {
    assert!(Workspace::from_json("{}").is_err());
    let directory = fixture();
    let value =
        json!({"workspace_root": directory.path(), "workspace_members":["absent"], "packages":[]});
    assert!(
        Workspace::from_json(&value.to_string())
            .unwrap_err()
            .contains("missing")
    );
    let value = json!({"workspace_root":directory.path(), "workspace_members":["app"], "packages":[{
        "id":"app", "name":"app", "manifest_path":directory.path().join("app/Cargo.toml"),
        "targets":[], "features":{}, "dependencies":[{
            "name":"gone", "kind":null, "target":null, "path":directory.path().join("gone")
        }]
    }]});
    assert!(Workspace::from_json(&value.to_string()).is_err());
}

#[test]
fn resolved_features_use_cargo_dependency_activation_and_unification() {
    let directory = fixture();
    fs::write(
        directory.path().join("core/Cargo.toml"),
        concat!(
            "[package]\nname='core'\nversion='0.1.0'\nedition='2024'\n",
            "[features]\ndefault=['base']\nbase=[]\nvia-dependency=['transitive']\ntransitive=[]\n"
        ),
    )
    .unwrap();
    fs::write(directory.path().join("app/Cargo.toml"), concat!(
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
        "[dependencies]\ncore_alias={package='core',path='../core',optional=true,default-features=false,features=['via-dependency']}\n",
        "[features]\ndefault=['dep:core_alias']\n"
    )).unwrap();
    lock(directory.path());
    let lockfile = fs::read(directory.path().join("Cargo.lock")).unwrap();
    let workspace = Workspace::load(directory.path()).unwrap();
    let features = workspace
        .features(&settings(selected(&[], true), None))
        .unwrap();
    assert!(features["app"].contains("default"));
    assert!(features["core"].contains("via-dependency"));
    assert!(features["core"].contains("transitive"));
    assert!(features["core"].contains("base"));
    let features = workspace
        .features(&settings(selected(&[], false), None))
        .unwrap();
    assert!(features["app"].is_empty());
    assert!(features["core"].is_empty(), "{features:?}");
    let features = workspace
        .features(&settings(selected(&["app/default"], false), None))
        .unwrap();
    assert!(features["core"].contains("via-dependency"));
    assert!(!features["core"].contains("base"));
    assert_eq!(
        fs::read(directory.path().join("Cargo.lock")).unwrap(),
        lockfile
    );
}

#[test]
fn feature_resolution_without_lockfile_reports_preparation_and_does_not_create_it() {
    let directory = fixture();
    let workspace = Workspace::load(directory.path()).unwrap();
    fs::remove_file(directory.path().join("Cargo.lock")).unwrap();
    let error = workspace
        .features(&settings(selected(&[], true), None))
        .unwrap_err();
    assert!(
        error.contains("Prepare the consumer's Cargo.lock"),
        "{error}"
    );
    assert!(!directory.path().join("Cargo.lock").exists());
}

#[test]
fn resolved_features_match_member_ids_instead_of_external_package_names() {
    let directory = fixture();
    let root = directory.path();
    let data = json!({
        "workspace_root": root,
        "workspace_members": ["member"],
        "packages": [
            {"id":"member","name":"core","manifest_path":root.join("core/Cargo.toml"),"targets":[],"dependencies":[]},
            {"id":"external","name":"core","manifest_path":"nonexistent","targets":[],"dependencies":[]}
        ],
        "resolve":{"nodes":[
            {"id":"member","features":["local"],"deps":[]},
            {"id":"external","features":["external"],"deps":[]}
        ]}
    });
    let workspace = Workspace::from_json(&data.to_string()).unwrap();
    let features = workspace.features(&settings(Features::All, None)).unwrap();
    assert_eq!(
        features["core"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["local"]
    );
    assert!(Workspace::from_json("{}").is_err());
    let mut without_resolve = data;
    without_resolve["resolve"] = serde_json::Value::Null;
    assert!(Workspace::from_json(&without_resolve.to_string()).is_err());
}

#[test]
fn registry_patch_workspace_edges_are_resolved_by_package_id() {
    let directory = fixture();
    fs::write(
        directory.path().join("Cargo.toml"),
        concat!(
            "[workspace]\nmembers=['core','app']\nresolver='3'\n",
            "[patch.crates-io]\ncore={path='core'}\n"
        ),
    )
    .unwrap();
    fs::write(
        directory.path().join("app/Cargo.toml"),
        concat!(
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
            "[dependencies]\nalias-name={package='core',version='0.1.0',optional=true}\n",
            "[target.'cfg(windows)'.build-dependencies]\nalias-name={package='core',version='0.1.0'}\n"
        ),
    )
    .unwrap();
    lock(directory.path());
    let declaration = Command::new(cargo())
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(declaration.status.success());
    let data: serde_json::Value = serde_json::from_slice(&declaration.stdout).unwrap();
    let app = data["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "app")
        .unwrap();
    assert!(app["dependencies"][0]["path"].is_null());
    let lockfile = fs::read(directory.path().join("Cargo.lock")).unwrap();
    let workspace = Workspace::load(directory.path()).unwrap();
    assert_eq!(workspace.edges.len(), 2, "{workspace:?}");
    assert!(workspace.edges.iter().all(|edge| edge.to == "core"));
    assert_eq!(
        fs::read(directory.path().join("Cargo.lock")).unwrap(),
        lockfile
    );
    fs::remove_file(directory.path().join("Cargo.lock")).unwrap();
    assert!(
        Workspace::load(directory.path())
            .unwrap_err()
            .contains("cargo metadata failed")
    );
    assert!(!directory.path().join("Cargo.lock").exists());
}

#[test]
fn registry_patch_with_custom_library_name_is_resolved_by_package_identity() {
    let directory = fixture();
    fs::write(
        directory.path().join("Cargo.toml"),
        concat!(
            "[workspace]\nmembers=['core','app']\nresolver='3'\n",
            "[patch.crates-io]\ncore={path='core'}\n"
        ),
    )
    .unwrap();
    fs::write(
        directory.path().join("core/Cargo.toml"),
        concat!(
            "[package]\nname='core'\nversion='0.1.0'\nedition='2024'\n",
            "[lib]\nname='core_api'\n"
        ),
    )
    .unwrap();
    fs::write(
        directory.path().join("app/Cargo.toml"),
        concat!(
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
            "[dependencies]\ncore={version='0.1.0'}\n"
        ),
    )
    .unwrap();
    lock(directory.path());
    assert_eq!(
        Workspace::load(directory.path()).unwrap().edges[0].to,
        "core"
    );
}

#[test]
fn target_kind_library() {
    assert!(TargetKind::ProcMacro.is_library());
    assert!(TargetKind::Staticlib.is_library());
    assert!(TargetKind::Lib.is_library());
    assert!(!TargetKind::Bin.is_library());
    assert!(!TargetKind::Other("custom".into()).is_library());
    assert_eq!(TargetKind::ProcMacro.as_str(), "proc-macro");
}

#[test]
fn inactive_optional_path_dependency_is_an_edge() {
    let directory = fixture();
    fs::write(
        directory.path().join("app/Cargo.toml"),
        concat!(
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
            "[dependencies]\ncore={path='../core',optional=true}\n"
        ),
    )
    .unwrap();
    lock(directory.path());
    let workspace = Workspace::load(directory.path()).unwrap();
    assert!(
        workspace
            .edges
            .iter()
            .any(|edge| edge.from == "app" && edge.to == "core")
    );
}

#[test]
fn selection_run_only_when_needed() {
    assert!(!needs_selection_run(&settings(Features::All, None)));
    assert!(needs_selection_run(&AnalysisSettings::default()));
    assert!(needs_selection_run(&settings(
        Features::All,
        Some("x86_64-pc-windows-msvc")
    )));
}

#[test]
fn bad_feature_fails_only_features() {
    let directory = fixture();
    let workspace = Workspace::load(directory.path()).unwrap();
    assert!(
        workspace
            .features(&settings(selected(&["absent"], true), None))
            .is_err()
    );
}
