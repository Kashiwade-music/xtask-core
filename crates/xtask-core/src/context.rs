//! Role: 検査の入力（policy、Cargo の構造、解析の設定、rustc の cfg）を読み、段階の間で共有する値を作る。
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use xtask_archive::Mode;
use xtask_modules::Configuration;
use xtask_policy::{AnalysisSettings, Policy};
use xtask_source::normalize;
use xtask_workspace::{TargetKind, Workspace};

use crate::config::{Config, Output};

/// The inputs that every stage reads. A failure while loading it stops the run.
pub(crate) struct Context {
    pub(crate) workspace: Workspace,
    pub(crate) policy: Policy,
    /// The analysis settings in effect: the API override, or else the policy's `[analysis]`.
    pub(crate) settings: AnalysisSettings,
    pub(crate) output: Output,
}

impl Context {
    pub(crate) fn load(config: &Config) -> Result<Self, String> {
        let requested = normalize(&config.root)?;
        let workspace = Workspace::load(&config.root)?;
        if requested != workspace.root {
            return Err(format!(
                "--root must name the workspace root ({})",
                workspace.root.display()
            ));
        }
        let policy = load_policy(&config.policy)?;
        let settings = config
            .analysis
            .clone()
            .unwrap_or_else(|| policy.analysis().clone());
        Ok(Self {
            workspace,
            policy,
            settings,
            output: config.output.clone(),
        })
    }
}

/// A missing policy file means the default policy; a file that exists must be valid.
fn load_policy(path: &Path) -> Result<Policy, String> {
    let exists = path
        .try_exists()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if exists {
        Policy::load(path)
    } else {
        Ok(Policy::default())
    }
}

/// The features that Cargo selects for each workspace member, read once for the whole run.
pub(crate) type FeatureTable = BTreeMap<String, BTreeSet<String>>;

impl Context {
    /// The features of each member under the analysis settings. Every member must have an entry.
    pub(crate) fn features(&self) -> Result<FeatureTable, String> {
        let features = self.workspace.features(&self.settings)?;
        if let Some(package) = self
            .workspace
            .packages
            .iter()
            .find(|package| !features.contains_key(&package.name))
        {
            return Err(format!(
                "Cargo feature resolution omitted workspace member `{}`",
                package.name
            ));
        }
        Ok(features)
    }
}

/// The cfg of the target and of the host, read once before any job starts.
pub(crate) struct Environment {
    target_cfg: BTreeSet<String>,
    host_cfg: BTreeSet<String>,
}

impl Environment {
    pub(crate) fn load(context: &Context) -> Result<Self, String> {
        let root = &context.workspace.root;
        let extra = context.settings.cfg();
        let target = context.settings.target();
        let target_cfg = rustc_cfg(root, target, extra)?;
        // Build scripts and procedural macros run on the host, which is the target without
        // `[analysis] target`.
        let host_cfg = match target {
            Some(_) => rustc_cfg(root, None, extra)?,
            None => target_cfg.clone(),
        };
        Ok(Self {
            target_cfg,
            host_cfg,
        })
    }

    /// The configuration of one analysis of a target: its cfg, plus `proc_macro` and `test`
    /// where they apply.
    pub(crate) fn configuration(&self, kinds: &[TargetKind], mode: Mode) -> Configuration {
        let procedural = kinds.contains(&TargetKind::ProcMacro);
        let host = procedural || kinds.contains(&TargetKind::CustomBuild);
        let mut cfg = if host {
            self.host_cfg.clone()
        } else {
            self.target_cfg.clone()
        };
        if procedural {
            cfg.insert("proc_macro".into());
        }
        if mode == Mode::Test {
            cfg.insert("test".into());
        }
        Configuration { cfg }
    }
}

fn rustc_cfg(
    root: &Path,
    target: Option<&str>,
    extra: &BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let mut command = Command::new(rustc);
    command.args(["--print", "cfg"]).current_dir(root);
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not read rustc cfg: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustc --print cfg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|error| format!("rustc cfg is not UTF-8: {error}"))?;
    let mut cfg: BTreeSet<String> = text.lines().map(str::to_owned).collect();
    cfg.extend(extra.iter().cloned());
    Ok(cfg)
}
