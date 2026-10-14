//! Role: 組み込みELKのNode実行を再利用し、配置結果の読込みと検証を行う。
use crate::model::{LayoutRequest, Scene};
use crate::{prepare, validate};
use serde::Deserialize;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread::JoinHandle;

pub fn layout(request: &LayoutRequest) -> Result<Scene, String> {
    LayoutSession::default().layout(request)
}

/// Use an explicit Node executable without changing global environment.
pub fn layout_with_node(request: &LayoutRequest, binary: &OsStr) -> Result<Scene, String> {
    LayoutSession::with_node(binary).layout(request)
}

/// Reuse one Node process and embedded bundle for a sequence of independent
/// diagrams. The worker starts lazily and is stopped when this session drops.
/// Own this in a normal scope, rather than thread-local storage: Windows holds
/// the loader lock during thread-local destruction, preventing reader joins.
pub struct LayoutSession {
    binary: OsString,
    runtime: Option<NodeRuntime>,
}

impl Default for LayoutSession {
    fn default() -> Self {
        let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
        Self::with_node(&binary)
    }
}

impl LayoutSession {
    pub fn with_node(binary: &OsStr) -> Self {
        Self {
            binary: binary.into(),
            runtime: None,
        }
    }

    pub fn layout(&mut self, request: &LayoutRequest) -> Result<Scene, String> {
        validate::request(request)?;
        let mut input = serde_json::to_vec(&prepare::request(request))
            .map_err(|error| format!("cannot encode layout request: {error}"))?;
        input.push(b'\n');
        if self.runtime.is_none() {
            self.runtime = Some(NodeRuntime::start(&self.binary)?);
        }
        let runtime = self.runtime.as_mut().expect("initialized Node runtime");
        let result = runtime.exchange(&input).and_then(|scene| {
            validate::scene(request, &scene)?;
            Ok(scene)
        });
        match result {
            Ok(scene) => Ok(scene),
            Err(error) => {
                // A broken stream or invalid scene must not poison later calls.
                let mut runtime = self.runtime.take().expect("initialized Node runtime");
                Err(runtime.failure(error))
            }
        }
    }
}

#[derive(Deserialize)]
struct Response {
    scene: Option<Scene>,
    error: Option<String>,
}

struct NodeRuntime {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: Option<JoinHandle<std::io::Result<File>>>,
    // The bundle is written only once and must outlive the Node process.
    _directory: tempfile::TempDir,
}

impl NodeRuntime {
    fn start(binary: &OsStr) -> Result<Self, String> {
        let directory =
            tempfile::tempdir().map_err(|error| format!("cannot prepare ELK runtime: {error}"))?;
        std::fs::write(
            directory.path().join("runner.cjs"),
            include_bytes!("../runtime/runner.cjs"),
        )
        .map_err(|error| format!("cannot write ELK runner: {error}"))?;
        std::fs::write(
            directory.path().join("elk.bundled.js"),
            include_bytes!("../vendor/elkjs/elk.bundled.js"),
        )
        .map_err(|error| format!("cannot write embedded ELK: {error}"))?;
        Self::spawn(binary, directory)
    }

    fn spawn(binary: &OsStr, directory: tempfile::TempDir) -> Result<Self, String> {
        // Spool diagnostics to disk instead of retaining an unbounded buffer or
        // allowing a full stderr pipe to block the layout worker.
        let mut diagnostics = tempfile::tempfile()
            .map_err(|error| format!("cannot prepare ELK diagnostics: {error}"))?;
        let mut child = Command::new(binary).arg(directory.path().join("runner.cjs")).arg(directory.path().join("elk.bundled.js"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|error| format!("cannot run Node {:?}: {error}; install Node.js or set XTASK_NODE to its executable", binary))?;
        let stdin = child.stdin.take().expect("piped Node stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped Node stdout"));
        let mut stderr = child.stderr.take().expect("piped Node stderr");
        let reader = std::thread::Builder::new()
            .name("xtask-elk-stderr".into())
            .spawn(move || {
                std::io::copy(&mut stderr, &mut diagnostics)?;
                Ok(diagnostics)
            });
        let stderr = match reader {
            Ok(reader) => Some(reader),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cannot read ELK diagnostics: {error}"));
            }
        };
        Ok(Self {
            child,
            stdin,
            stdout,
            stderr,
            _directory: directory,
        })
    }

    fn exchange(&mut self, input: &[u8]) -> Result<Scene, String> {
        self.stdin
            .write_all(input)
            .map_err(|error| format!("cannot send layout request to ELK: {error}"))?;
        let mut output = String::new();
        self.stdout
            .read_line(&mut output)
            .map_err(|error| format!("cannot read ELK scene: {error}"))?;
        if output.trim().is_empty() {
            return Err("ELK returned an empty scene".into());
        }
        let response: Response = serde_json::from_str(&output)
            .map_err(|error| format!("invalid ELK scene JSON: {error}"))?;
        match (response.scene, response.error) {
            (Some(scene), None) => Ok(scene),
            (None, Some(error)) => Err(format!("ELK layout failed: {error}")),
            _ => Err("invalid ELK scene JSON: expected a scene or an error".into()),
        }
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn failure(&mut self, mut error: String) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                error.push_str(&format!("; ELK runtime exited with {status}"));
            }
            _ => (),
        }
        self.stop();
        if let Some(reader) = self.stderr.take() {
            match reader.join() {
                Ok(Ok(mut diagnostics)) => {
                    let mut bytes = Vec::new();
                    match diagnostics
                        .rewind()
                        .and_then(|()| diagnostics.read_to_end(&mut bytes))
                    {
                        Ok(_) => {
                            let message = String::from_utf8_lossy(&bytes);
                            if !message.trim().is_empty() {
                                error.push_str(&format!("; ELK stderr: {}", message.trim()));
                            }
                        }
                        Err(problem) => {
                            error.push_str(&format!("; cannot read ELK diagnostics: {problem}"))
                        }
                    }
                }
                Ok(Err(problem)) => {
                    error.push_str(&format!("; cannot read ELK diagnostics: {problem}"))
                }
                Err(_) => error.push_str("; ELK diagnostics reader panicked"),
            }
        }
        error
    }
}

impl Drop for NodeRuntime {
    fn drop(&mut self) {
        self.stop();
        if let Some(reader) = self.stderr.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    //! Role: Node再利用の独立性・復旧と診断ストリームの保存を検証する。
    use super::*;
    use crate::{Direction, NodeRequest};

    fn node() -> Option<OsString> {
        let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
        Command::new(&binary)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
            .then_some(binary)
    }

    fn request() -> LayoutRequest {
        LayoutRequest {
            title: "reusable worker 日本語\nsecond line".into(),
            direction: Direction::LeftToRight,
            nodes: vec![NodeRequest {
                id: "a".into(),
                label: "first\n日本語".into(),
            }],
            regions: vec![],
            edges: vec![],
            ranks: vec![],
        }
    }

    #[test]
    fn reuses_node_without_scene_state_and_recovers_after_worker_exit() {
        let Some(binary) = node() else {
            return;
        };
        let request = request();
        let mut session = LayoutSession::with_node(&binary);
        let first = session.layout(&request).unwrap();
        let pid = session.runtime.as_ref().unwrap().child.id();
        let mut different = request.clone();
        different.title = "different graph".into();
        different.nodes.push(NodeRequest {
            id: "b".into(),
            label: "second".into(),
        });
        assert_eq!(session.layout(&different).unwrap().nodes.len(), 2);
        assert_eq!(session.layout(&request).unwrap(), first);
        assert_eq!(session.runtime.as_ref().unwrap().child.id(), pid);
        let fresh_request = request.clone();
        let fresh_binary = binary.clone();
        let fresh =
            std::thread::spawn(move || layout_with_node(&fresh_request, &fresh_binary).unwrap())
                .join()
                .unwrap();
        assert_eq!(first, fresh);
        session.runtime.as_mut().unwrap().stop();
        assert!(session.layout(&request).is_err());
        assert!(session.runtime.is_none());
        assert_eq!(session.layout(&request).unwrap(), first);
        assert!(
            layout_with_node(&request, "missing-node-executable".as_ref())
                .unwrap_err()
                .contains("XTASK_NODE")
        );
        assert_eq!(session.layout(&request).unwrap(), first);
    }

    fn fixture(binary: &OsStr, script: &str) -> NodeRuntime {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("runner.cjs"), script).unwrap();
        NodeRuntime::spawn(binary, directory).unwrap()
    }

    #[test]
    fn drains_large_stderr_and_preserves_diagnostics_for_invalid_json() {
        let Some(binary) = node() else {
            return;
        };
        let mut runtime = fixture(
            &binary,
            "process.stdin.once('data', () => { process.stderr.write('x'.repeat(256 * 1024) + 'diagnostic-end', () => process.stdout.write('invalid-json\\n')); });",
        );
        let error = runtime.exchange(b"{}\n").unwrap_err();
        let error = runtime.failure(error);
        assert!(error.contains("invalid ELK scene JSON"));
        assert!(error.ends_with("diagnostic-end"));
        assert!(error.len() > 256 * 1024);
    }

    #[test]
    fn reports_worker_errors_and_empty_output_with_stderr() {
        let Some(binary) = node() else {
            return;
        };
        let mut runtime = fixture(
            &binary,
            "process.stdin.once('data', () => process.stdout.write(JSON.stringify({error: 'layout diagnostic 日本語'}) + '\\n'));",
        );
        let error = runtime.exchange(b"{}\n").unwrap_err();
        assert!(runtime.failure(error).contains("layout diagnostic 日本語"));
        let mut runtime = fixture(
            &binary,
            "process.stdin.once('data', () => { console.error('startup diagnostic'); process.exitCode = 7; process.stdin.destroy(); });",
        );
        let error = runtime.exchange(b"{}\n").unwrap_err();
        let error = runtime.failure(error);
        assert!(error.contains("empty scene"));
        assert!(error.contains("startup diagnostic"));
    }

    #[test]
    fn invalid_scene_is_rejected_and_session_restarts() {
        let Some(binary) = node() else {
            return;
        };
        let runtime = fixture(
            &binary,
            "process.stdin.once('data', () => process.stdout.write(JSON.stringify({scene: {title: 'unexpected', width: 0, height: 0, nodes: [], regions: [], edges: []}}) + '\\n'));",
        );
        let mut session = LayoutSession {
            binary,
            runtime: Some(runtime),
        };
        let request = request();
        assert!(
            session
                .layout(&request)
                .unwrap_err()
                .contains("invalid ELK scene")
        );
        assert!(session.runtime.is_none());
        assert_eq!(session.layout(&request).unwrap().nodes.len(), 1);
    }
}
