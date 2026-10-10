//! Role: 既定のリポジトリルートで共通xtaskのCLIを起動する。
fn main() -> std::process::ExitCode {
    xtask_core::main(std::env::current_dir().expect("current directory is accessible"))
}
