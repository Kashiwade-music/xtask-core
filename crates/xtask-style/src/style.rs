//! Role: スタイル検査を並列に実行し、理由付き例外を適用して診断と通知を報告する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use xtask_policy::{AnalysisSettings, StyleRule, StyleSettings};
use xtask_workspace::Workspace;

use crate::model::{StyleDiagnostic, StyleReport};
use crate::style_source::{self, ItemSpan, SourceReport, innermost};
use crate::style_tools;

/// The prepared inputs of one style check. The caller normalizes `root` and the
/// paths in `files` and `modules`, and removes duplicate files.
pub struct StyleInput<'a> {
    /// The repository root, normalized with `xtask_source::normalize`.
    pub root: &'a Path,
    pub workspace: &'a Workspace,
    pub analysis: &'a AnalysisSettings,
    /// The features that Cargo selects for each member (`Workspace::features`). An
    /// `Err` is reported once by the Default probe; the Clippy run does not depend on it.
    pub features: &'a Result<BTreeMap<String, BTreeSet<String>>, String>,
    pub style: &'a StyleSettings,
    /// The Rust files to check, normalized and without duplicates.
    pub files: &'a [PathBuf],
    /// File to the module path of its root module (`crate::a::b`), used to name items.
    pub modules: &'a BTreeMap<PathBuf, String>,
}

/// Whether the style settings enable the rule.
pub(crate) fn enabled(style: &StyleSettings, rule: StyleRule) -> bool {
    style.rules().contains(&rule)
}

/// Run every style check and report what is not excepted. A check that cannot run
/// is an error; a violation is a diagnostic. Exceptions move matched violations
/// into notes, and an exception that matches nothing is noted as unused.
pub fn check(input: &StyleInput<'_>) -> StyleReport {
    let mut report = StyleReport::default();
    api_package_errors(input, &mut report);
    let (source, tools) = run_checks(input);
    report.errors.extend(
        source
            .errors
            .iter()
            .map(|error| format!("style source check: {error}")),
    );
    report.errors.extend(tools.errors);
    report.notes.extend(tools.notes);
    let mut diagnostics = source.diagnostics;
    diagnostics.extend(tools.diagnostics);
    // Paths are normalized before sorting, so one file has one spelling in the order.
    for diagnostic in &mut diagnostics {
        diagnostic.path = normalized(input.root, &diagnostic.path);
    }
    diagnostics.sort();
    diagnostics.dedup();
    let resolved = resolve(input, &source.items, diagnostics);
    apply_exceptions(input, resolved, &mut report);
    report.notes.push(format!(
        "checked {} style rules; source checks include inactive source syntax, compiler checks cover the configured Cargo target/features",
        input.style.rules().len()
    ));
    report
}

/// The source check and the external tools are independent, so they run at once.
fn run_checks(input: &StyleInput<'_>) -> (SourceReport, StyleReport) {
    std::thread::scope(|scope| {
        let source = scope.spawn(|| style_source::check(input.files, input.style));
        let tools = style_tools::check(input);
        let source = source.join().expect("style source check thread panicked");
        (source, tools)
    })
}

fn api_package_errors(input: &StyleInput<'_>, report: &mut StyleReport) {
    if !enabled(input.style, StyleRule::NonExhaustive) {
        return;
    }
    for name in input.style.api_packages() {
        let library = input
            .workspace
            .packages
            .iter()
            .find(|package| &package.name == name)
            .is_some_and(|package| {
                package
                    .targets
                    .iter()
                    .any(|target| target.kinds.iter().any(|kind| kind.is_library()))
            });
        if !library {
            report.errors.push(format!(
                "style api_packages `{name}` must name a workspace package with a library target"
            ));
        }
    }
}

/// A diagnostic path made absolute against the root, then normalized. A path that
/// cannot be normalized, such as one that does not exist, is kept as it is.
fn normalized(root: &Path, path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    xtask_source::normalize(&absolute).unwrap_or(absolute)
}

/// Give each diagnostic the package-free item name that a policy exception uses,
/// and, when the check named no item, the innermost item at its position.
fn resolve(
    input: &StyleInput<'_>,
    items: &BTreeMap<PathBuf, Vec<ItemSpan>>,
    diagnostics: Vec<StyleDiagnostic>,
) -> Vec<StyleDiagnostic> {
    diagnostics
        .into_iter()
        .map(|mut diagnostic| {
            strip_package(input, &mut diagnostic);
            if diagnostic.item.is_none() {
                diagnostic.item = item_at(input, items, &diagnostic);
            }
            diagnostic
        })
        .collect()
}

/// An explicit selector names its package, while policy exceptions name the item
/// within its module hierarchy.
fn strip_package(input: &StyleInput<'_>, diagnostic: &mut StyleDiagnostic) {
    let Some(item) = &diagnostic.item else {
        return;
    };
    let Some((package, local)) = item.split_once("::") else {
        return;
    };
    if input
        .workspace
        .packages
        .iter()
        .any(|candidate| candidate.name == package)
    {
        diagnostic.item = Some(local.to_owned());
    }
}

fn item_at(
    input: &StyleInput<'_>,
    items: &BTreeMap<PathBuf, Vec<ItemSpan>>,
    diagnostic: &StyleDiagnostic,
) -> Option<String> {
    let spans = items.get(&diagnostic.path)?;
    let prefix = input
        .modules
        .get(&diagnostic.path)
        .map_or("", |module| module_prefix(module));
    innermost(spans, diagnostic.line, diagnostic.column).map(|span| {
        if prefix.is_empty() {
            span.item.clone()
        } else {
            format!("{prefix}::{}", span.item)
        }
    })
}

/// The item-name prefix of a file's root module: `crate::a::b` names `a::b`, and
/// the crate root names nothing.
fn module_prefix(module: &str) -> &str {
    match module.strip_prefix("crate::") {
        Some(rest) => rest,
        None if module == "crate" => "",
        None => module,
    }
}

fn apply_exceptions(
    input: &StyleInput<'_>,
    diagnostics: Vec<StyleDiagnostic>,
    report: &mut StyleReport,
) {
    let mut used = BTreeSet::new();
    for diagnostic in diagnostics {
        let relative = relative_path(input.root, &diagnostic.path);
        let matched = input
            .style
            .exceptions()
            .iter()
            .enumerate()
            .find(|(_, exception)| {
                exception.matches(diagnostic.rule, &relative, diagnostic.item.as_deref())
            });
        match matched {
            Some((index, exception)) => {
                used.insert(index);
                report.notes.push(format!(
                    "style exception [{}] {}{}: {}",
                    diagnostic.rule,
                    relative,
                    item_suffix(diagnostic.item.as_deref()),
                    exception.reason
                ));
            }
            None => report.diagnostics.push(diagnostic),
        }
    }
    for (index, exception) in input.style.exceptions().iter().enumerate() {
        if !used.contains(&index) {
            report.notes.push(format!(
                "unused style exception [{}] {}{}",
                exception.rule,
                exception.path,
                item_suffix(exception.item.as_deref())
            ));
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn item_suffix(item: Option<&str>) -> String {
    item.map_or(String::new(), |item| format!("::{item}"))
}

#[cfg(test)]
mod tests {
    //! Role: ソース位置から例外対象の正確な項目名を解決する。
    use crate::style_source::index_items;

    #[test]
    fn nested_items_and_methods_have_exact_paths() {
        let parsed = syn::parse_file("mod api {\nstruct Options;\nimpl Options {\nfn build() {}\n}\ntrait Ext { fn run(); }\n}").unwrap();
        let mut items = Vec::new();
        index_items(&parsed.items, "", &mut items);
        assert!(
            items
                .iter()
                .any(|item| item.item == "api::Options::build" && item.start_line == 4)
        );
        assert!(items.iter().any(|item| item.item == "api::Ext::run"));
    }

    #[test]
    fn item_exception_does_not_expand_to_a_sibling_on_the_same_line() {
        let text = "fn approved() {} fn unapproved() {}";
        let parsed = syn::parse_file(text).unwrap();
        let mut items = Vec::new();
        index_items(&parsed.items, "", &mut items);
        let column = text.find("unapproved").unwrap() + 1;
        let matching: Vec<_> = items
            .iter()
            .filter(|item| item.contains(1, column))
            .map(|item| item.item.as_str())
            .collect();
        assert_eq!(matching, ["unapproved"]);
    }
}
