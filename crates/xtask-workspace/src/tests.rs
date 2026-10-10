use std::collections::BTreeMap;
use std::fs;

use serde_json::json;

use crate::{Package, Policy, Workspace};

fn fixture() -> tempfile::TempDir {
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/workspace-fixtures");
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
    directory
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
    let workspace = Workspace::load(directory.path()).unwrap();
    assert_eq!(workspace.packages.len(), 2);
    assert_eq!(workspace.edges.len(), 4);
    assert!(
        workspace
            .edges
            .iter()
            .all(|edge| edge.from == "app" && edge.to == "core")
    );
    for kind in ["normal", "dev", "build"] {
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
            .find(|target| target.kind.contains(&"lib".into()))
            .unwrap()
            .test
    );
    assert_eq!(
        app.active_features(&[], true, false)
            .unwrap()
            .into_iter()
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
    assert_eq!(workspace.edges[0].kind, "normal");
}

#[test]
fn visibility_checks_stale_providers_consumers_and_all_declared_edges() {
    let directory = fixture();
    fs::write(
        directory.path().join("app/Cargo.toml"),
        concat!(
            "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
            "[build-dependencies]\ncore={path='../core'}\n"
        ),
    )
    .unwrap();
    let workspace = Workspace::load(directory.path()).unwrap();
    let policy = Policy::from_toml("[visible_to]\ncore=['other*']\ngone=[]\n").unwrap();
    let errors = workspace.check_visibility(&policy);
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("`gone` is not a workspace member"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("`other*` matches no workspace member"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("build dependency on `core`"))
    );
}

#[test]
fn policy_defaults_and_patterns_and_unknown_fields_are_strict() {
    let policy = Policy::from_toml("[visible_to]\ncore=['app*']\nprivate=[]\n").unwrap();
    assert_eq!(policy.limits.max_code_lines, 1000);
    assert!(policy.visible("open", "anything"));
    assert!(policy.visible("core", "app"));
    assert!(policy.visible("core", "app-shell"));
    assert!(!policy.visible("core", "other"));
    assert!(!policy.visible("private", "app"));
    assert!(policy.visible("private", "private"));
    for invalid in [
        "[limits]\nmax_code_lines=1001\n",
        "[limits]\nmax_prelude_names=1\n",
        "[dependency_allowlist]\ncore=[]\n",
        "[visible_to]\ncore=['app**']\n",
        "[visible_to]\ncore=['a*p']\n",
        "[visible_to]\ncore=['']\n",
    ] {
        assert!(Policy::from_toml(invalid).is_err(), "accepted {invalid}");
    }
    assert!(Policy::load(std::path::Path::new("missing-policy.toml")).is_err());
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
fn feature_closure_handles_cycles_dependency_entries_and_unknown_requests() {
    let package = Package {
        id: "test".into(),
        name: "test".into(),
        manifest_path: "Cargo.toml".into(),
        targets: vec![],
        features: BTreeMap::from([
            (
                "first".into(),
                vec![
                    "second".into(),
                    "dep:optional".into(),
                    "dep_name/feature".into(),
                ],
            ),
            (
                "second".into(),
                vec!["first".into(), "dep_name?/feature".into()],
            ),
            ("extra".into(), vec![]),
        ]),
    };
    assert_eq!(
        package
            .active_features(&["first".into()], false, false)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(package.active_features(&[], false, true).unwrap().len(), 3);
    assert!(
        package
            .active_features(&["absent".into()], false, false)
            .is_err()
    );
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
    let status =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["generate-lockfile", "--offline"])
            .current_dir(directory.path())
            .status()
            .unwrap();
    assert!(status.success());
    let lockfile = fs::read(directory.path().join("Cargo.lock")).unwrap();
    let features = Workspace::resolved_features(directory.path(), &[], true, false, None).unwrap();
    assert!(features["app"].contains("default"));
    assert!(features["core"].contains("via-dependency"));
    assert!(features["core"].contains("transitive"));
    assert!(features["core"].contains("base"));
    let features = Workspace::resolved_features(directory.path(), &[], false, false, None).unwrap();
    assert!(features["app"].is_empty());
    assert!(features["core"].is_empty(), "{features:?}");
    let features = Workspace::resolved_features(
        directory.path(),
        &["app/default".into()],
        false,
        false,
        None,
    )
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
    let error = Workspace::resolved_features(directory.path(), &[], true, false, None).unwrap_err();
    assert!(
        error.contains("Prepare the consumer's Cargo.lock"),
        "{error}"
    );
    assert!(!directory.path().join("Cargo.lock").exists());
}

#[test]
fn resolved_features_match_member_ids_instead_of_external_package_names() {
    let metadata = json!({
        "workspace_members":["member"],
        "packages":[{"id":"member","name":"core"},{"id":"external","name":"core"}],
        "resolve":{"nodes":[
            {"id":"member","features":["local"]},
            {"id":"external","features":["external"]}
        ]}
    });
    let features = Workspace::resolved_features_from_json(&metadata.to_string()).unwrap();
    assert_eq!(
        features["core"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["local"]
    );
    assert!(Workspace::resolved_features_from_json("{}").is_err());
    let mut metadata = metadata;
    metadata["resolve"] = serde_json::Value::Null;
    assert!(Workspace::resolved_features_from_json(&metadata.to_string()).is_err());
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
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(&cargo)
        .args(["generate-lockfile", "--offline"])
        .current_dir(directory.path())
        .status()
        .unwrap();
    assert!(status.success());
    let declaration = std::process::Command::new(&cargo)
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
            .contains("cannot resolve registry dependency identity")
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
    assert!(
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["generate-lockfile", "--offline"])
            .current_dir(directory.path())
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        Workspace::load(directory.path()).unwrap().edges[0].to,
        "core"
    );
}
