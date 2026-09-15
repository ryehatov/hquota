use crate::{
    config::Provider,
    model::{AccountQuota, ErrorCode, QuotaFact, QuotaScope, Role, Unit, timestamp},
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Response {
    rate_limit: Option<Pool>,
    additional_rate_limits: Option<Vec<Additional>>,
    credits: Option<Credits>,
    spend_control: Option<Spend>,
    rate_limit_reached_type: Option<Reason>,
}
#[derive(Deserialize)]
struct Reason {
    r#type: String,
}
#[derive(Deserialize)]
struct Pool {
    allowed: bool,
    limit_reached: bool,
    primary_window: Option<Window>,
    secondary_window: Option<Window>,
}
#[derive(Deserialize)]
struct Window {
    used_percent: f64,
    limit_window_seconds: i64,
    reset_after_seconds: i64,
    reset_at: i64,
}
#[derive(Deserialize)]
struct Additional {
    limit_name: String,
    metered_feature: String,
    rate_limit: Option<Pool>,
}
#[derive(Deserialize)]
struct Credits {
    has_credits: bool,
    unlimited: bool,
    balance: Option<String>,
}
#[derive(Deserialize)]
struct Spend {
    reached: bool,
    individual_limit: Option<SpendLimit>,
}
#[derive(Deserialize)]
struct SpendLimit {
    limit: String,
    used: String,
    remaining: String,
    used_percent: i32,
    remaining_percent: i32,
    reset_after_seconds: i64,
    reset_at: i64,
}
fn number(text: &str) -> Result<f64, ErrorCode> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or(ErrorCode::UpstreamSchemaChanged)
}
fn pool(
    pool: Pool,
    id: &str,
    reason: Option<&str>,
    facts: &mut Vec<QuotaFact>,
) -> Result<(), ErrorCode> {
    for window in [pool.primary_window, pool.secondary_window]
        .into_iter()
        .flatten()
    {
        if window.limit_window_seconds <= 0 || window.reset_after_seconds < 0 {
            return Err(ErrorCode::UpstreamSchemaChanged);
        }
        facts.push(QuotaFact::window(
            id,
            Some(window.limit_window_seconds as u64),
            window.used_percent,
            Some(timestamp(window.reset_at)?),
        )?);
    }
    let reason = if pool.limit_reached {
        Some("rate_limit_reached")
    } else {
        reason.map(|r| {
            if r == "rate_limit_reached" {
                r
            } else {
                "provider_restriction"
            }
        })
    };
    facts.push(QuotaFact::Availability {
        scope: id.into(),
        allowed: pool.allowed,
        reason_code: reason.map(str::to_owned),
    });
    Ok(())
}
pub fn normalize(bytes: &[u8], account: &str) -> Result<AccountQuota, ErrorCode> {
    let response: Response =
        serde_json::from_slice(bytes).map_err(|_| ErrorCode::UpstreamSchemaChanged)?;
    let mut scopes = vec![];
    let mut facts = vec![];
    if let Some(ordinary) = response.rate_limit {
        scopes.push(QuotaScope {
            id: "codex".into(),
            role: Role::Primary,
            label: None,
        });
        pool(
            ordinary,
            "codex",
            response
                .rate_limit_reached_type
                .as_ref()
                .map(|r| r.r#type.as_str()),
            &mut facts,
        )?;
    }
    for (index, additional) in response
        .additional_rate_limits
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let raw = if additional.metered_feature.is_empty() {
            &additional.limit_name
        } else {
            &additional.metered_feature
        };
        let safe = raw.to_ascii_lowercase().replace(['_', ' '], "-");
        let id = if crate::config::valid_name(&safe)
            && !["codex", "extra-credits", "individual-spend-control"].contains(&safe.as_str())
        {
            safe
        } else {
            format!("additional-{}", index + 1)
        };
        scopes.push(QuotaScope {
            id: id.clone(),
            role: Role::Auxiliary,
            label: Some(additional.limit_name),
        });
        if let Some(extra) = additional.rate_limit {
            pool(extra, &id, None, &mut facts)?;
        }
    }
    if let Some(credits) = response.credits {
        scopes.push(QuotaScope {
            id: "extra-credits".into(),
            role: Role::Auxiliary,
            label: None,
        });
        facts.push(QuotaFact::Balance {
            scope: "extra-credits".into(),
            value: credits.balance.as_deref().map(number).transpose()?,
            unit: Unit::Credits,
            unlimited: credits.unlimited,
        });
        let _ = credits.has_credits;
    }
    if let Some(spend) = response.spend_control {
        let _ = spend.reached;
        if let Some(limit) = spend.individual_limit {
            if !(0..=100).contains(&limit.used_percent)
                || !(0..=100).contains(&limit.remaining_percent)
                || limit.used_percent + limit.remaining_percent != 100
                || limit.reset_after_seconds < 0
            {
                return Err(ErrorCode::UpstreamSchemaChanged);
            }
            number(&limit.remaining)?;
            scopes.push(QuotaScope {
                id: "individual-spend-control".into(),
                role: Role::Auxiliary,
                label: None,
            });
            facts.push(QuotaFact::SpendLimit {
                scope: "individual-spend-control".into(),
                used_value: number(&limit.used)?,
                limit_value: number(&limit.limit)?,
                unit: Unit::ProviderUnits,
                remaining_percent: Some(limit.remaining_percent as u8),
                reset_at: Some(timestamp(limit.reset_at)?),
            });
        }
    }
    AccountQuota::success(Provider::Codex, account, scopes, facts)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn missing_windows_and_atomic_drift() {
        let good = json!({"rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":30.1,"limit_window_seconds":18000,"reset_after_seconds":500,"reset_at":1780000000}},"credits":{"has_credits":true,"unlimited":false,"balance":"12.5"},"unknown":true});
        let account = normalize(&serde_json::to_vec(&good).unwrap(), "test").unwrap();
        assert_eq!(account.headroom_percent, Some(69));
        assert_eq!(account.facts.len(), 3);
        for (key, value) in [
            ("used_percent", json!("30")),
            ("reset_at", json!("later")),
            ("limit_window_seconds", json!(0)),
        ] {
            let mut bad = good.clone();
            bad["rate_limit"]["primary_window"][key] = value;
            assert!(normalize(&serde_json::to_vec(&bad).unwrap(), "test").is_err());
        }
        let extra=br#"{"additional_rate_limits":[{"metered_feature":"reserve","limit_name":"Reserve","rate_limit":{"allowed":true,"limit_reached":false}}]}"#;
        assert_eq!(normalize(extra, "test").unwrap().primary_scope, None);
        assert!(normalize(b"{}", "test").is_ok());
    }
}
