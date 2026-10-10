//! Role: CLI引数を設定へ変換し、検査結果と実行エラーを表示する。
use std::path::Path;
use std::process::ExitCode;

use crate::{Command, Config, run};

const USAGE: &str = "usage: xtask <check|deps-graph|modules-graph|graphs> [--root PATH] [--policy PATH] [--output PATH] [--max-role-chars N] [--target TRIPLE] [--features LIST] [--all-features] [--no-default-features] [--cfg VALUE] [--no-tests] [--trust-attribute PATH] [--trust-derive PATH]";

/// Parse command-line arguments, excluding the executable name.
pub fn parse_args(
    root: impl AsRef<Path>,
    arguments: impl IntoIterator<Item = String>,
) -> Result<(Config, Command), String> {
    let mut arguments = arguments.into_iter();
    let command = match arguments.next().as_deref() {
        Some("check") => Command::Check,
        Some("deps-graph") => Command::Dependencies,
        Some("modules-graph") => Command::Modules,
        Some("graphs") => Command::Graphs,
        _ => return Err(USAGE.into()),
    };
    let mut config = Config::new(root);
    let mut policy = None;
    let mut output = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => {
                config = Config {
                    root: value(&mut arguments, &argument)?.into(),
                    ..config
                }
            }
            "--policy" => policy = Some(value(&mut arguments, &argument)?),
            "--output" => output = Some(value(&mut arguments, &argument)?),
            "--max-role-chars" => {
                let limit = value(&mut arguments, &argument)?;
                config.max_role_chars = Some(
                    limit
                        .parse::<usize>()
                        .ok()
                        .filter(|limit| *limit > 0)
                        .ok_or_else(|| {
                            format!("{argument} requires a positive integer\n{USAGE}")
                        })?,
                );
            }
            "--target" => config.target = Some(value(&mut arguments, &argument)?),
            "--features" => config.features.extend(
                value(&mut arguments, &argument)?
                    .split([',', ' '])
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned),
            ),
            "--cfg" => config.cfg.push(value(&mut arguments, &argument)?),
            "--trust-attribute" => {
                config
                    .trusted_attributes
                    .insert(value(&mut arguments, &argument)?);
            }
            "--trust-derive" => {
                config
                    .trusted_derives
                    .insert(value(&mut arguments, &argument)?);
            }
            "--all-features" => config.all_features = true,
            "--no-default-features" => config.default_features = false,
            "--no-tests" => config.tests = false,
            _ => return Err(format!("unknown argument `{argument}`\n{USAGE}")),
        }
    }
    config.policy = config
        .root
        .join(policy.unwrap_or_else(|| "dev/xtask/policy/workspace.toml".into()));
    config.output = config
        .root
        .join(output.unwrap_or_else(|| "architecture".into()));
    Ok((config, command))
}

fn value(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value\n{USAGE}"))
}

/// Entrypoint for the consumer's tiny `dev/xtask/src/main.rs`.
pub fn main(root: impl AsRef<Path>) -> ExitCode {
    let (config, command) = match parse_args(root, std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let report = match run(&config, command) {
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
    //! Role: CLI引数の検証と、パス・feature設定の反映を検証する。

    use super::*;

    #[test]
    fn root_overrides_rebase_default_paths_and_features_accumulate() {
        let args = [
            "check",
            "--root",
            "repo",
            "--features",
            "one,two",
            "--features",
            "three",
            "--no-default-features",
        ];
        let (config, command) = parse_args("old", args.map(str::to_owned)).unwrap();
        assert_eq!(command, Command::Check);
        assert_eq!(config.output, Path::new("repo/architecture"));
        assert_eq!(
            config.policy,
            Path::new("repo/dev/xtask/policy/workspace.toml")
        );
        assert_eq!(config.features, ["one", "two", "three"]);
        assert!(!config.default_features);
    }

    #[test]
    fn bad_arguments_return_usage_errors() {
        for args in [
            vec![],
            vec!["check", "--output"],
            vec!["check", "--typo"],
            vec!["check", "--max-role-chars"],
            vec!["check", "--max-role-chars", "0"],
            vec!["check", "--max-role-chars", "-1"],
            vec!["check", "--max-role-chars", "many"],
            vec!["check", "--max-role-chars", "9999999999999999999999999999"],
            vec!["nope"],
        ] {
            assert!(parse_args(".", args.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn role_limit_overrides_survive_root_changes_and_accept_values_above_default() {
        let (default, _) = parse_args(".", ["check".into()]).unwrap();
        assert_eq!(default.max_role_chars, None);
        let (config, _) = parse_args(
            ".",
            ["graphs", "--max-role-chars", "160", "--root", "repo"].map(str::to_owned),
        )
        .unwrap();
        assert_eq!(config.max_role_chars, Some(160));
        assert_eq!(config.root, Path::new("repo"));
    }
}
