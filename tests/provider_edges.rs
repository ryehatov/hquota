use hquota::{
    codex, command_code,
    model::{AccountQuota, QuotaFact, Role},
};
use serde_json::json;
fn auxiliary_ids(account: &AccountQuota) -> Vec<String> {
    account
        .scopes
        .iter()
        .filter(|s| s.role == Role::Auxiliary)
        .map(|s| s.id.clone())
        .collect()
}
fn id_for_label(account: &AccountQuota, label: &str) -> String {
    account
        .scopes
        .iter()
        .find(|s| s.label.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no scope labelled {label}"))
        .id
        .clone()
}
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
fn codex_auxiliary_ids_stay_distinct_and_preserve_facts() {
    // Distinct provider identifiers that normalize to the same candidate ID, plus a provider
    // identifier that collides with an index-generated fallback. Reversal changes the index-derived
    // generated IDs; the dedicated case below covers a provider-first generated collision.
    let pools = vec![
        json!({"limit_name":"Alpha","metered_feature":"A_B","rate_limit":{"allowed":true,"limit_reached":false,"secondary_window":{"used_percent":25,"limit_window_seconds":18000,"reset_after_seconds":5,"reset_at":1789470000}}}),
        json!({"limit_name":"Beta","metered_feature":"a-b","rate_limit":{"allowed":false,"limit_reached":true}}),
        json!({"limit_name":"Generated","metered_feature":"!!!","rate_limit":{"allowed":true,"limit_reached":false}}),
        json!({"limit_name":"Provider","metered_feature":"additional-3"}),
    ];
    let mut reversed = pools.clone();
    reversed.reverse();
    // Reversal moves both the provider scope and the index-derived generated IDs. The provider ID
    // at index 3 collides with the generated ID for index 2 in the forward order only.
    let orderings = [
        (
            pools,
            ["a-b", "a-b-2", "additional-3", "additional-3-2"],
            ["Alpha", "Beta", "Generated", "Provider"],
        ),
        (
            reversed,
            ["additional-3", "additional-2", "a-b", "a-b-2"],
            ["Provider", "Generated", "Beta", "Alpha"],
        ),
    ];
    for (pools, expected, labels) in orderings {
        let body = serde_json::to_vec(&json!({
            "rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":40,"limit_window_seconds":18000,"reset_after_seconds":5,"reset_at":1789470000}},
            "additional_rate_limits":pools
        }))
        .unwrap();
        let first = codex::normalize(&body, "test").unwrap();
        let ids = auxiliary_ids(&first);
        assert_eq!(ids, expected, "deterministic collision-aware allocation");
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            4,
            "auxiliary IDs must stay distinct: {ids:?}"
        );
        // Labels are preserved verbatim on every auxiliary scope, in provider order.
        for (scope, label) in first.scopes.iter().skip(1).zip(labels) {
            assert_eq!(scope.role, Role::Auxiliary);
            assert_eq!(scope.label.as_deref(), Some(label));
        }
        // Primary headroom and every auxiliary fact survive, each tied to its own scope.
        let alpha = id_for_label(&first, "Alpha");
        let beta = id_for_label(&first, "Beta");
        assert_eq!(first.primary_scope.as_deref(), Some("codex"));
        assert_eq!(first.headroom_percent, Some(60));
        assert_eq!(first.facts.len(), 6);
        assert!(first.facts.contains(&QuotaFact::Availability {
            scope: alpha.clone(),
            allowed: true,
            reason_code: None
        }));
        assert!(first.facts.contains(&QuotaFact::Availability {
            scope: beta.clone(),
            allowed: false,
            reason_code: Some("rate_limit_reached".into())
        }));
        assert!(first.facts.iter().any(|f| matches!(f, QuotaFact::RateWindow { scope, used_percent: 25, duration_seconds: Some(18000), reset_at: Some(t), .. } if scope == &alpha && t == "2026-09-15T11:00:00Z")));
        assert!(first.facts.contains(&QuotaFact::Availability {
            scope: id_for_label(&first, "Generated"),
            allowed: true,
            reason_code: None
        }));
        // Repeated normalization of identical bytes is deterministic.
        assert_eq!(codex::normalize(&body, "test").unwrap(), first);
    }
    // Provider-first generated collision: the generated ID must not reuse the provider identifier
    // already taken at index 0.
    let provider_first = json!({
        "additional_rate_limits":[
            {"limit_name":"Provider","metered_feature":"additional-2"},
            {"limit_name":"Generated","metered_feature":"!!!"}
        ]
    });
    assert_eq!(
        auxiliary_ids(
            &codex::normalize(&serde_json::to_vec(&provider_first).unwrap(), "test").unwrap()
        ),
        ["additional-2", "additional-2-2"]
    );
}
#[test]
fn codex_repeated_provider_identifiers_get_unique_ids() {
    // Neither specification establishes raw provider-identifier uniqueness, and HEAD accepted
    // repeated unusable identifiers through the index fallback. Repeated identifiers must keep
    // normalizing, each into its own deterministic unique public scope ID.
    for metered in ["reserve", "!!!", "codex"] {
        let body = json!({
            "additional_rate_limits":[
                {"limit_name":"First","metered_feature":metered},
                {"limit_name":"Second","metered_feature":metered}
            ]
        });
        let account = codex::normalize(&serde_json::to_vec(&body).unwrap(), "test")
            .unwrap_or_else(|e| panic!("repeated identifier {metered:?} must normalize: {e:?}"));
        let ids = auxiliary_ids(&account);
        assert_eq!(ids.len(), 2);
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            2,
            "repeated identifier {metered:?} must still yield unique IDs: {ids:?}"
        );
        // Each pool keeps its own label, and the run is deterministic.
        assert_eq!(
            account
                .scopes
                .iter()
                .map(|s| s.label.as_deref())
                .collect::<Vec<_>>(),
            [Some("First"), Some("Second")]
        );
        assert_eq!(account.primary_scope, None);
        assert_eq!(
            codex::normalize(&serde_json::to_vec(&body).unwrap(), "test").unwrap(),
            account
        );
    }
    // Distinct identifiers are not duplicates even when both need generated IDs.
    let distinct = json!({
        "additional_rate_limits":[
            {"limit_name":"J1","metered_feature":"!!!"},
            {"limit_name":"J2","metered_feature":"???"}
        ]
    });
    let account = codex::normalize(&serde_json::to_vec(&distinct).unwrap(), "test").unwrap();
    assert_eq!(auxiliary_ids(&account).len(), 2);
    // The limit_name fallback feeds the same collision-aware allocation.
    let fallback = json!({
        "rate_limit":{"allowed":true,"limit_reached":false},
        "additional_rate_limits":[
            {"limit_name":"A_B","metered_feature":""},
            {"limit_name":"a-b","metered_feature":""}
        ]
    });
    assert_eq!(
        auxiliary_ids(&codex::normalize(&serde_json::to_vec(&fallback).unwrap(), "test").unwrap()),
        ["a-b", "a-b-2"]
    );
    // A disambiguated ID that later meets a provider identifier of the same shape stays distinct.
    let chained = json!({
        "additional_rate_limits":[
            {"limit_name":"A","metered_feature":"A_B"},
            {"limit_name":"B","metered_feature":"a-b"},
            {"limit_name":"C","metered_feature":"a-b-2"}
        ]
    });
    assert_eq!(
        auxiliary_ids(&codex::normalize(&serde_json::to_vec(&chained).unwrap(), "test").unwrap()),
        ["a-b", "a-b-2", "a-b-2-2"]
    );
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
