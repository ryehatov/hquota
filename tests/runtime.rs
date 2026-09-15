use hquota::{
    broker,
    cli::Command,
    client,
    config::Config,
    http,
    model::{AccountQuota, ErrorCode, QuotaReport},
    protocol::{self, Operation, Outcome, Request, Response},
};
use std::{
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "hquota-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn real_socket_lifecycle_and_protocol_operations() {
    let temp = Temp::new();
    let socket = temp.0.join("broker.sock");
    if std::fs::metadata("/proc/self").unwrap().uid() == 0 {
        assert!(broker::bind(&socket).is_err());
        return;
    }
    let listener = broker::bind(&socket).unwrap();
    assert_eq!(std::fs::metadata(&socket).unwrap().mode() & 0o777, 0o600);
    assert!(broker::bind(&socket).is_err());
    // The duplicate-broker probe queued one harmless EOF connection.
    let (mut probe, _) = listener.accept().unwrap();
    let mut buf = [0];
    use std::io::Read;
    assert_eq!(probe.read(&mut buf).unwrap(), 0);
    drop(listener);
    let listener = broker::bind(&socket).unwrap();
    let config = Config::parse(br#"{"schema_version":1,"accounts":[]}"#).unwrap();
    let thread = std::thread::spawn(move || {
        let http = http::client().unwrap();
        for _ in 0..4 {
            let (stream, _) = listener.accept().unwrap();
            broker::handle(stream, &config, &http);
        }
    });
    assert_eq!(
        protocol::request(&socket, Operation::Health).unwrap()["configured_accounts"],
        0
    );
    let report = client::report(protocol::request(&socket, Operation::Quota).unwrap()).unwrap();
    assert!(report.accounts.is_empty());
    assert_eq!(
        protocol::request(&socket, Operation::Doctor).unwrap(),
        serde_json::json!([])
    );
    let mut stream = UnixStream::connect(&socket).unwrap();
    protocol::write_json(
        &mut stream,
        &Request {
            protocol_version: 2,
            op: Operation::Health,
        },
    )
    .unwrap();
    let response: Response = protocol::read_json(&mut stream).unwrap();
    assert!(matches!(response.result, Outcome::Error { .. }));
    thread.join().unwrap();
}
#[test]
fn client_filter_keeps_account_error_successful() {
    let temp = Temp::new();
    let path = temp.0.join("mock.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _: Request = protocol::read_json(&mut stream).unwrap();
        let account = AccountQuota::failure(
            hquota::config::Provider::Codex,
            "business",
            ErrorCode::AuthenticationRequired,
        );
        protocol::write_json(
            &mut stream,
            &Response::ok(
                serde_json::to_value(QuotaReport {
                    schema_version: 1,
                    fetched_at: "2026-09-15T00:00:00Z".into(),
                    accounts: vec![account],
                })
                .unwrap(),
            ),
        )
        .unwrap();
    });
    let (output, success) = client::run(
        Command::Quota {
            json: true,
            provider: None,
            account: Some("business".into()),
        },
        &path,
    )
    .unwrap();
    assert!(success);
    assert!(output.contains("authentication_required"));
    thread.join().unwrap();
}
#[test]
fn credentials_are_reloaded_and_doctor_is_sanitized() {
    let temp = Temp::new();
    let key = temp.0.join("key");
    let config=Config::parse(&serde_json::to_vec(&serde_json::json!({"schema_version":1,"accounts":[{"provider":"command-code","name":"test","api_key_file":key}]})).unwrap()).unwrap();
    let http = http::client().unwrap();
    let (report, _) = broker::acquire(&config, &http, false);
    assert_eq!(
        report.accounts[0].error.as_ref().unwrap().code,
        ErrorCode::CredentialMissing
    );
    std::fs::write(&key, b"synthetic private invalid key").unwrap();
    let (report, diagnostics) = broker::acquire(&config, &http, true);
    assert_eq!(
        report.accounts[0].error.as_ref().unwrap().code,
        ErrorCode::CredentialInvalid
    );
    let output = serde_json::to_string(&diagnostics).unwrap();
    assert!(!output.contains("synthetic"));
    assert!(output.contains("credential_invalid"));
}
