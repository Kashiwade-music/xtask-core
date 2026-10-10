use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// Repository-owned settings for the adopted common rules.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub visible_to: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default = "default_max_code_lines")]
    pub max_code_lines: usize,
}

const fn default_max_code_lines() -> usize {
    1000
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_code_lines: default_max_code_lines(),
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

    /// Restrict the common limit further, but never relax the 1000-line rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.limits.max_code_lines > 1000 {
            return Err("limits.max_code_lines must be at most 1000".into());
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
