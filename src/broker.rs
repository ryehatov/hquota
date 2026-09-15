use crate::{
    config::{AccountConfig, Config, Provider},
    doctor::{Diagnostic, diagnostics as account_diagnostics, unknown_fields},
    http,
    model::{AccountQuota, ErrorCode, QuotaReport, timestamp},
    protocol::{self, Operation, Request, Response},
};
use serde_json::{Value, json};
use std::{
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::Arc,
};

pub fn acquire(
    config: &Config,
    client: &reqwest::blocking::Client,
    doctor: bool,
) -> (QuotaReport, Vec<Diagnostic>) {
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = config
            .accounts
            .iter()
            .map(|account| scope.spawn(move || fetch(account, client, doctor)))
            .collect();
        handles
            .into_iter()
            .zip(&config.accounts)
            .map(|(handle, account)| {
                handle.join().unwrap_or_else(|_| {
                    (
                        AccountQuota::failure(
                            account.provider(),
                            account.name(),
                            ErrorCode::UpstreamUnavailable,
                        ),
                        false,
                    )
                })
            })
            .collect()
    });
    let mut diagnostics = Vec::new();
    let accounts = results
        .into_iter()
        .map(|(account, unknown)| {
            if doctor {
                diagnostics.extend(account_diagnostics(&account, unknown));
            }
            account
        })
        .collect();
    (
        QuotaReport {
            schema_version: 1,
            fetched_at: timestamp(time::OffsetDateTime::now_utc().unix_timestamp())
                .expect("current UTC time is representable"),
            accounts,
        },
        diagnostics,
    )
}
fn fetch(
    account: &AccountConfig,
    client: &reqwest::blocking::Client,
    doctor: bool,
) -> (AccountQuota, bool) {
    let mut unknown = false;
    let result = (|| {
        let credential = match account.provider() {
            Provider::Codex => http::codex_credential(account.credential_path())?,
            Provider::CommandCode => http::command_credential(account.credential_path())?,
        };
        let body = http::fetch(client, account.provider(), credential)?;
        if doctor {
            unknown = unknown_fields(account.provider(), &body);
        }
        match account.provider() {
            Provider::Codex => crate::codex::normalize(&body, account.name()),
            Provider::CommandCode => crate::command_code::normalize(&body, account.name()),
        }
    })();
    (
        result
            .unwrap_or_else(|code| AccountQuota::failure(account.provider(), account.name(), code)),
        unknown,
    )
}
pub fn handle(mut stream: UnixStream, config: &Config, client: &reqwest::blocking::Client) {
    if protocol::configure(&stream, protocol::IO_TIMEOUT).is_err() {
        return;
    }
    let response = match protocol::read_json::<Value>(&mut stream) {
        Ok(value) if value.get("protocol_version").and_then(Value::as_u64) != Some(1) => {
            Response::error("protocol_version_mismatch")
        }
        Ok(value) => match serde_json::from_value::<Request>(value) {
            Ok(request) => match request.op {
                Operation::Health => {
                    Response::ok(json!({"status":"ok","configured_accounts":config.accounts.len()}))
                }
                Operation::Quota => match serde_json::to_value(acquire(config, client, false).0) {
                    Ok(payload) => Response::ok(payload),
                    Err(_) => Response::error("internal_error"),
                },
                Operation::Doctor => match serde_json::to_value(acquire(config, client, true).1) {
                    Ok(payload) => Response::ok(payload),
                    Err(_) => Response::error("internal_error"),
                },
            },
            Err(_) => Response::error("invalid_request"),
        },
        Err(_) => Response::error("invalid_request"),
    };
    let _ = protocol::write_json(&mut stream, &response);
}
pub fn bind(path: &Path) -> Result<UnixListener, &'static str> {
    let parent = path.parent().ok_or("invalid_socket_path")?;
    let directory =
        std::fs::symlink_metadata(parent).map_err(|_| "runtime_directory_unavailable")?;
    // Linux exposes effective ownership without adding a libc dependency.
    let uid = std::fs::metadata("/proc/self")
        .map_err(|_| "process_identity_unavailable")?
        .uid();
    if uid == 0 {
        return Err("broker_must_be_non_root");
    }
    if !directory.is_dir() || directory.mode() & 0o7777 != 0o700 || directory.uid() != uid {
        return Err("unsafe_runtime_directory");
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket() || metadata.uid() != uid {
                return Err("unsafe_socket_path");
            }
            match UnixStream::connect(path) {
                Ok(_) => return Err("broker_already_running"),
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(path).map_err(|_| "stale_socket_removal_failed")?
                }
                Err(_) => return Err("socket_probe_failed"),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("socket_path_unavailable"),
    }
    let listener = UnixListener::bind(path).map_err(|_| "socket_bind_failed")?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "socket_permissions_failed")?;
    Ok(listener)
}
pub fn serve(config_path: &Path) -> Result<(), &'static str> {
    let config = Arc::new(Config::load(config_path)?);
    let client = http::client().map_err(|_| "http_client_failed")?;
    let listener = bind(Path::new(protocol::SOCKET))?;
    eprintln!("hquota: listening on {}", protocol::SOCKET);
    for stream in listener.incoming() {
        let stream = stream.map_err(|_| "socket_accept_failed")?;
        let config = Arc::clone(&config);
        let client = client.clone();
        std::thread::spawn(move || handle(stream, &config, &client));
    }
    Ok(())
}
