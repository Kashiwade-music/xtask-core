//! Role: 今回生成した図を埋め込み、単体で開ける対話型アーキテクチャHTMLを出力する。
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use xtask_workspace::DiagramTargets;

use crate::{Analysis, Config, graphs};

pub(crate) fn write(
    config: &Config,
    analysis: &mut Analysis,
    selection: DiagramTargets,
) -> Result<(), String> {
    if config.no_write {
        return Ok(());
    }
    // Only use artifacts produced successfully by this invocation. Preserved
    // SVGs for failed or feature-skipped targets must not masquerade as current.
    let generated: BTreeSet<_> = analysis.report.artifacts.iter().collect();
    let read = |path: &PathBuf| -> Result<Option<String>, String> {
        if !generated.contains(path) {
            return Ok(None);
        }
        fs::read_to_string(path)
            .map(Some)
            .map_err(|error| format!("cannot embed {}: {error}", path.display()))
    };
    let crates = read(&config.output_dir().join("crates.svg"))?;
    let mut modules = Vec::new();
    for target in analysis
        .targets
        .iter()
        .filter(|target| selection.includes(target.kind.split('+')))
    {
        let path = graphs::module_svg_path(
            config,
            &target.package,
            &target.target,
            &target.kind,
            target.test,
        );
        if let Some(svg) = read(&path)? {
            modules.push(json!({
                "package": target.package,
                "target": target.target,
                "kind": target.kind,
                "test": target.test,
                "svg": svg,
            }));
        }
    }
    modules.sort_by_key(|module| {
        (
            module["package"].as_str().unwrap_or_default().to_owned(),
            module["target"].as_str().unwrap_or_default().to_owned(),
            module["kind"].as_str().unwrap_or_default().to_owned(),
            module["test"].as_bool().unwrap_or_default(),
        )
    });
    let mut notices = analysis.report.errors.clone();
    notices.extend(
        analysis
            .skipped
            .iter()
            .filter(|target| selection.includes(target.kind.split('+')))
            .map(|target| {
                format!(
                    "{}::{} ({}): analysis skipped because required-features are inactive.",
                    target.package, target.target, target.kind
                )
            }),
    );
    // Keep navigation available even when no current diagram could be rendered.
    let packages: BTreeSet<_> = analysis
        .workspace
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let data =
        json!({ "crates": crates, "modules": modules, "packages": packages, "notices": notices });
    let path = config.output_dir().join("index.html");
    save(&path, &html(&data)?)?;
    analysis
        .report
        .notes
        .push(format!("wrote {} (open in a browser)", path.display()));
    analysis.report.artifacts.push(path);
    Ok(())
}

fn html(data: &Value) -> Result<String, String> {
    let data = serde_json::to_string(data)
        .map_err(|error| format!("cannot serialize architecture map: {error}"))?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    // Substitute data last so source labels containing placeholder text remain
    // literal data, even when they resemble a template marker or script tag.
    Ok(include_str!("arch_map/viewer.html")
        .replace("<!--XTASK_STYLE-->", include_str!("arch_map/viewer.css"))
        .replace("<!--XTASK_SCRIPT-->", include_str!("arch_map/viewer.js"))
        .replace("<!--XTASK_DATA-->", &data))
}

fn save(path: &Path, html: &str) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let (temporary, mut file) = TemporaryHtml::create(parent)?;
    file.write_all(html.as_bytes())
        .map_err(|error| format!("cannot write HTML {}: {error}", path.display()))?;
    drop(file);
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("cannot replace HTML {}: {error}", path.display()))?;
    }
    fs::rename(&temporary.0, path)
        .map_err(|error| format!("cannot save HTML {}: {error}", path.display()))
}

struct TemporaryHtml(PathBuf);

impl TemporaryHtml {
    fn create(parent: &Path) -> Result<(Self, fs::File), String> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".xtask-map-{}-{id}.html.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => return Ok((Self(path), file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("cannot create temporary HTML: {error}")),
            }
        }
    }
}

impl Drop for TemporaryHtml {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    //! Role: HTML埋め込みでのscript終端とテンプレート記号の無害化を検証する。
    use super::*;

    #[test]
    fn data_cannot_close_the_script_or_expand_template_markers() {
        let value = "</script><script>alert('&')</script>\u{2028}\u{2029}<!--XTASK_SCRIPT-->";
        let document = html(&json!({ "notices": [value] })).unwrap();
        let script = document.split("id=\"architecture-data\"").nth(1).unwrap();
        let payload = script
            .split_once('>')
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0;
        assert!(!payload.contains('<'));
        assert!(!payload.contains('&'));
        let parsed: Value = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed["notices"][0], value);
    }
}
