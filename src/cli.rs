use crate::config::{Provider, valid_name};
use std::{ffi::OsString, path::PathBuf};

pub const USAGE: &str = "usage: hquota [--json] [--provider codex|command-code] [--account NAME]\n       hquota doctor\n       hquota serve [--config PATH]";

#[derive(Debug, PartialEq)]
pub enum Command {
    Quota {
        json: bool,
        provider: Option<Provider>,
        account: Option<String>,
    },
    Doctor,
    Serve {
        config: PathBuf,
    },
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, &'static str> {
    let args: Vec<_> = args.into_iter().collect();
    if args.first().is_some_and(|s| s == "doctor") {
        return if args.len() == 1 {
            Ok(Command::Doctor)
        } else {
            Err(USAGE)
        };
    }
    if args.first().is_some_and(|s| s == "serve") {
        return match args.as_slice() {
            [_] => Ok(Command::Serve {
                config: "/etc/hquota/config.json".into(),
            }),
            [_, flag, path]
                if flag == "--config"
                    && !path.is_empty()
                    && !path.as_encoded_bytes().starts_with(b"--") =>
            {
                Ok(Command::Serve {
                    config: path.into(),
                })
            }
            _ => Err(USAGE),
        };
    }
    let (mut json, mut provider, mut account) = (false, None, None);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--json") if !json => json = true,
            Some("--provider") if provider.is_none() => {
                provider = Some(match iter.next().and_then(|s| s.to_str()) {
                    Some("codex") => Provider::Codex,
                    Some("command-code") => Provider::CommandCode,
                    _ => return Err(USAGE),
                });
            }
            Some("--account") if account.is_none() => {
                let name = iter
                    .next()
                    .and_then(|s| s.to_str())
                    .filter(|s| valid_name(s))
                    .ok_or(USAGE)?;
                account = Some(name.to_owned());
            }
            _ => return Err(USAGE),
        }
    }
    Ok(Command::Quota {
        json,
        provider,
        account,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(args: &[&str]) -> Result<Command, &'static str> {
        parse(args.iter().map(OsString::from))
    }
    #[test]
    fn accepted_and_rejected_arguments() {
        assert_eq!(
            run(&[]),
            Ok(Command::Quota {
                json: false,
                provider: None,
                account: None
            })
        );
        assert!(run(&["--json", "--account", "business", "--provider", "codex"]).is_ok());
        assert_eq!(run(&["doctor"]), Ok(Command::Doctor));
        assert!(run(&["serve", "--config", "/tmp/config.json"]).is_ok());
        for args in [
            vec!["--json", "--json"],
            vec!["--provider"],
            vec!["--provider", "other"],
            vec!["--account", "Bad"],
            vec!["--provider", "codex", "--provider", "codex"],
            vec!["--account", "a", "--account", "b"],
            vec!["serve", "--json"],
            vec!["doctor", "--json"],
            vec!["--window", "5h"],
            vec!["serve", "--config", "--json"],
            vec!["quota"],
        ] {
            assert!(run(&args).is_err(), "{args:?}");
        }
        use std::os::unix::ffi::OsStringExt;
        assert!(
            parse([
                OsString::from("serve"),
                OsString::from("--config"),
                OsString::from_vec(vec![b'/', 255])
            ])
            .is_ok()
        );
        assert!(parse([OsString::from("--account"), OsString::from_vec(vec![255])]).is_err());
    }
}
