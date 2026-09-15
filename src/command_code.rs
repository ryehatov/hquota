use crate::{
    config::Provider,
    model::{AccountQuota, ErrorCode, QuotaFact, QuotaScope, Role, Unit, timestamp},
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    credits: Credits,
    window_limits: Option<Windows>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credits {
    monthly_credits: Option<f64>,
    purchased_credits: Option<f64>,
    free_credits: Option<f64>,
    premium_monthly_credits: Option<f64>,
    opensource_monthly_credits: Option<f64>,
    window_limits: Option<Windows>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Windows {
    five_hour: Option<Window>,
    weekly: Option<Window>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    cap: f64,
    used: f64,
    reset_at: Option<Reset>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Reset {
    Epoch(i64),
    Text(String),
}
impl Reset {
    fn normalize(self) -> Result<String, ErrorCode> {
        match self {
            Self::Epoch(value) => timestamp(if value > 10_000_000_000 {
                value / 1000
            } else {
                value
            }),
            Self::Text(text) => {
                let time = time::OffsetDateTime::parse(
                    &text,
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|_| ErrorCode::UpstreamSchemaChanged)?;
                timestamp(time.unix_timestamp())
            }
        }
    }
}
pub fn normalize(bytes: &[u8], account: &str) -> Result<AccountQuota, ErrorCode> {
    let response: Response =
        serde_json::from_slice(bytes).map_err(|_| ErrorCode::UpstreamSchemaChanged)?;
    let mut scopes = vec![];
    let mut facts = vec![];
    // Both observed locations are parsed strictly; conflicting simultaneous pools are ambiguous.
    if response.window_limits.is_some() && response.credits.window_limits.is_some() {
        return Err(ErrorCode::UpstreamSchemaChanged);
    }
    if let Some(windows) = response.window_limits.or(response.credits.window_limits) {
        scopes.push(QuotaScope {
            id: "included".into(),
            role: Role::Primary,
            label: None,
        });
        for (window, duration) in [(windows.five_hour, 18000), (windows.weekly, 604800)] {
            if let Some(window) = window {
                if !window.cap.is_finite()
                    || window.cap <= 0.0
                    || !window.used.is_finite()
                    || window.used < 0.0
                {
                    return Err(ErrorCode::UpstreamSchemaChanged);
                }
                let mut fact = QuotaFact::window(
                    "included",
                    Some(duration),
                    100.0 * (window.used / window.cap),
                    window.reset_at.map(Reset::normalize).transpose()?,
                )?;
                if let QuotaFact::RateWindow {
                    used_value,
                    limit_value,
                    unit,
                    ..
                } = &mut fact
                {
                    *used_value = Some(window.used);
                    *limit_value = Some(window.cap);
                    *unit = Some(Unit::Credits);
                }
                facts.push(fact);
            }
        }
    }
    // Monthly/premium/open-source amounts have ambiguous granted-versus-remaining semantics.
    for value in [
        response.credits.monthly_credits,
        response.credits.premium_monthly_credits,
        response.credits.opensource_monthly_credits,
    ] {
        if value.is_some_and(|v| !v.is_finite()) {
            return Err(ErrorCode::UpstreamSchemaChanged);
        }
    }
    for (id, value) in [
        ("purchased-credits", response.credits.purchased_credits),
        ("free-credits", response.credits.free_credits),
    ] {
        if let Some(value) = value {
            scopes.push(QuotaScope {
                id: id.into(),
                role: Role::Auxiliary,
                label: None,
            });
            facts.push(QuotaFact::Balance {
                scope: id.into(),
                value: Some(value),
                unit: Unit::Credits,
                unlimited: false,
            });
        }
    }
    AccountQuota::success(Provider::CommandCode, account, scopes, facts)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn fractional_windows_and_atomic_drift() {
        let fixture = json!({"credits":{"monthlyCredits":8.7784,"purchasedCredits":2.0,"freeCredits":1.0},"windowLimits":{"fiveHour":{"cap":3.0,"used":0.751,"resetAt":1780000000000_i64},"weekly":{"cap":15.0,"used":1.5}},"additive":true});
        let account = normalize(&serde_json::to_vec(&fixture).unwrap(), "test").unwrap();
        assert_eq!(account.headroom_percent, Some(74));
        assert_eq!(account.facts.len(), 4);
        for value in [json!(0), json!(-1), json!("3"), json!(null)] {
            let mut bad = fixture.clone();
            bad["windowLimits"]["fiveHour"]["cap"] = value;
            assert!(normalize(&serde_json::to_vec(&bad).unwrap(), "test").is_err());
        }
        assert!(
            normalize(
                br#"{"credits":{},"windowLimits":{"fiveHour":{"cap":5}}}"#,
                "test"
            )
            .is_err()
        );
        assert!(
            normalize(br#"{"credits":{}}"#, "test")
                .unwrap()
                .facts
                .is_empty()
        );
    }
}
