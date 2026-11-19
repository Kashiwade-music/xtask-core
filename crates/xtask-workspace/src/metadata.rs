//! Role: Cargo metadataを読み、workspaceの構造・依存の辺・featureを検証して作る。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use xtask_policy::{AnalysisSettings, Features};
use xtask_source::normalize;

use crate::model::{Dependency, DependencyKind, Edition, Package, Target, TargetKind, Workspace};

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
}

#[derive(Deserialize)]
struct SelectionMetadata {
    resolve: Option<Resolve>,
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
    pkg: String,
    dep_kinds: Vec<ResolvedKind>,
}

#[derive(Deserialize)]
struct ResolvedKind {
    kind: Option<String>,
    target: Option<String>,
}

impl ResolvedKind {
    fn matches(&self, kind: DependencyKind, target: Option<&str>) -> bool {
        self.kind.as_deref().unwrap_or("normal") == kind.as_str()
            && self.target.as_deref() == target
    }
}

/// Whether `features` needs a second `cargo metadata` run. The structural run already gives
/// the features of `Features::All` without a target.
pub fn needs_selection_run(analysis: &AnalysisSettings) -> bool {
    !(matches!(analysis.features(), Features::All) && analysis.target().is_none())
}

impl Workspace {
    /// Read the structure of the workspace with one `cargo metadata` run under `--all-features`.
    /// A failure is fatal, because nothing can be checked without the structure.
    pub fn load(root: &Path) -> Result<Self, String> {
        let json = run_metadata(root, None)?;
        Self::from_json(&json)
    }

    /// The features Cargo activates for each member under the analysis selection. A failure
    /// belongs to the analysis environment, so other checks can still run.
    pub fn features(
        &self,
        analysis: &AnalysisSettings,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        if !needs_selection_run(analysis) {
            return Ok(self.all_features.clone());
        }
        let json = run_metadata(&self.root, Some(analysis)).map_err(|error| {
            format!(
                "{error}\nPrepare the consumer's Cargo.lock and dependency cache before running checks (cargo generate-lockfile and cargo fetch)."
            )
        })?;
        let metadata: SelectionMetadata = serde_json::from_str(&json)
            .map_err(|error| format!("invalid Cargo feature metadata: {error}"))?;
        let resolve = metadata
            .resolve
            .ok_or("cargo metadata did not return a resolve graph")?;
        member_features(&resolve, &self.member_names())
    }

    fn member_names(&self) -> BTreeMap<&str, &str> {
        self.packages
            .iter()
            .map(|package| (package.id.as_str(), package.name.as_str()))
            .collect()
    }

    /// Parse one full metadata document. Paths must exist, so filesystem identity is checked.
    pub(crate) fn from_json(json: &str) -> Result<Self, String> {
        let metadata: Metadata = serde_json::from_str(json)
            .map_err(|error| format!("invalid Cargo metadata: {error}"))?;
        let root = normalize(&metadata.workspace_root)?;
        let names: BTreeMap<String, String> = metadata
            .packages
            .iter()
            .map(|package| (package.id.clone(), package.name.clone()))
            .collect();
        let members = select_members(metadata.packages, &metadata.workspace_members)?;
        let resolve = metadata
            .resolve
            .ok_or("cargo metadata did not return a resolve graph")?;
        let (edges, all_features) = {
            let member_names: BTreeMap<&str, &str> = members
                .iter()
                .map(|(package, _)| (package.id.as_str(), package.name.as_str()))
                .collect();
            (
                dependency_edges(&members, &names, &member_names, &resolve)?,
                member_features(&resolve, &member_names)?,
            )
        };
        let mut packages = members
            .into_iter()
            .map(|(package, manifest)| package_record(package, manifest))
            .collect::<Result<Vec<_>, String>>()?;
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self {
            root,
            packages,
            edges: edges.into_iter().collect(),
            all_features,
        })
    }
}

/// The one `cargo metadata` command. `None` is the structural run; `Some` selects features.
fn metadata_command(root: &Path, selection: Option<&AnalysisSettings>) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .args(["metadata", "--format-version", "1", "--locked", "--offline"])
        .current_dir(root);
    match selection {
        None => {
            command.arg("--all-features");
        }
        Some(analysis) => {
            match analysis.features() {
                Features::All => {
                    command.arg("--all-features");
                }
                Features::Selected {
                    features,
                    default_features,
                } => {
                    if !features.is_empty() {
                        let list: Vec<&str> = features.iter().map(String::as_str).collect();
                        command.arg("--features").arg(list.join(","));
                    }
                    if !default_features {
                        command.arg("--no-default-features");
                    }
                }
            }
            if let Some(target) = analysis.target() {
                command.arg("--filter-platform").arg(target);
            }
        }
    }
    command
}

fn run_metadata(root: &Path, selection: Option<&AnalysisSettings>) -> Result<String, String> {
    let output = metadata_command(root, selection)
        .output()
        .map_err(|error| format!("cannot run cargo metadata in {}: {error}", root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim();
        return Err(format!("cargo metadata failed: {last}"));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("cargo metadata output is not UTF-8: {error}"))
}

/// Select the workspace members, normalizing each manifest path once.
fn select_members(
    packages: Vec<MetadataPackage>,
    members: &[String],
) -> Result<Vec<(MetadataPackage, PathBuf)>, String> {
    let wanted: BTreeSet<&str> = members.iter().map(String::as_str).collect();
    let mut found = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    let mut selected = Vec::new();
    for package in packages {
        if !wanted.contains(package.id.as_str()) {
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
        let manifest = normalize(&package.manifest_path)?;
        if !manifests.insert(manifest.clone()) {
            return Err(format!(
                "duplicate workspace manifest {}",
                manifest.display()
            ));
        }
        selected.push((package, manifest));
    }
    if let Some(missing) = members.iter().find(|id| !found.contains(id.as_str())) {
        return Err(format!(
            "workspace member `{missing}` is missing from Cargo metadata packages"
        ));
    }
    Ok(selected)
}

fn package_record(package: MetadataPackage, manifest_path: PathBuf) -> Result<Package, String> {
    let targets = package
        .targets
        .into_iter()
        .map(target_record)
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Package {
        id: package.id,
        name: package.name,
        manifest_path,
        targets,
    })
}

fn target_record(target: MetadataTarget) -> Result<Target, String> {
    Ok(Target {
        name: target.name,
        kinds: target
            .kind
            .iter()
            .map(String::as_str)
            .map(TargetKind::from_cargo)
            .collect(),
        src_path: normalize(&target.src_path)?,
        required_features: target.required_features,
        test: target.test,
        edition: Edition::from_cargo(&target.edition),
    })
}

/// The declared dependencies between members, from the manifests of the members.
fn dependency_edges(
    members: &[(MetadataPackage, PathBuf)],
    names: &BTreeMap<String, String>,
    member_names: &BTreeMap<&str, &str>,
    resolve: &Resolve,
) -> Result<BTreeSet<Dependency>, String> {
    let directories: BTreeMap<PathBuf, &str> = members
        .iter()
        .filter_map(|(package, manifest)| {
            manifest
                .parent()
                .map(|directory| (directory.to_path_buf(), package.name.as_str()))
        })
        .collect();
    let member_name_set: BTreeSet<&str> = member_names.values().copied().collect();
    let mut edges = BTreeSet::new();
    for (package, _) in members {
        for dependency in &package.dependencies {
            let kind = DependencyKind::from_cargo(dependency.kind.as_deref().unwrap_or("normal"))?;
            let provider = match &dependency.path {
                Some(path) => directories.get(&normalize(path)?).copied(),
                None if member_name_set.contains(dependency.name.as_str()) => {
                    let id = registry_package_id(resolve, package, dependency, kind, names)?;
                    member_names.get(id.as_str()).copied()
                }
                None => None,
            };
            if let Some(provider) = provider {
                edges.insert(Dependency {
                    from: package.name.clone(),
                    to: provider.into(),
                    kind,
                    target: dependency.target.clone(),
                });
            }
        }
    }
    Ok(edges)
}

/// The package ID that a declaration without a `path` resolves to. Each declaration must
/// resolve to exactly one package, so a patched or a registry package is told apart by ID.
fn registry_package_id(
    resolve: &Resolve,
    package: &MetadataPackage,
    dependency: &MetadataDependency,
    kind: DependencyKind,
    names: &BTreeMap<String, String>,
) -> Result<String, String> {
    let node = resolve
        .nodes
        .iter()
        .find(|node| node.id == package.id)
        .ok_or_else(|| {
            format!(
                "workspace member `{}` is missing from Cargo resolve graph",
                package.name
            )
        })?;
    let target = dependency.target.as_deref();
    let candidates: BTreeSet<&str> = node
        .deps
        .iter()
        .filter(|dep| {
            names
                .get(&dep.pkg)
                .is_some_and(|name| *name == dependency.name)
        })
        .filter(|dep| {
            dep.dep_kinds
                .iter()
                .any(|kinds| kinds.matches(kind, target))
        })
        .map(|dep| dep.pkg.as_str())
        .collect();
    let mut candidates = candidates.into_iter();
    match (candidates.next(), candidates.next()) {
        (Some(id), None) => Ok(id.to_string()),
        _ => Err(format!(
            "cannot resolve registry dependency identity for {} of {}",
            dependency.name, package.name
        )),
    }
}

/// The features of each member, by package name, from the resolve graph.
fn member_features(
    resolve: &Resolve,
    members: &BTreeMap<&str, &str>,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut features = BTreeMap::new();
    let mut found = BTreeSet::new();
    for node in &resolve.nodes {
        let Some(name) = members.get(node.id.as_str()) else {
            continue;
        };
        if !found.insert(node.id.as_str()) {
            return Err(format!("duplicate resolve node `{}`", node.id));
        }
        let activated: BTreeSet<String> = node.features.iter().cloned().collect();
        if features.insert((*name).to_string(), activated).is_some() {
            return Err(format!("duplicate workspace package name `{name}`"));
        }
    }
    if let Some(missing) = members.keys().copied().find(|id| !found.contains(id)) {
        return Err(format!(
            "workspace member `{missing}` is missing from Cargo resolve graph"
        ));
    }
    Ok(features)
}
