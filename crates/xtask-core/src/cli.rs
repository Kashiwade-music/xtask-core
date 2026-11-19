//! Role: CLI引数を設定へ変換し、検査結果と実行エラーを表示する。
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{Config, Output, run};

const USAGE: &str = concat!(
    "usage: xtask check [--root PATH] [--write]\n",
    "       xtask --help|-h\n",
    "\n",
    "check   runs every repository rule. It writes nothing unless --write is given.\n",
    "--write also lays out the architecture diagrams and writes them to the file\n",
    "        named by [diagrams] output (default target/xtask/architecture.xtaskmap).",
);

/// What the command line asks for: the usage text, or a check with its configuration.
#[derive(Debug)]
pub enum Command {
    Help,
    Check(Config),
}

/// Parse arguments excluding the executable name. `root` is the repository root
/// unless `--root` names another path.
pub fn parse_args(
    root: &Path,
    arguments: impl IntoIterator<Item = String>,
) -> Result<Command, String> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        Some("check") => {}
        Some("--help" | "-h") => {
            if let Some(argument) = arguments.next() {
                return Err(format!(
                    "unexpected argument `{argument}` after help\n{USAGE}"
                ));
            }
            return Ok(Command::Help);
        }
        Some(command) => return Err(format!("unknown command `{command}`\n{USAGE}")),
        None => return Err(format!("missing command `check`\n{USAGE}")),
    }
    let mut selected_root = None;
    let mut help_seen = false;
    let mut write_seen = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => {
                if selected_root.is_some() {
                    return Err(format!("duplicate --root option\n{USAGE}"));
                }
                selected_root = Some(root_value(arguments.next())?);
            }
            "--help" | "-h" => {
                if help_seen {
                    return Err(format!("duplicate help option\n{USAGE}"));
                }
                help_seen = true;
            }
            "--write" => {
                if write_seen {
                    return Err(format!("duplicate --write option\n{USAGE}"));
                }
                write_seen = true;
            }
            _ => return Err(format!("unknown argument `{argument}`\n{USAGE}")),
        }
    }
    if help_seen {
        return Ok(Command::Help);
    }
    let mut config = Config::new(selected_root.unwrap_or_else(|| root.to_path_buf()));
    if write_seen {
        config.output = Output::Policy;
    }
    Ok(Command::Check(config))
}

/// The value of `--root`. A relative path is resolved from the current directory.
fn root_value(value: Option<String>) -> Result<PathBuf, String> {
    let value = value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("--root requires a path\n{USAGE}"))?;
    let current = std::env::current_dir()
        .map_err(|error| format!("cannot read the current directory: {error}"))?;
    Ok(current.join(value))
}

/// Entrypoint for the consumer's tiny `dev/xtask/src/main.rs`.
pub fn main(root: &Path) -> ExitCode {
    let config = match parse_args(root, std::env::args().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Check(config)) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let report = match run(&config) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    for note in &report.notes {
        println!("{note}");
    }
    for error in &report.errors {
        eprintln!("error: {error}");
    }
    if report.is_success() {
        println!("ok");
        ExitCode::SUCCESS
    } else {
        eprintln!("{} error(s)", report.errors.len());
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    //! Role: checkだけのCLI、既定設定、root再設定とヘルプ引数を検証する。
    use super::*;

    fn parse(root: &str, arguments: &[&str]) -> Result<Command, String> {
        parse_args(
            Path::new(root),
            arguments.iter().map(|argument| (*argument).to_owned()),
        )
    }

    fn check_config(command: Command) -> Config {
        match command {
            Command::Check(config) => config,
            Command::Help => panic!("expected a check command"),
        }
    }

    fn current() -> PathBuf {
        std::env::current_dir().unwrap()
    }

    #[test]
    fn check_uses_default_configuration() {
        let config = check_config(parse("repo", &["check"]).unwrap());
        assert_eq!(config.root, Path::new("repo"));
        assert_eq!(
            config.policy,
            Path::new("repo/dev/xtask/policy/workspace.toml")
        );
        assert_eq!(config.output, Output::None);
        assert!(config.analysis.is_none());
    }

    #[test]
    fn root_override_recreates_default_paths() {
        let config = check_config(parse("old", &["check", "--root", "repo"]).unwrap());
        let root = current().join("repo");
        assert_eq!(config.root, root);
        assert_eq!(config.output, Output::None);
        assert_eq!(config.policy, root.join("dev/xtask/policy/workspace.toml"));
    }

    #[test]
    fn write_selects_the_policy_output_in_either_order() {
        for arguments in [
            vec!["check", "--write", "--root", "repo"],
            vec!["check", "--root", "repo", "--write"],
        ] {
            let config = check_config(parse("old", &arguments).unwrap());
            assert_eq!(config.output, Output::Policy);
            assert_eq!(config.root, current().join("repo"));
        }
    }

    #[test]
    fn root_value_may_start_with_dash() {
        let config = check_config(parse(".", &["check", "--root", "-x"]).unwrap());
        assert_eq!(config.root, current().join("-x"));
        let config = check_config(parse(".", &["check", "--root", "--help"]).unwrap());
        assert_eq!(config.root, current().join("--help"));
    }

    #[test]
    fn root_requires_a_value() {
        for arguments in [vec!["check", "--root"], vec!["check", "--root", ""]] {
            let error = parse(".", &arguments).unwrap_err();
            assert!(error.contains("--root requires a path"));
            assert!(error.contains(USAGE));
        }
    }

    #[test]
    fn old_flags_are_rejected() {
        for option in ["--no-write", "--prune"] {
            for arguments in [
                vec!["check", option],
                vec!["check", "--root", "repo", option],
            ] {
                let error = parse(".", &arguments).unwrap_err();
                assert!(error.contains("unknown argument"));
                assert!(error.contains(USAGE));
            }
        }
    }

    #[test]
    fn removed_commands_and_options_are_rejected() {
        for command in ["deps-graph", "modules-graph", "graphs", "nope"] {
            let error = parse(".", &[command]).unwrap_err();
            assert!(error.contains("unknown command"));
            assert!(error.contains(USAGE));
        }
        for option in [
            "--policy",
            "--output",
            "--max-role-chars",
            "--target",
            "--features",
            "--all-features",
            "--no-default-features",
            "--cfg",
            "--no-tests",
            "--trust-attribute",
            "--trust-derive",
        ] {
            let error = parse(".", &["check", option]).unwrap_err();
            assert!(error.contains("unknown argument"));
            assert!(error.contains(USAGE));
        }
    }

    #[test]
    fn missing_and_surplus_arguments_return_usage_errors() {
        let cases: &[&[&str]] = &[
            &[],
            &["check", "--root", "one", "--root", "two"],
            &["--help", "extra"],
            &["check", "--help", "--help"],
            &["check", "extra"],
            &["check", "--typo"],
            &["check", "--write", "--write"],
            &["check", "--write", "extra"],
        ];
        for arguments in cases {
            let error = parse(".", arguments).unwrap_err();
            assert!(error.contains(USAGE), "{arguments:?}");
        }
    }

    #[test]
    fn help_is_accepted_on_its_own_or_with_check() {
        for arguments in [
            vec!["--help"],
            vec!["-h"],
            vec!["check", "--help"],
            vec!["check", "-h"],
            vec!["check", "--help", "--root", "repo"],
        ] {
            assert!(matches!(
                parse("not-present", &arguments).unwrap(),
                Command::Help
            ));
        }
    }
}
