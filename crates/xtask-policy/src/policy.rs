//! Role: policy全体を読み、検証済みの節（limits、diagrams、visible_to）を保持する。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::analysis::{AnalysisSettings, RawAnalysis};
use crate::checks::check_names;
use crate::style::{RawStyle, StyleSettings};

/// Repository-owned settings for the adopted common rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    limits: Limits,
    diagrams: Diagrams,
    analysis: AnalysisSettings,
    style: Option<StyleSettings>,
    visible_to: BTreeMap<String, Vec<String>>,
}

/// Size limits for module code and Role descriptions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    max_code_lines: usize,
    max_role_chars: usize,
}

/// Which cargo targets get module diagrams, and where the diagram file is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagrams {
    targets: DiagramTargets,
    output: PathBuf,
}

/// Selection of the cargo targets whose module diagrams are generated.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DiagramTargets {
    /// Every target: libraries, binaries, tests, benches, examples and build scripts.
    #[default]
    All,
    /// Library-like targets only (`lib`, `rlib`, `staticlib`, `cdylib`, `dylib`, `proc-macro`).
    Lib,
    /// Library-like targets and `bin` targets.
    LibBin,
}

const DEFAULT_MAX_CODE_LINES: usize = 1000;
const CODE_LINE_CEILING: usize = 1000;
const DEFAULT_MAX_ROLE_CHARS: usize = 80;
const DEFAULT_OUTPUT: &str = "target/xtask/architecture.xtaskmap";
const SECTIONS: [&str; 5] = ["limits", "diagrams", "analysis", "style", "visible_to"];

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_code_lines: DEFAULT_MAX_CODE_LINES,
            max_role_chars: DEFAULT_MAX_ROLE_CHARS,
        }
    }
}

impl Limits {
    /// Maximum physical code lines in one source file.
    pub fn max_code_lines(&self) -> usize {
        self.max_code_lines
    }

    /// Maximum Unicode scalar values in a module's Role description.
    pub fn max_role_chars(&self) -> usize {
        self.max_role_chars
    }
}

impl Default for Diagrams {
    fn default() -> Self {
        Self {
            targets: DiagramTargets::default(),
            output: PathBuf::from(DEFAULT_OUTPUT),
        }
    }
}

impl Diagrams {
    pub fn targets(&self) -> DiagramTargets {
        self.targets
    }

    /// Diagram file destination, relative to the repository root unless absolute.
    pub fn output(&self) -> &Path {
        &self.output
    }
}

impl Policy {
    /// Read a policy file. Missing files and invalid fields are errors.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::from_toml(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Parse and validate policy text. Every error is reported as the first one found.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let table: toml::Table = text
            .parse()
            .map_err(|error: toml::de::Error| error.to_string())?;
        if table.contains_key("output") {
            return Err("unknown field `output`: output moved to [diagrams] output".into());
        }
        for section in SECTIONS {
            if table.get(section).is_some_and(|value| !value.is_table()) {
                return Err(format!("[{section}] must be a table"));
            }
        }
        let raw: RawPolicy = toml::Value::Table(table)
            .try_into()
            .map_err(|error: toml::de::Error| error.to_string())?;
        raw.into_policy()
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn diagrams(&self) -> &Diagrams {
        &self.diagrams
    }

    pub fn analysis(&self) -> &AnalysisSettings {
        &self.analysis
    }

    /// Present only when `[style] enabled = true`.
    pub fn style(&self) -> Option<&StyleSettings> {
        self.style.as_ref()
    }

    pub fn visible_to(&self) -> &BTreeMap<String, Vec<String>> {
        &self.visible_to
    }

    /// Omitted providers are public; an empty list makes a provider private.
    pub fn visible(&self, provider: &str, consumer: &str) -> bool {
        provider == consumer
            || self
                .visible_to
                .get(provider)
                .is_none_or(|patterns| patterns.iter().any(|pattern| matches(pattern, consumer)))
    }
}

fn matches(pattern: &str, consumer: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => consumer.starts_with(prefix),
        None => consumer == pattern,
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawPolicy {
    limits: RawLimits,
    diagrams: RawDiagrams,
    analysis: RawAnalysis,
    style: RawStyle,
    visible_to: BTreeMap<String, Vec<String>>,
}

impl RawPolicy {
    fn into_policy(self) -> Result<Policy, String> {
        Ok(Policy {
            limits: self.limits.into_limits()?,
            diagrams: self.diagrams.into_diagrams()?,
            analysis: self.analysis.into_settings()?,
            style: self.style.into_settings()?,
            visible_to: visible_to(self.visible_to)?,
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawLimits {
    max_code_lines: Option<usize>,
    max_role_chars: Option<usize>,
}

impl RawLimits {
    fn into_limits(self) -> Result<Limits, String> {
        let max_code_lines = self.max_code_lines.unwrap_or(DEFAULT_MAX_CODE_LINES);
        if max_code_lines == 0 {
            return Err("limits.max_code_lines must be at least 1".into());
        }
        if max_code_lines > CODE_LINE_CEILING {
            return Err("limits.max_code_lines must be at most 1000".into());
        }
        let max_role_chars = self.max_role_chars.unwrap_or(DEFAULT_MAX_ROLE_CHARS);
        if max_role_chars == 0 {
            return Err("limits.max_role_chars must be at least 1".into());
        }
        Ok(Limits {
            max_code_lines,
            max_role_chars,
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawDiagrams {
    targets: DiagramTargets,
    output: Option<String>,
}

impl RawDiagrams {
    fn into_diagrams(self) -> Result<Diagrams, String> {
        let output = self.output.unwrap_or_else(|| DEFAULT_OUTPUT.into());
        if output.trim().is_empty() {
            return Err("diagrams.output must be a nonempty path".into());
        }
        Ok(Diagrams {
            targets: self.targets,
            output: PathBuf::from(output),
        })
    }
}

fn visible_to(raw: BTreeMap<String, Vec<String>>) -> Result<BTreeMap<String, Vec<String>>, String> {
    raw.into_iter()
        .map(|(provider, patterns)| {
            if provider.is_empty() || provider.contains('*') {
                return Err(format!("invalid visible_to provider `{provider}`"));
            }
            for pattern in &patterns {
                let prefix = pattern.strip_suffix('*').unwrap_or(pattern.as_str());
                if prefix.contains('*') {
                    return Err(format!(
                        "invalid visible_to pattern `{pattern}` for `{provider}`; only a trailing `*` is supported"
                    ));
                }
            }
            check_names(
                &format!("visible_to.{provider}"),
                patterns.iter().map(String::as_str),
            )?;
            Ok((provider, patterns))
        })
        .collect()
}
