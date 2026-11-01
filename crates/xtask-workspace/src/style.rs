//! Role: Google Rust規則の選択、対象宣言と理由付き例外を定義・検証する。
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

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

/// Opt-in common style checks. The first thirteen rules form the basic preset.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct StylePolicy {
    pub enabled: bool,
    pub extra_rules: Vec<StyleRule>,
    pub disabled_rules: Vec<StyleRule>,
    pub exceptions: Vec<StyleException>,
    /// Exact `package::module::Struct` selectors for options structures; root
    /// items use `package::Struct`.
    pub options_structs: Vec<String>,
    /// Workspace package names whose exported types require non_exhaustive.
    pub api_packages: Vec<String>,
    /// Optional rustup toolchain name. Components must be installed beforehand.
    pub toolchain: Option<String>,
}

impl StylePolicy {
    pub fn rules(&self) -> BTreeSet<StyleRule> {
        if !self.enabled {
            return BTreeSet::new();
        }
        let mut rules: BTreeSet<_> = StyleRule::ALL[..13].iter().copied().collect();
        rules.extend(self.extra_rules.iter().copied());
        for rule in &self.disabled_rules {
            rules.remove(rule);
        }
        rules
    }

    pub fn is_enabled(&self, rule: StyleRule) -> bool {
        self.enabled
            && !self.disabled_rules.contains(&rule)
            && (StyleRule::ALL[..13].contains(&rule) || self.extra_rules.contains(&rule))
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, rules) in [
            ("extra_rules", &self.extra_rules),
            ("disabled_rules", &self.disabled_rules),
        ] {
            let distinct: BTreeSet<_> = rules.iter().collect();
            if distinct.len() != rules.len() {
                return Err(format!("style.{name} contains duplicate rules"));
            }
        }
        if let Some(rule) = self
            .extra_rules
            .iter()
            .find(|rule| self.disabled_rules.contains(rule))
        {
            return Err(format!("style rule `{rule}` is both extra and disabled"));
        }
        if self.toolchain.as_deref().is_some_and(|name| {
            name.is_empty()
                || name.starts_with(['-', '+'])
                || name.chars().any(char::is_whitespace)
                || name.contains(['/', '\\', '\0'])
        }) {
            return Err("style.toolchain must be a rustup toolchain name".into());
        }
        for exception in &self.exceptions {
            if exception.reason.trim().is_empty() {
                return Err("style exception reason must not be blank".into());
            }
            let path = exception.path.replace('\\', "/");
            let prefix = path.strip_suffix('*').unwrap_or(&path);
            if path.is_empty()
                || path.starts_with('/')
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
        let distinct: BTreeSet<_> = self.options_structs.iter().collect();
        if distinct.len() != self.options_structs.len() {
            return Err("style.options_structs contains duplicate selectors".into());
        }
        for selector in &self.options_structs {
            let Some((package, item)) = selector.split_once("::") else {
                return Err(
                    "style.options_structs requires package::module::Item selectors".into(),
                );
            };
            if !package_name(package) || !item_path(item) {
                return Err(format!(
                    "invalid style.options_structs selector `{selector}`"
                ));
            }
        }
        if self.api_packages.iter().any(|name| !package_name(name)) {
            return Err("style.api_packages must contain exact package names".into());
        }
        if self.api_packages.iter().collect::<BTreeSet<_>>().len() != self.api_packages.len() {
            return Err("style.api_packages contains duplicate package names".into());
        }
        for (rule, populated, field) in [
            (
                StyleRule::NonExhaustive,
                !self.api_packages.is_empty(),
                "api_packages",
            ),
            (
                StyleRule::OptionsDefault,
                !self.options_structs.is_empty(),
                "options_structs",
            ),
        ] {
            if self.is_enabled(rule) && !populated {
                return Err(format!(
                    "style rule `{rule}` requires nonempty style.{field}"
                ));
            }
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    //! Role: 規則選択、対象宣言と理由付き例外の妥当性を検証する。
    use super::{StyleException, StylePolicy, StyleRule};
    use crate::Policy;

    #[test]
    fn old_policies_remain_disabled_and_opt_in_selects_thirteen_rules() {
        assert!(Policy::from_toml("").unwrap().style.rules().is_empty());
        let policy = Policy::from_toml("[style]\nenabled=true\n").unwrap();
        assert_eq!(policy.style.rules().len(), 13);
        assert!(policy.style.is_enabled(StyleRule::Unwrap));
        assert!(!policy.style.is_enabled(StyleRule::GlobImports));
        for rule in StyleRule::ALL {
            assert_eq!(rule.as_str().parse::<StyleRule>().unwrap(), rule);
        }
    }

    #[test]
    fn all_twenty_rules_can_be_selected_with_explicit_api_targets() {
        let policy = Policy::from_toml(concat!(
            "[style]\nenabled=true\n",
            "extra_rules=['glob-imports','non-leaf-implementation','rustfmt-skip',",
            "'non-exhaustive','catch-unwind','poison-access','options-default']\n",
            "api_packages=['my-api']\n",
            "options_structs=['my-api::Options']\ntoolchain='1.85.0'\n"
        ))
        .unwrap();
        assert_eq!(policy.style.rules().len(), 20);
        let policy =
            Policy::from_toml("[style]\nenabled=true\ndisabled_rules=['format']\n").unwrap();
        assert_eq!(policy.style.rules().len(), 12);
    }

    #[test]
    fn invalid_policy_fields_rules_targets_and_exceptions_are_rejected() {
        for setting in [
            "unknown=true",
            "extra_rules=['typo']",
            "extra_rules=['glob-imports','glob-imports']",
            "extra_rules=['glob-imports']\ndisabled_rules=['glob-imports']",
            "extra_rules=['non-exhaustive']",
            "extra_rules=['extension-trait-names']",
            "extra_rules=['options-default']",
            "extension_traits=['my-api::ReadExt']",
            "options_structs=['Options']",
            "options_structs=['api::bad::']",
            "toolchain='--help'",
            "toolchain='../toolchain'",
            "toolchain=''",
            "api_packages=['*']",
        ] {
            let text = format!("[style]\nenabled=true\n{setting}\n");
            assert!(Policy::from_toml(&text).is_err(), "accepted {text}");
        }
        assert!(Policy::from_toml("style=[]\n").is_err());
        for path in [
            "",
            "/src/lib.rs",
            "../src/lib.rs",
            "src/../lib.rs",
            "C:/src/lib.rs",
            "src/*.rs",
            "src/**",
        ] {
            let text = format!(
                "[style]\n[[style.exceptions]]\nrule='unwrap'\npath='{path}'\nreason='reviewed'\n"
            );
            assert!(Policy::from_toml(&text).is_err(), "accepted {text}");
        }
        assert!(
            Policy::from_toml(
                "[[style.exceptions]]\nrule='unwrap'\npath='src/lib.rs'\nreason='  '\n"
            )
            .is_err()
        );
    }

    #[test]
    fn exceptions_match_exact_rules_paths_and_items_without_broadening() {
        let exception = StyleException {
            rule: StyleRule::GlobImports,
            path: "crates/api/src/*".into(),
            item: Some("io::tests".into()),
            reason: "test fixtures intentionally import their parent".into(),
        };
        assert!(exception.matches(
            StyleRule::GlobImports,
            "crates\\api\\src\\io.rs",
            Some("io::tests")
        ));
        assert!(!exception.matches(
            StyleRule::GlobImports,
            "crates/api2/src/io.rs",
            Some("io::tests")
        ));
        assert!(!exception.matches(StyleRule::GlobImports, "crates/api/src/io.rs", Some("io")));
        assert!(!exception.matches(StyleRule::Unwrap, "crates/api/src/io.rs", Some("io::tests")));
        assert!(!exception.matches(StyleRule::GlobImports, "crates/api/src/io.rs", None));
        StylePolicy {
            exceptions: vec![exception],
            ..StylePolicy::default()
        }
        .validate()
        .unwrap();
    }
}
