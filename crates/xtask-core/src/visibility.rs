//! Role: 公開の依存の可視性（visible_to）を、Cargo の依存の辺と照らして検証する。
use std::collections::BTreeMap;

use xtask_workspace::{Dependency, DependencyKind, Workspace};

/// Check `visible_to` against the workspace: each provider must be a member, each pattern must
/// match a member, and every declared dependency edge must be visible to its consumer.
pub(crate) fn visibility(
    workspace: &Workspace,
    visible_to: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let members: Vec<&str> = workspace
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut errors = Vec::new();
    for (provider, patterns) in visible_to {
        if !members.contains(&provider.as_str()) {
            errors.push(format!(
                "`{provider}` is not a workspace member; remove its visible_to"
            ));
        }
        for pattern in patterns {
            if !members.iter().any(|member| matches(pattern, member)) {
                errors.push(format!(
                    "`{provider}` visible_to `{pattern}` matches no workspace member"
                ));
            }
        }
    }
    for edge in &workspace.edges {
        if let Some(error) = invisible_edge(visible_to, edge) {
            errors.push(error);
        }
    }
    errors
}

/// The message for a declared edge whose consumer the provider does not list.
fn invisible_edge(visible_to: &BTreeMap<String, Vec<String>>, edge: &Dependency) -> Option<String> {
    let visible = visible_to.get(&edge.to).is_none_or(|patterns| {
        edge.to == edge.from || patterns.iter().any(|pattern| matches(pattern, &edge.from))
    });
    if visible {
        return None;
    }
    let target = edge
        .target
        .as_ref()
        .map_or(String::new(), |target| format!(" for `{target}`"));
    Some(format!(
        "`{}` has a {} dependency{} on `{}`, which is visible only to {:?}",
        edge.from,
        kind_name(edge.kind),
        target,
        edge.to,
        visible_to[&edge.to]
    ))
}

fn kind_name(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Normal => "normal",
        DependencyKind::Dev => "dev",
        DependencyKind::Build => "build",
    }
}

/// A pattern is a package name, or a prefix followed by a trailing `*`.
fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

#[cfg(test)]
mod tests {
    //! Role: 可視性の検査が古い provider・存在しない consumer・全部の宣言された辺を報告することを確かめる。
    use std::fs;

    use xtask_policy::Policy;
    use xtask_workspace::Workspace;

    use super::visibility;

    fn write_fixture(root: &std::path::Path) {
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers=['core', 'app']\nresolver='3'\n",
        )
        .unwrap();
        for package in ["core", "app"] {
            fs::create_dir_all(root.join(package).join("src")).unwrap();
            fs::write(root.join(package).join("src/lib.rs"), "").unwrap();
            fs::write(
                root.join(package).join("Cargo.toml"),
                format!("[package]\nname='{package}'\nversion='0.1.0'\nedition='2024'\n"),
            )
            .unwrap();
        }
    }

    #[test]
    fn visibility_checks_stale_providers_consumers_and_all_declared_edges() {
        let directory = tempfile::tempdir().unwrap();
        write_fixture(directory.path());
        fs::write(
            directory.path().join("app/Cargo.toml"),
            concat!(
                "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
                "[build-dependencies]\ncore={path='../core'}\n"
            ),
        )
        .unwrap();
        let status =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["generate-lockfile", "--offline"])
                .current_dir(directory.path())
                .status()
                .unwrap();
        assert!(status.success());
        let workspace = Workspace::load(directory.path()).unwrap();
        let policy = Policy::from_toml("[visible_to]\ncore=['other*']\ngone=[]\n").unwrap();
        let errors = visibility(&workspace, policy.visible_to());
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
}
