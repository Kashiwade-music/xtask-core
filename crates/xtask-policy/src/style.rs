//! Role: Google Rust規則の選択、対象宣言と理由付き例外を検証して保持する。
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use crate::checks::name_set;

/// A versioned, mechanically checked subset of the Google Rust style guide.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum StyleRule {
    Format,
    HexLiterals,
    ImportGranularity,
    UnstableFeatures,
    Unwrap,
    CheckedUnwrap,
    FlexibleReferences,
    DerivableImpls,
    NewDefault,
    LenIsEmpty,
    MissingDocs,
    SafetyDocs,
    SafetyComments,
    GlobImports,
    NonLeafImplementation,
    RustfmtSkip,
    NonExhaustive,
    CatchUnwind,
    PoisonAccess,
    OptionsDefault,
}

impl StyleRule {
    /// The basic preset, selected by `[style] enabled = true`.
    pub const BASIC: [Self; 13] = [
        Self::Format,
        Self::HexLiterals,
        Self::ImportGranularity,
        Self::UnstableFeatures,
        Self::Unwrap,
        Self::CheckedUnwrap,
        Self::FlexibleReferences,
        Self::DerivableImpls,
        Self::NewDefault,
        Self::LenIsEmpty,
        Self::MissingDocs,
        Self::SafetyDocs,
        Self::SafetyComments,
    ];

    /// Stable rule identifiers used by policy files and diagnostics.
    pub const ALL: [Self; 20] = [
        Self::Format,
        Self::HexLiterals,
        Self::ImportGranularity,
        Self::UnstableFeatures,
        Self::Unwrap,
        Self::CheckedUnwrap,
        Self::FlexibleReferences,
        Self::DerivableImpls,
        Self::NewDefault,
        Self::LenIsEmpty,
        Self::MissingDocs,
        Self::SafetyDocs,
        Self::SafetyComments,
        Self::GlobImports,
        Self::NonLeafImplementation,
        Self::RustfmtSkip,
        Self::NonExhaustive,
        Self::CatchUnwind,
        Self::PoisonAccess,
        Self::OptionsDefault,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::HexLiterals => "hex-literals",
            Self::ImportGranularity => "import-granularity",
            Self::UnstableFeatures => "unstable-features",
            Self::Unwrap => "unwrap",
            Self::CheckedUnwrap => "checked-unwrap",
            Self::FlexibleReferences => "flexible-references",
            Self::DerivableImpls => "derivable-impls",
            Self::NewDefault => "new-default",
            Self::LenIsEmpty => "len-is-empty",
            Self::MissingDocs => "missing-docs",
            Self::SafetyDocs => "safety-docs",
            Self::SafetyComments => "safety-comments",
            Self::GlobImports => "glob-imports",
            Self::NonLeafImplementation => "non-leaf-implementation",
            Self::RustfmtSkip => "rustfmt-skip",
            Self::NonExhaustive => "non-exhaustive",
            Self::CatchUnwind => "catch-unwind",
            Self::PoisonAccess => "poison-access",
            Self::OptionsDefault => "options-default",
        }
    }
}

impl fmt::Display for StyleRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for StyleRule {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|rule| rule.as_str() == value)
            .ok_or_else(|| format!("unknown style rule `{value}`"))
    }
}

/// A narrow repository-owned exception, reviewed together with its reason.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StyleException {
    pub rule: StyleRule,
    /// Workspace-relative path, optionally ending in a prefix wildcard `*`.
    pub path: String,
    /// Optional exact item path within the source module hierarchy.
    pub item: Option<String>,
    pub reason: String,
}

impl StyleException {
    /// Match a normalized repository-relative file path and optional exact item.
    pub fn matches(&self, rule: StyleRule, path: &str, item: Option<&str>) -> bool {
        if self.rule != rule || self.item.as_deref().is_some_and(|name| Some(name) != item) {
            return false;
        }
        let pattern = self.path.replace('\\', "/");
        let path = path.replace('\\', "/");
        match pattern.strip_suffix('*') {
            Some(prefix) => path.starts_with(prefix),
            None => path == pattern,
        }
    }
}

/// Opt-in common style checks, present only when the policy enables them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSettings {
    rules: BTreeSet<StyleRule>,
    api_packages: BTreeSet<String>,
    options_structs: BTreeSet<String>,
    allowed_globs: BTreeSet<String>,
    toolchain: Option<String>,
    exceptions: Vec<StyleException>,
}

impl StyleSettings {
    /// The rules in effect: the basic preset, plus extra rules, minus disabled rules.
    pub fn rules(&self) -> &BTreeSet<StyleRule> {
        &self.rules
    }

    /// Workspace package names whose exported types require non_exhaustive.
    pub fn api_packages(&self) -> &BTreeSet<String> {
        &self.api_packages
    }

    /// Exact `package::module::Struct` selectors for options structures; root items use
    /// `package::Struct`.
    pub fn options_structs(&self) -> &BTreeSet<String> {
        &self.options_structs
    }

    /// Glob imports, as `crate::module` paths, the `glob-imports` rule accepts everywhere.
    pub fn allowed_globs(&self) -> &BTreeSet<String> {
        &self.allowed_globs
    }

    /// Optional rustup toolchain name. Components must be installed beforehand.
    pub fn toolchain(&self) -> Option<&str> {
        self.toolchain.as_deref()
    }

    pub fn exceptions(&self) -> &[StyleException] {
        &self.exceptions
    }
}

/// The `[style]` table as written. `into_settings` checks it, whether or not it is enabled.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RawStyle {
    enabled: bool,
    extra_rules: Vec<StyleRule>,
    disabled_rules: Vec<StyleRule>,
    exceptions: Vec<StyleException>,
    options_structs: Vec<String>,
    api_packages: Vec<String>,
    toolchain: Option<String>,
    allowed_globs: Vec<String>,
}

impl RawStyle {
    pub(crate) fn into_settings(self) -> Result<Option<StyleSettings>, String> {
        check_rule_lists(&self.extra_rules, &self.disabled_rules)?;
        check_toolchain(self.toolchain.as_deref())?;
        check_exceptions(&self.exceptions)?;
        let api_packages = name_set("style.api_packages", self.api_packages)?;
        if api_packages.iter().any(|name| !package_name(name)) {
            return Err("style.api_packages must contain exact package names".into());
        }
        if self.extra_rules.contains(&StyleRule::NonExhaustive) && api_packages.is_empty() {
            return Err("style rule `non-exhaustive` requires nonempty style.api_packages".into());
        }
        let options_structs = name_set("style.options_structs", self.options_structs)?;
        check_selectors(&options_structs)?;
        let allowed_globs = name_set("style.allowed_globs", self.allowed_globs)?;
        if !self.enabled {
            return Ok(None);
        }
        let rules = StyleRule::BASIC
            .into_iter()
            .chain(self.extra_rules.iter().copied())
            .filter(|rule| !self.disabled_rules.contains(rule))
            .collect();
        Ok(Some(StyleSettings {
            rules,
            api_packages,
            options_structs,
            allowed_globs,
            toolchain: self.toolchain,
            exceptions: self.exceptions,
        }))
    }
}

fn check_rule_lists(extra: &[StyleRule], disabled: &[StyleRule]) -> Result<(), String> {
    for (name, rules) in [("extra_rules", extra), ("disabled_rules", disabled)] {
        let distinct: BTreeSet<_> = rules.iter().collect();
        if distinct.len() != rules.len() {
            return Err(format!("style.{name} contains duplicate rules"));
        }
    }
    if let Some(rule) = extra.iter().find(|rule| disabled.contains(rule)) {
        return Err(format!("style rule `{rule}` is both extra and disabled"));
    }
    Ok(())
}

fn check_toolchain(toolchain: Option<&str>) -> Result<(), String> {
    if toolchain.is_some_and(|name| {
        name.is_empty()
            || name.starts_with(['-', '+'])
            || name.chars().any(char::is_whitespace)
            || name.contains(['/', '\\', '\0'])
    }) {
        return Err("style.toolchain must be a rustup toolchain name".into());
    }
    Ok(())
}

fn check_exceptions(exceptions: &[StyleException]) -> Result<(), String> {
    for exception in exceptions {
        if exception.reason.trim().is_empty() {
            return Err("style exception reason must not be blank".into());
        }
        let path = exception.path.replace('\\', "/");
        let prefix = path.strip_suffix('*').unwrap_or(&path);
        if path.is_empty()
            || path.starts_with('/')
            || prefix.is_empty()
            || prefix.contains(['*', '?', '[', ']', ':', '\0'])
            || prefix.split('/').any(|part| matches!(part, "." | ".."))
        {
            return Err(format!(
                "invalid style exception path `{}`; use a relative path with only an optional trailing `*`",
                exception.path
            ));
        }
        if exception
            .item
            .as_deref()
            .is_some_and(|item| !item_path(item))
        {
            return Err("style exception item must be an exact module::item path".into());
        }
    }
    Ok(())
}

fn check_selectors(selectors: &BTreeSet<String>) -> Result<(), String> {
    for selector in selectors {
        let Some((package, item)) = selector.split_once("::") else {
            return Err("style.options_structs requires package::module::Item selectors".into());
        };
        if !package_name(package) || !item_path(item) {
            return Err(format!(
                "invalid style.options_structs selector `{selector}`"
            ));
        }
    }
    Ok(())
}

fn package_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
}

fn item_path(value: &str) -> bool {
    !value.is_empty()
        && value.split("::").all(|part| {
            let part = part.strip_prefix("r#").unwrap_or(part);
            let mut characters = part.chars();
            characters
                .next()
                .is_some_and(|character| character.is_alphabetic() || character == '_')
                && characters.all(|character| character.is_alphanumeric() || character == '_')
        })
}
