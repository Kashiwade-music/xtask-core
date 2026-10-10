use crate::policy::{self, Policy};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// A Cargo workspace, restricted to the IDs in `workspace_members`.
#[derive(Clone, Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<Package>,
    pub edges: Vec<Dependency>,
}

#[derive(Clone, Debug)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub manifest_path: PathBuf,
    pub targets: Vec<Target>,
    pub features: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub name: String,
    pub kind: Vec<String>,
    pub src_path: PathBuf,
    pub required_features: Vec<String>,
    /// Whether Cargo enables the target's test harness by default.
    pub test: bool,
    pub edition: String,
}

/// A declared member-to-member dependency. Endpoints are unique package names.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Dependency {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub target: Option<String>,
}

impl Package {
    /// Compute local Cargo feature activation, including transitive features.
    /// Dependency feature selections do not become cfg features of this package.
    pub fn active_features(
        &self,
        requested: &[String],
        defaults: bool,
        all: bool,
    ) -> Result<BTreeSet<String>, String> {
        let mut pending = requested.to_vec();
        if defaults && self.features.contains_key("default") {
            pending.push("default".into());
        }
        if all {
            pending.extend(self.features.keys().cloned());
        }
        let mut active = BTreeSet::new();
        while let Some(name) = pending.pop() {
            let values = self
                .features
                .get(&name)
                .ok_or_else(|| format!("package `{}` has no feature `{name}`", self.name))?;
            if !active.insert(name) {
                continue;
            }
            for value in values {
                if !value.starts_with("dep:") && !value.contains('/') {
                    pending.push(value.clone());
                }
            }
        }
        Ok(active)
    }
}

impl Workspace {
    /// Validate provider names, consumer patterns, and declared dependency edges.
    /// Unlisted providers are public; self dependencies are always allowed.
    pub fn check_visibility(&self, policy: &Policy) -> Vec<String> {
        let members: BTreeSet<_> = self.packages.iter().map(|p| p.name.as_str()).collect();
        let mut errors = Vec::new();
        if let Err(error) = policy.validate() {
            errors.push(error);
        }
        for (provider, patterns) in &policy.visible_to {
            if !members.contains(provider.as_str()) {
                errors.push(format!(
                    "`{provider}` is not a workspace member; remove its visible_to"
                ));
            }
            for pattern in patterns {
                if !members
                    .iter()
                    .any(|member| policy::matches(pattern, member))
                {
                    errors.push(format!(
                        "`{provider}` visible_to `{pattern}` matches no workspace member"
                    ));
                }
            }
        }
        for edge in &self.edges {
            if !policy.visible(&edge.to, &edge.from) {
                let target = edge
                    .target
                    .as_ref()
                    .map_or(String::new(), |target| format!(" for `{target}`"));
                errors.push(format!(
                    "`{}` has a {} dependency{} on `{}`, which is visible only to {:?}",
                    edge.from, edge.kind, target, edge.to, policy.visible_to[&edge.to]
                ));
            }
        }
        errors
    }
}
