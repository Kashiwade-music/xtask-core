//! Role: SVG出力先、解析条件、コード行数・役割説明の上限と可視性設定を読み込み検証する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Repository-owned settings for the adopted common rules.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// SVG destination, relative to the repository root unless absolute.
    #[serde(default = "default_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default, deserialize_with = "analysis_settings")]
    pub analysis: AnalysisSettings,
    #[serde(default)]
    pub visible_to: BTreeMap<String, Vec<String>>,
}

fn default_output() -> PathBuf {
    PathBuf::from("architecture")
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            output: default_output(),
            limits: Limits::default(),
            analysis: AnalysisSettings::default(),
            visible_to: BTreeMap::new(),
        }
    }
}

/// Repository-owned Cargo, cfg and expansion assumptions for module analysis.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisSettings {
    pub target: Option<String>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub default_features: bool,
    pub cfg: Vec<String>,
    pub trusted_attributes: BTreeSet<String>,
    pub trusted_derives: BTreeSet<String>,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            target: None,
            features: Vec::new(),
            all_features: false,
            default_features: true,
            cfg: Vec::new(),
            trusted_attributes: BTreeSet::new(),
            trusted_derives: BTreeSet::new(),
        }
    }
}

fn analysis_settings<'de, D>(deserializer: D) -> Result<AnalysisSettings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Serde's defaulted struct visitor also accepts an empty sequence. Policy
    // settings must be a TOML table rather than silently accepting `[]`.
    let table = toml::Table::deserialize(deserializer)?;
    toml::Value::Table(table)
        .try_into()
        .map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default = "default_max_code_lines")]
    pub max_code_lines: usize,
    /// Maximum Unicode scalar values in a module's Role description.
    #[serde(default = "default_max_role_chars")]
    pub max_role_chars: usize,
}

const fn default_max_code_lines() -> usize {
    1000
}

const fn default_max_role_chars() -> usize {
    80
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_code_lines: default_max_code_lines(),
            max_role_chars: default_max_role_chars(),
        }
    }
}

impl Policy {
    /// Read a policy file. Missing files and invalid fields are errors.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::from_toml(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn from_toml(text: &str) -> Result<Self, String> {
        let policy: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        policy.validate()?;
        Ok(policy)
    }

    /// Validate the output path, configurable limits and visibility patterns. The common
    /// 1000-line ceiling can be tightened; Role limits may be any positive value.
    pub fn validate(&self) -> Result<(), String> {
        if self.output.as_os_str().to_string_lossy().trim().is_empty() {
            return Err("output must be a nonempty path".into());
        }
        if self.limits.max_code_lines > 1000 {
            return Err("limits.max_code_lines must be at most 1000".into());
        }
        if self.limits.max_role_chars == 0 {
            return Err("limits.max_role_chars must be at least 1".into());
        }
        for (provider, consumers) in &self.visible_to {
            if provider.is_empty() || provider.contains('*') {
                return Err(format!("invalid visible_to provider `{provider}`"));
            }
            for pattern in consumers {
                let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
                if pattern.is_empty() || prefix.contains('*') {
                    return Err(format!(
                        "invalid visible_to pattern `{pattern}` for `{provider}`; only a trailing `*` is supported"
                    ));
                }
            }
        }
        Ok(())
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

pub(crate) fn matches(pattern: &str, consumer: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => consumer.starts_with(prefix),
        None => consumer == pattern,
    }
}
