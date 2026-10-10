//! Role: 組み込みELKをNodeで実行し、配置結果の読込みと検証を行う。
use crate::model::{LayoutRequest, Scene};
use crate::prepare;
use crate::validate;
use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn layout(request: &LayoutRequest) -> Result<Scene, String> {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    layout_with_node(request, &binary)
}

/// Use an explicit Node executable without changing global environment.
pub fn layout_with_node(request: &LayoutRequest, binary: &OsStr) -> Result<Scene, String> {
    validate::request(request)?;
    let directory =
        tempfile::tempdir().map_err(|error| format!("cannot prepare ELK runtime: {error}"))?;
    let runner = directory.path().join("runner.cjs");
    let bundle = directory.path().join("elk.bundled.js");
    std::fs::write(&runner, include_bytes!("../runtime/runner.cjs"))
        .map_err(|error| format!("cannot write ELK runner: {error}"))?;
    std::fs::write(&bundle, include_bytes!("../vendor/elkjs/elk.bundled.js"))
        .map_err(|error| format!("cannot write embedded ELK: {error}"))?;
    let input = serde_json::to_vec(&prepare::request(request))
        .map_err(|error| format!("cannot encode layout request: {error}"))?;
    let mut child = Command::new(binary).arg(&runner).arg(&bundle)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|error| format!("cannot run Node {:?}: {error}; install Node.js or set XTASK_NODE to its executable", binary))?;
    let write_result = child
        .stdin
        .take()
        .ok_or("ELK runtime stdin is unavailable")?
        .write_all(&input);
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("cannot send layout request to ELK: {error}"));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("cannot wait for ELK runtime: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ELK layout failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.iter().all(u8::is_ascii_whitespace) {
        return Err("ELK returned an empty scene".into());
    }
    let scene = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid ELK scene JSON: {error}"))?;
    validate::scene(request, &scene)?;
    Ok(scene)
}
