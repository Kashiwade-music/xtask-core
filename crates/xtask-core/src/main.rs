//! Role: 既定のリポジトリルートで共通xtaskのCLIを起動する。
use std::path::Path;

fn main() -> std::process::ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/xtask-core lies under the repository root");
    xtask_core::main(root)
}
