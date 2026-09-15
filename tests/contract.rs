use hquota::{
    codex, command_code,
    config::Provider,
    model::{AccountQuota, ErrorCode, QuotaReport},
};
#[test]
fn public_snapshot_and_renderer() {
    let report = QuotaReport {
        schema_version: 1,
        fetched_at: "2026-09-15T00:00:00Z".into(),
        accounts: vec![
            codex::normalize(include_bytes!("fixtures/codex.json"), "business").unwrap(),
            AccountQuota::failure(
                Provider::Codex,
                "personal",
                ErrorCode::AuthenticationRequired,
            ),
            command_code::normalize(include_bytes!("fixtures/command-code.json"), "goat").unwrap(),
        ],
    };
    let actual = serde_json::to_value(&report).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/report.json")).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(hquota::client::report(actual).unwrap(), report);
    let human = hquota::render::human(&report);
    for text in [
        "business",
        "personal",
        "goat",
        "5h",
        "7d",
        "authentication_required",
        "headroom 50%",
        "balance",
    ] {
        assert!(human.contains(text), "{text}");
    }
}
#[test]
fn malformed_body_is_not_public() {
    let secret = b"synthetic-private-provider-body";
    for result in [
        codex::normalize(secret, "test"),
        command_code::normalize(secret, "test"),
    ] {
        assert_eq!(result.unwrap_err(), ErrorCode::UpstreamSchemaChanged);
    }
}
