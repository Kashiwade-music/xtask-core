//! Role: Share the temporary fixtures and the options of the module analysis tests.
use std::{collections::BTreeSet, fs, path::Path};
use xtask_modules::{Configuration, ModuleGraph, Settings, analyze};

/// The options of one test: the settings of the package and one configuration.
#[derive(Clone)]
pub struct Options {
    pub cfg: BTreeSet<String>,
    pub features: BTreeSet<String>,
    pub trusted_attributes: BTreeSet<String>,
    pub trusted_derives: BTreeSet<String>,
    pub trusted_macros: BTreeSet<String>,
    pub trusted_globs: BTreeSet<String>,
    pub max_role_chars: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cfg: BTreeSet::new(),
            features: BTreeSet::new(),
            trusted_attributes: BTreeSet::new(),
            trusted_derives: BTreeSet::new(),
            trusted_macros: BTreeSet::new(),
            trusted_globs: BTreeSet::new(),
            max_role_chars: 80,
        }
    }
}

impl Options {
    /// The package settings part of these options.
    pub fn settings(&self) -> Settings {
        Settings {
            features: self.features.clone(),
            trusted_attributes: self.trusted_attributes.clone(),
            trusted_derives: self.trusted_derives.clone(),
            trusted_macros: self.trusted_macros.clone(),
            trusted_globs: self.trusted_globs.clone(),
            max_role_chars: self.max_role_chars,
        }
    }

    /// Analyzes `root` under this configuration.
    pub fn analyze(&self, root: &Path) -> Result<ModuleGraph, String> {
        let configuration = Configuration {
            cfg: self.cfg.clone(),
        };
        analyze(root, &self.settings(), &[configuration])
            .pop()
            .expect("one configuration gives one result")
    }
}

/// Writes the files below a new temporary directory, verbatim.
pub fn write_fixture<S: AsRef<str>>(files: &[(&str, S)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, source) in files {
        let path = directory.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source.as_ref()).unwrap();
    }
    directory
}
