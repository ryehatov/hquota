use crate::{
    config::Provider,
    model::{AccountQuota, Status},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub provider: Provider,
    pub account: String,
    pub severity: Severity,
    pub code: String,
}
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Ok,
    Warning,
    Error,
}

pub fn diagnostics(account: &AccountQuota, unknown: bool) -> Vec<Diagnostic> {
    let mut results = vec![Diagnostic {
        provider: account.provider,
        account: account.account.clone(),
        severity: if account.status == Status::Ok {
            Severity::Ok
        } else {
            Severity::Error
        },
        code: account
            .error
            .as_ref()
            .map(|e| {
                serde_json::to_value(e.code)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .unwrap_or_else(|| "checks_passed".into()),
    }];
    if unknown {
        results.push(Diagnostic {
            provider: account.provider,
            account: account.account.clone(),
            severity: Severity::Warning,
            code: "unknown_provider_fields".into(),
        });
    }
    results
}
// Only return a fixed warning code. Unknown key names can themselves contain private data.
pub fn unknown_fields(provider: Provider, bytes: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    inspect(
        &value,
        if provider == Provider::Codex {
            "codex"
        } else {
            "command"
        },
    )
}
fn inspect(value: &Value, kind: &str) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let keys: &[&str] = match kind {
        "codex" => &[
            "plan_type",
            "rate_limit",
            "additional_rate_limits",
            "credits",
            "spend_control",
            "rate_limit_reached_type",
            "rate_limit_reset_credits",
            "account_id",
            "user_id",
            "rate_limit_upsell",
        ],
        "pool" => &[
            "allowed",
            "limit_reached",
            "primary_window",
            "secondary_window",
        ],
        "window" => &[
            "used_percent",
            "limit_window_seconds",
            "reset_after_seconds",
            "reset_at",
        ],
        "additional" => &[
            "limit_name",
            "metered_feature",
            "normal_model_slug",
            "rate_limit",
        ],
        "credits" => &[
            "has_credits",
            "unlimited",
            "balance",
            "approx_local_messages",
            "approx_cloud_messages",
        ],
        "spend" => &["reached", "individual_limit"],
        "limit" => &[
            "source",
            "limit",
            "used",
            "remaining",
            "used_percent",
            "remaining_percent",
            "reset_after_seconds",
            "reset_at",
        ],
        "reason" => &["type"],
        "reset" => &["available_count"],
        "command" => &["credits", "windowLimits"],
        "command-credits" => &[
            "monthlyCredits",
            "purchasedCredits",
            "freeCredits",
            "premiumMonthlyCredits",
            "opensourceMonthlyCredits",
            "belowThreshold",
            "creditThreshold",
            "windowLimits",
        ],
        "windows" => &["fiveHour", "weekly"],
        "command-window" => &["used", "cap", "resetAt"],
        _ => return false,
    };
    if object.keys().any(|key| !keys.contains(&key.as_str())) {
        return true;
    }
    object.iter().any(|(key, value)| {
        let child = match (kind, key.as_str()) {
            ("codex", "rate_limit") | ("additional", "rate_limit") => "pool",
            ("pool", "primary_window" | "secondary_window") => "window",
            ("codex", "credits") => "credits",
            ("codex", "spend_control") => "spend",
            ("spend", "individual_limit") => "limit",
            ("codex", "rate_limit_reached_type") => "reason",
            ("codex", "rate_limit_reset_credits") => "reset",
            ("command", "credits") => "command-credits",
            ("command" | "command-credits", "windowLimits") => "windows",
            ("windows", "fiveHour" | "weekly") => "command-window",
            ("codex", "additional_rate_limits") => {
                return value
                    .as_array()
                    .is_some_and(|items| items.iter().any(|v| inspect(v, "additional")));
            }
            _ => return false,
        };
        inspect(value, child)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warnings_are_nested_and_sanitized() {
        assert!(unknown_fields(
            Provider::Codex,
            br#"{"rate_limit":{"secret-looking-key":true}}"#
        ));
        assert!(!unknown_fields(
            Provider::Codex,
            br#"{"rate_limit":{"allowed":true}}"#
        ));
        assert!(unknown_fields(
            Provider::CommandCode,
            br#"{"credits":{"windowLimits":{"weekly":{"new":1}}}}"#
        ));
        let account = AccountQuota::failure(
            Provider::Codex,
            "test",
            crate::model::ErrorCode::CredentialInvalid,
        );
        let output = serde_json::to_string(&diagnostics(&account, true)).unwrap();
        assert!(!output.contains("secret-looking-key"));
        assert!(output.contains("credential_invalid"));
    }
}
