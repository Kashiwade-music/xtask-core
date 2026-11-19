//! Role: 解析に使うCargoの構成・cfg・信頼する展開の設定を検証して保持する。
use std::collections::BTreeSet;

use serde::Deserialize;

use crate::checks::{check_names, name_set};

/// Cargo features selected for analysis: every feature, or an explicit selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Features {
    All,
    Selected {
        features: BTreeSet<String>,
        default_features: bool,
    },
}

/// Repository-owned Cargo, cfg and expansion assumptions for module analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisSettings {
    target: Option<String>,
    features: Features,
    cfg: BTreeSet<String>,
    trusted_attributes: BTreeSet<String>,
    trusted_derives: BTreeSet<String>,
    trusted_macros: BTreeSet<String>,
    /// External glob imports, as `crate::module` paths, known to provide no macros, derives or
    /// attributes, such as a framework's prelude of types and traits. Other external globs make
    /// macro and derive providers unknowable.
    trusted_globs: BTreeSet<String>,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            target: None,
            features: Features::Selected {
                features: BTreeSet::new(),
                default_features: true,
            },
            cfg: BTreeSet::new(),
            trusted_attributes: BTreeSet::new(),
            trusted_derives: BTreeSet::new(),
            trusted_macros: BTreeSet::new(),
            trusted_globs: BTreeSet::new(),
        }
    }
}

impl AnalysisSettings {
    /// Validate analysis settings built in code. Uses the same checks as a policy file.
    pub fn new(
        target: Option<String>,
        features: Features,
        cfg: Vec<String>,
        trusted_attributes: Vec<String>,
        trusted_derives: Vec<String>,
        trusted_macros: Vec<String>,
        trusted_globs: Vec<String>,
    ) -> Result<Self, String> {
        if let Features::Selected { features, .. } = &features {
            check_names("analysis.features", features.iter().map(String::as_str))?;
        }
        Ok(Self {
            target,
            features,
            cfg: name_set("analysis.cfg", cfg)?,
            trusted_attributes: name_set("analysis.trusted_attributes", trusted_attributes)?,
            trusted_derives: name_set("analysis.trusted_derives", trusted_derives)?,
            trusted_macros: name_set("analysis.trusted_macros", trusted_macros)?,
            trusted_globs: name_set("analysis.trusted_globs", trusted_globs)?,
        })
    }

    /// Target triple whose cfg and dependencies are analyzed, or the host when absent.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn features(&self) -> &Features {
        &self.features
    }

    pub fn cfg(&self) -> &BTreeSet<String> {
        &self.cfg
    }

    pub fn trusted_attributes(&self) -> &BTreeSet<String> {
        &self.trusted_attributes
    }

    pub fn trusted_derives(&self) -> &BTreeSet<String> {
        &self.trusted_derives
    }

    pub fn trusted_macros(&self) -> &BTreeSet<String> {
        &self.trusted_macros
    }

    pub fn trusted_globs(&self) -> &BTreeSet<String> {
        &self.trusted_globs
    }
}

/// The `[analysis]` table as written. `into_settings` checks it.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RawAnalysis {
    target: Option<String>,
    features: Vec<String>,
    all_features: bool,
    default_features: Option<bool>,
    cfg: Vec<String>,
    trusted_attributes: Vec<String>,
    trusted_derives: Vec<String>,
    trusted_macros: Vec<String>,
    trusted_globs: Vec<String>,
}

impl RawAnalysis {
    pub(crate) fn into_settings(self) -> Result<AnalysisSettings, String> {
        let features = self.feature_selection()?;
        AnalysisSettings::new(
            self.target,
            features,
            self.cfg,
            self.trusted_attributes,
            self.trusted_derives,
            self.trusted_macros,
            self.trusted_globs,
        )
    }

    fn feature_selection(&self) -> Result<Features, String> {
        if self.all_features {
            if !self.features.is_empty() || self.default_features.is_some() {
                return Err(
                    "all_features cannot be combined with features or default_features".into(),
                );
            }
            return Ok(Features::All);
        }
        Ok(Features::Selected {
            features: name_set("analysis.features", self.features.clone())?,
            default_features: self.default_features.unwrap_or(true),
        })
    }
}
