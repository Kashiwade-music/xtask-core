//! Role: CLI引数を設定へ変換し、検査結果と実行エラーを表示する。
use std::path::Path;
use std::process::ExitCode;

use crate::{Config, run};

const USAGE: &str =
    "usage: xtask check [--root PATH] [--no-write | --prune] [--help|-h]\n       xtask --help|-h";

/// Parse arguments excluding the executable name. Help also yields a valid
/// configuration; the CLI entrypoint displays usage before invoking the check.
pub fn parse_args(
    root: impl AsRef<Path>,
    arguments: impl IntoIterator<Item = String>,
) -> Result<Config, String> {
    let mut arguments = arguments.into_iter();
    let mut config = Config::new(root);
    match arguments.next().as_deref() {
        Some("check") => {}
        Some("--help" | "-h") => {
            if let Some(argument) = arguments.next() {
                return Err(format!(
                    "unexpected argument `{argument}` after help\n{USAGE}"
                ));
            }
            return Ok(config);
        }
        Some(command) => return Err(format!("unknown command `{command}`\n{USAGE}")),
        None => return Err(format!("missing command `check`\n{USAGE}")),
    }
    let mut root_seen = false;
    let mut help_seen = false;
    let mut no_write = false;
    let mut prune = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => {
                if root_seen {
                    return Err(format!("duplicate --root option\n{USAGE}"));
                }
                let value = arguments
                    .next()
                    .filter(|value| !value.is_empty() && !value.starts_with('-'))
                    .ok_or_else(|| format!("--root requires a path\n{USAGE}"))?;
                config = Config::new(value);
                root_seen = true;
            }
            "--help" | "-h" => {
                if help_seen {
                    return Err(format!("duplicate help option\n{USAGE}"));
                }
                help_seen = true;
            }
            "--no-write" => {
                if no_write {
                    return Err(format!("duplicate --no-write option\n{USAGE}"));
                }
                no_write = true;
            }
            "--prune" => {
                if prune {
                    return Err(format!("duplicate --prune option\n{USAGE}"));
                }
                prune = true;
            }
            _ => return Err(format!("unknown argument `{argument}`\n{USAGE}")),
        }
    }
    if no_write && prune {
        return Err(format!(
            "--prune cannot be combined with --no-write\n{USAGE}"
        ));
    }
    config.no_write = no_write;
    config.prune = prune;
    Ok(config)
}

/// Entrypoint for the consumer's tiny `dev/xtask/src/main.rs`.
pub fn main(root: impl AsRef<Path>) -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let config = match parse_args(root, arguments.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
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

    #[test]
    fn check_uses_default_configuration() {
        let config = parse_args("repo", ["check".into()]).unwrap();
        assert_eq!(config.root, Path::new("repo"));
        assert_eq!(
            config.policy,
            Path::new("repo/dev/xtask/policy/workspace.toml")
        );
        assert_eq!(config.output, None);
        assert!(!config.no_write);
        assert!(!config.prune);
        assert_eq!(config.max_role_chars, None);
        assert!(config.analysis.is_none());
    }

    #[test]
    fn root_override_recreates_configuration_and_default_paths() {
        let config = parse_args("old", ["check", "--root", "repo"].map(str::to_owned)).unwrap();
        assert_eq!(config.root, Path::new("repo"));
        assert_eq!(config.output, None);
        assert!(!config.no_write);
        assert_eq!(
            config.policy,
            Path::new("repo/dev/xtask/policy/workspace.toml")
        );
    }

    #[test]
    fn no_write_survives_root_override_in_either_order() {
        for arguments in [
            vec!["check", "--no-write", "--root", "repo"],
            vec!["check", "--root", "repo", "--no-write"],
        ] {
            let config = parse_args("old", arguments.into_iter().map(str::to_owned)).unwrap();
            assert!(config.no_write);
            assert_eq!(config.root, Path::new("repo"));
            assert_eq!(
                config.policy,
                Path::new("repo/dev/xtask/policy/workspace.toml")
            );
            assert_eq!(config.output, None);
        }
        assert!(
            parse_args("old", ["check", "--no-write"].map(str::to_owned))
                .unwrap()
                .no_write
        );
    }

    #[test]
    fn duplicate_no_write_returns_usage_error() {
        for arguments in [
            vec!["check", "--no-write", "--no-write"],
            vec!["check", "--no-write", "--root", "repo", "--no-write"],
        ] {
            let error = parse_args("old", arguments.into_iter().map(str::to_owned)).unwrap_err();
            assert!(error.contains("duplicate --no-write"));
            assert!(error.contains(USAGE));
        }
    }

    #[test]
    fn prune_survives_root_override_and_conflicts_with_no_write() {
        for arguments in [
            vec!["check", "--prune", "--root", "repo"],
            vec!["check", "--root", "repo", "--prune"],
        ] {
            let config = parse_args("old", arguments.into_iter().map(str::to_owned)).unwrap();
            assert!(config.prune);
            assert!(!config.no_write);
            assert_eq!(config.root, Path::new("repo"));
        }
        for arguments in [
            vec!["check", "--prune", "--no-write"],
            vec!["check", "--no-write", "--prune"],
            vec!["check", "--prune", "--prune"],
        ] {
            let error = parse_args("old", arguments.into_iter().map(str::to_owned)).unwrap_err();
            assert!(error.contains(USAGE));
        }
    }

    #[test]
    fn removed_commands_and_options_are_rejected() {
        for command in ["deps-graph", "modules-graph", "graphs", "nope"] {
            let error = parse_args(".", [command.to_owned()]).unwrap_err();
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
            let error = parse_args(".", ["check", option].map(str::to_owned)).unwrap_err();
            assert!(error.contains("unknown argument"));
            assert!(error.contains(USAGE));
        }
    }

    #[test]
    fn missing_and_surplus_arguments_return_usage_errors() {
        for arguments in [
            vec![],
            vec!["check", "--root"],
            vec!["check", "--root", "--help"],
            vec!["check", "--root", ""],
            vec!["check", "extra"],
            vec!["check", "--typo"],
            vec!["check", "--root", "one", "--root", "two"],
            vec!["--help", "extra"],
            vec!["check", "--help", "--help"],
            vec!["check", "--no-write", "true"],
        ] {
            let error = parse_args(".", arguments.into_iter().map(str::to_owned)).unwrap_err();
            assert!(error.contains(USAGE));
        }
    }

    #[test]
    fn help_is_accepted_on_its_own_or_with_check() {
        for arguments in [
            vec!["--help"],
            vec!["-h"],
            vec!["check", "--help"],
            vec!["check", "-h"],
        ] {
            let config =
                parse_args("not-present", arguments.into_iter().map(str::to_owned)).unwrap();
            assert_eq!(config.root, Path::new("not-present"));
        }
        let config = parse_args(
            "old",
            ["check", "--help", "--root", "repo"].map(str::to_owned),
        )
        .unwrap();
        assert_eq!(config.root, Path::new("repo"));
    }
}
