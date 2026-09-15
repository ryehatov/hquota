use hquota::{
    codex, command_code,
    model::{QuotaFact, Role},
};
use serde_json::json;
#[test]
fn codex_auxiliary_spend_and_exact_duration() {
    let fixture = json!({
        "rate_limit":{"allowed":false,"limit_reached":true,"primary_window":{"used_percent":100,"limit_window_seconds":900,"reset_after_seconds":5,"reset_at":1789470000}},
        "additional_rate_limits":[{"limit_name":"Reserve","metered_feature":"reserve","rate_limit":{"allowed":true,"limit_reached":false,"secondary_window":{"used_percent":1,"limit_window_seconds":18000,"reset_after_seconds":5,"reset_at":1789470000}}}],
        "credits":{"has_credits":true,"unlimited":true,"balance":null},
        "spend_control":{"reached":false,"individual_limit":{"limit":"50","used":"20","remaining":"30","used_percent":40,"remaining_percent":60,"reset_after_seconds":5,"reset_at":1789470000}}
    });
    let account = codex::normalize(&serde_json::to_vec(&fixture).unwrap(), "test").unwrap();
    assert_eq!(account.headroom_percent, Some(0));
    assert_eq!(account.scopes[1].role, Role::Auxiliary);
    assert!(matches!(
        &account.facts[0],
        QuotaFact::RateWindow {
            canonical_window: None,
            ..
        }
    ));
    assert!(account.facts.iter().any(|f| matches!(
        f,
        QuotaFact::SpendLimit {
            remaining_percent: Some(60),
            ..
        }
    )));
    let mut bad = fixture.clone();
    bad["credits"]["balance"] = json!("NaN");
    assert!(codex::normalize(&serde_json::to_vec(&bad).unwrap(), "test").is_err());
    let mut bad = fixture;
    bad["additional_rate_limits"][0]["rate_limit"]["allowed"] = json!("yes");
    assert!(codex::normalize(&serde_json::to_vec(&bad).unwrap(), "test").is_err());
}
#[test]
fn command_code_returned_cap_and_reset_validation() {
    let fixture = json!({"credits":{"windowLimits":{"fiveHour":{"used":1.25,"cap":5,"resetAt":"2026-09-15T13:00:00+02:00"}}}});
    let account = command_code::normalize(&serde_json::to_vec(&fixture).unwrap(), "test").unwrap();
    assert_eq!(account.headroom_percent, Some(75));
    assert!(
        matches!(&account.facts[0],QuotaFact::RateWindow{reset_at:Some(t),..} if t=="2026-09-15T11:00:00Z")
    );
    for (key, value) in [
        ("used", json!(-1)),
        ("used", json!("1")),
        ("resetAt", json!(false)),
        ("resetAt", json!("invalid")),
    ] {
        let mut bad = fixture.clone();
        bad["credits"]["windowLimits"]["fiveHour"][key] = value;
        assert!(command_code::normalize(&serde_json::to_vec(&bad).unwrap(), "test").is_err());
    }
}
