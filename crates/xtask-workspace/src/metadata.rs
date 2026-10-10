//! Role: Cargoの宣言・解決情報からworkspaceの依存とfeatureを取得する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::model::{Dependency, Package, Target, Workspace};

#[derive(Deserialize)]
struct Metadata {
    workspace_root: PathBuf,
    workspace_members: Vec<String>,
    packages: Vec<MetadataPackage>,
    resolve: Option<Resolve>,
}

#[derive(Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
    features: BTreeMap<String, Vec<String>>,
    dependencies: Vec<MetadataDependency>,
}

#[derive(Deserialize)]
struct MetadataTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
    #[serde(default = "default_test")]
    test: bool,
    #[serde(default = "default_edition")]
    edition: String,
}

fn default_edition() -> String {
    "2015".into()
}

const fn default_test() -> bool {
    true
}

#[derive(Deserialize)]
struct MetadataDependency {
    name: String,
    kind: Option<String>,
    target: Option<String>,
    path: Option<PathBuf>,
    rename: Option<String>,
}

#[derive(Deserialize)]
struct FeatureMetadata {
    workspace_members: Vec<String>,
    packages: Vec<FeaturePackage>,
    resolve: Option<Resolve>,
}

#[derive(Deserialize)]
struct FeaturePackage {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<ResolveNode>,
}

#[derive(Deserialize)]
struct ResolveNode {
    id: String,
    features: Vec<String>,
    #[serde(default)]
    deps: Vec<ResolvedDependency>,
}

#[derive(Deserialize)]
struct ResolvedDependency {
    name: String,
    pkg: String,
    dep_kinds: Vec<ResolvedKind>,
}

#[derive(Deserialize)]
struct ResolvedKind {
    kind: Option<String>,
    target: Option<String>,
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))
}

impl Workspace {
    /// Ask Cargo for resolved feature activation, including dependency feature
    /// requests and unification. The consumer's lockfile and cache must exist.
    /// Unlike declaration loading this resolves external dependencies, but never
    /// downloads crates or changes the lockfile.
    pub fn resolved_features(
        root: &Path,
        features: &[String],
        default_features: bool,
        all_features: bool,
        target: Option<&str>,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut command = Command::new(cargo);
        command
            .args(["metadata", "--format-version", "1", "--locked", "--offline"])
            .current_dir(root);
        if !features.is_empty() {
            command.arg("--features").arg(features.join(","));
        }
        if !default_features {
            command.arg("--no-default-features");
        }
        if all_features {
            command.arg("--all-features");
        }
        if let Some(target) = target {
            command.arg("--filter-platform").arg(target);
        }
        let output = command.output().map_err(|error| {
            format!(
                "cannot resolve Cargo features in {}: {error}",
                root.display()
            )
        })?;
        if !output.status.success() {
            return Err(format!(
                "Cargo feature resolution in {} failed ({}): {}\nPrepare the consumer's Cargo.lock and dependency cache before running checks (cargo generate-lockfile and cargo fetch).",
                root.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Self::resolved_features_from_json(
            &String::from_utf8(output.stdout)
                .map_err(|error| format!("Cargo feature metadata is not UTF-8: {error}"))?,
        )
    }

    /// Read the member feature map from full Cargo metadata, by package ID.
    pub fn resolved_features_from_json(
        json: &str,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        let metadata: FeatureMetadata = serde_json::from_str(json)
            .map_err(|error| format!("invalid Cargo feature metadata: {error}"))?;
        let members: BTreeSet<_> = metadata.workspace_members.into_iter().collect();
        let packages: BTreeMap<_, _> = metadata
            .packages
            .into_iter()
            .filter(|package| members.contains(&package.id))
            .map(|package| (package.id, package.name))
            .collect();
        if let Some(missing) = members.iter().find(|id| !packages.contains_key(*id)) {
            return Err(format!(
                "workspace member `{missing}` is missing from feature metadata packages"
            ));
        }
        let resolve = metadata
            .resolve
            .ok_or("Cargo feature metadata has no resolve graph")?;
        let mut features = BTreeMap::new();
        let mut found = BTreeSet::new();
        for node in resolve.nodes {
            if let Some(name) = packages.get(&node.id) {
                if !found.insert(node.id.clone()) {
                    return Err(format!("duplicate resolve node `{}`", node.id));
                }
                if features
                    .insert(name.clone(), node.features.into_iter().collect())
                    .is_some()
                {
                    return Err(format!("duplicate workspace package name `{name}`"));
                }
            }
        }
        if let Some(missing) = members.difference(&found).next() {
            return Err(format!(
                "workspace member `{missing}` is missing from Cargo resolve graph"
            ));
        }
        Ok(features)
    }

    /// Run offline declaration metadata, resolving ambiguous same-name registry
    /// dependencies when needed to distinguish patches from external packages.
    /// `CARGO` may select the Cargo executable used by the caller's xtask.
    pub fn load(root: &Path) -> Result<Self, String> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
                "--offline",
            ])
            .current_dir(root)
            .output()
            .map_err(|error| format!("cannot run cargo metadata in {}: {error}", root.display()))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata in {} failed ({}): {}",
                root.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let json = std::str::from_utf8(&output.stdout)
            .map_err(|error| format!("cargo metadata output is not UTF-8: {error}"))?;
        let metadata: Metadata = serde_json::from_str(json)
            .map_err(|error| format!("invalid Cargo metadata: {error}"))?;
        let members: BTreeSet<_> = metadata.workspace_members.iter().collect();
        let member_names: BTreeSet<_> = metadata
            .packages
            .iter()
            .filter(|package| members.contains(&package.id))
            .map(|package| package.name.as_str())
            .collect();
        let ambiguous = metadata
            .packages
            .iter()
            .filter(|package| members.contains(&package.id))
            .any(|package| {
                package
                    .dependencies
                    .iter()
                    .any(|dep| dep.path.is_none() && member_names.contains(dep.name.as_str()))
            });
        if ambiguous {
            let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args([
                    "metadata",
                    "--format-version",
                    "1",
                    "--locked",
                    "--offline",
                    "--all-features",
                ])
                .current_dir(root)
                .output()
                .map_err(|error| {
                    format!("cannot resolve ambiguous workspace dependency identity: {error}")
                })?;
            if !output.status.success() {
                return Err(format!(
                    "cannot resolve registry dependency identity against same-name workspace members: {}\nPrepare the consumer's Cargo.lock and dependency cache (cargo generate-lockfile and cargo fetch).",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            let json = std::str::from_utf8(&output.stdout)
                .map_err(|error| format!("cargo metadata output is not UTF-8: {error}"))?;
            let full: Metadata = serde_json::from_str(json)
                .map_err(|error| format!("invalid resolved Cargo metadata: {error}"))?;
            if full.resolve.is_none() {
                return Err("cannot resolve ambiguous dependency identity: full Cargo metadata has no resolve graph".into());
            }
            return Self::from_json(json);
        }
        Self::from_json(json)
    }

    /// Parse Cargo metadata. Paths must exist, so filesystem identity is checked.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let metadata: Metadata = serde_json::from_str(json)
            .map_err(|error| format!("invalid Cargo metadata: {error}"))?;
        let root = canonical(&metadata.workspace_root)?;
        let resolved: Option<BTreeMap<_, _>> = metadata.resolve.map(|resolve| {
            resolve
                .nodes
                .into_iter()
                .map(|node| (node.id, node.deps))
                .collect()
        });
        let resolved_libraries: BTreeMap<_, _> = metadata
            .packages
            .iter()
            .map(|package| {
                let libraries: BTreeSet<_> = package
                    .targets
                    .iter()
                    .filter(|target| {
                        target.kind.iter().any(|kind| {
                            matches!(
                                kind.as_str(),
                                "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro"
                            )
                        })
                    })
                    .map(|target| target.name.replace('-', "_"))
                    .collect();
                (package.id.clone(), (package.name.clone(), libraries))
            })
            .collect();
        let members: BTreeSet<_> = metadata.workspace_members.into_iter().collect();
        let mut found = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut manifests = BTreeMap::new();
        let mut selected = Vec::new();
        for package in metadata.packages {
            if !members.contains(&package.id) {
                continue;
            }
            if !found.insert(package.id.clone()) {
                return Err(format!("duplicate workspace package ID `{}`", package.id));
            }
            if !names.insert(package.name.clone()) {
                return Err(format!(
                    "duplicate workspace package name `{}`",
                    package.name
                ));
            }
            let manifest = canonical(&package.manifest_path)?;
            if manifests
                .insert(manifest.clone(), package.id.clone())
                .is_some()
            {
                return Err(format!(
                    "duplicate workspace manifest {}",
                    manifest.display()
                ));
            }
            selected.push((package, manifest));
        }
        if let Some(missing) = members.difference(&found).next() {
            return Err(format!(
                "workspace member `{missing}` is missing from Cargo metadata packages"
            ));
        }
        let by_id: BTreeMap<_, _> = selected
            .iter()
            .map(|(package, _)| (package.id.as_str(), package.name.as_str()))
            .collect();
        let mut edges = BTreeSet::new();
        for (package, _) in &selected {
            for dependency in &package.dependencies {
                let kind = dependency.kind.as_deref().unwrap_or("normal");
                let provider_id = if let Some(path) = &dependency.path {
                    let manifest = canonical(&path.join("Cargo.toml"))?;
                    manifests.get(&manifest).cloned()
                } else if let Some(resolved) = &resolved {
                    if !names.contains(&dependency.name) {
                        continue;
                    }
                    let alias = dependency
                        .rename
                        .as_deref()
                        .unwrap_or(&dependency.name)
                        .replace('-', "_");
                    let deps = resolved.get(&package.id).ok_or_else(|| {
                        format!(
                            "workspace member `{}` is missing from Cargo resolve graph",
                            package.name
                        )
                    })?;
                    let candidates: BTreeSet<_> = deps
                        .iter()
                        .filter(|dep| {
                            let name_matches = if dependency.rename.is_some() {
                                dep.name == alias
                            } else {
                                resolved_libraries
                                    .get(&dep.pkg)
                                    .is_some_and(|(name, libraries)| {
                                        name == &dependency.name && libraries.contains(&dep.name)
                                    })
                            };
                            name_matches
                                && dep.dep_kinds.iter().any(|resolved_kind| {
                                    resolved_kind.kind.as_deref().unwrap_or("normal") == kind
                                        && resolved_kind.target == dependency.target
                                })
                        })
                        .map(|dep| &dep.pkg)
                        .collect();
                    if candidates.len() != 1 {
                        return Err(format!(
                            "cannot uniquely resolve `{}` {} dependency of `{}` for {:?}; provide full unfiltered Cargo metadata with --all-features",
                            dependency.name, kind, package.name, dependency.target
                        ));
                    }
                    candidates.into_iter().next().cloned()
                } else {
                    // No resolve graph cannot prove a registry declaration was
                    // patched locally. `load` resolves ambiguous names first.
                    None
                };
                let Some(provider_id) = provider_id else {
                    continue;
                };
                let Some(provider) = by_id.get(provider_id.as_str()).copied() else {
                    continue;
                };
                if dependency.name != provider {
                    return Err(format!(
                        "dependency `{}` of `{}` points to workspace package `{provider}` ({provider_id})",
                        dependency.name, package.name
                    ));
                }
                if !matches!(kind, "normal" | "dev" | "build") {
                    return Err(format!("invalid Cargo dependency kind `{kind}`"));
                }
                edges.insert(Dependency {
                    from: package.name.clone(),
                    to: provider.into(),
                    kind: kind.into(),
                    target: dependency.target.clone(),
                });
            }
        }
        let mut packages: Vec<_> = selected
            .into_iter()
            .map(|(package, manifest)| Package {
                id: package.id,
                name: package.name,
                manifest_path: manifest,
                features: package.features,
                targets: package
                    .targets
                    .into_iter()
                    .map(|target| Target {
                        name: target.name,
                        kind: target.kind,
                        src_path: target.src_path,
                        required_features: target.required_features,
                        test: target.test,
                        edition: target.edition,
                    })
                    .collect(),
            })
            .collect();
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self {
            root,
            packages,
            edges: edges.into_iter().collect(),
        })
    }
}
