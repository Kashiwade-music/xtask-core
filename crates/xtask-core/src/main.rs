fn main() -> std::process::ExitCode {
    xtask_core::main(std::env::current_dir().expect("current directory is accessible"))
}
