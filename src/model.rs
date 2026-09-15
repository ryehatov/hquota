use crate::config::Provider;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QuotaReport {
    pub schema_version: u32,
    pub fetched_at: String,
    pub accounts: Vec<AccountQuota>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccountQuota {
    pub provider: Provider,
    pub account: String,
    pub status: Status,
    pub primary_scope: Option<String>,
    pub headroom_percent: Option<u8>,
    pub scopes: Vec<QuotaScope>,
    pub facts: Vec<QuotaFact>,
    pub error: Option<AccountError>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QuotaScope {
    pub id: String,
    pub role: Role,
    pub label: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Primary,
    Auxiliary,
    Unknown,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Credits,
    ProviderUnits,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    CredentialMissing,
    CredentialInvalid,
    AuthenticationRequired,
    Timeout,
    UpstreamUnavailable,
    UpstreamSchemaChanged,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccountError {
    pub code: ErrorCode,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum QuotaFact {
    RateWindow {
        scope: String,
        duration_seconds: Option<u64>,
        canonical_window: Option<String>,
        used_percent: u8,
        remaining_percent: u8,
        reset_at: Option<String>,
        used_value: Option<f64>,
        limit_value: Option<f64>,
        unit: Option<Unit>,
    },
    Balance {
        scope: String,
        value: Option<f64>,
        unit: Unit,
        unlimited: bool,
    },
    SpendLimit {
        scope: String,
        used_value: f64,
        limit_value: f64,
        unit: Unit,
        remaining_percent: Option<u8>,
        reset_at: Option<String>,
    },
    Availability {
        scope: String,
        allowed: bool,
        reason_code: Option<String>,
    },
}
pub fn timestamp(seconds: i64) -> Result<String, ErrorCode> {
    OffsetDateTime::from_unix_timestamp(seconds)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .ok_or(ErrorCode::UpstreamSchemaChanged)
}
pub fn used_percent(raw: f64) -> Result<u8, ErrorCode> {
    if !raw.is_finite() {
        return Err(ErrorCode::UpstreamSchemaChanged);
    }
    Ok(raw.ceil().clamp(0.0, 100.0) as u8)
}
pub fn canonical(duration: Option<u64>) -> Option<String> {
    match duration {
        Some(18000) => Some("5h".into()),
        Some(604800) => Some("7d".into()),
        _ => None,
    }
}
impl QuotaFact {
    pub fn scope(&self) -> &str {
        match self {
            Self::RateWindow { scope, .. }
            | Self::Balance { scope, .. }
            | Self::SpendLimit { scope, .. }
            | Self::Availability { scope, .. } => scope,
        }
    }
    pub fn window(
        scope: &str,
        duration: Option<u64>,
        raw: f64,
        reset_at: Option<String>,
    ) -> Result<Self, ErrorCode> {
        let used_percent = used_percent(raw)?;
        Ok(Self::RateWindow {
            scope: scope.into(),
            duration_seconds: duration,
            canonical_window: canonical(duration),
            used_percent,
            remaining_percent: 100 - used_percent,
            reset_at,
            used_value: None,
            limit_value: None,
            unit: None,
        })
    }
}
impl AccountQuota {
    pub fn failure(provider: Provider, account: &str, code: ErrorCode) -> Self {
        Self {
            provider,
            account: account.into(),
            status: Status::Error,
            primary_scope: None,
            headroom_percent: None,
            scopes: vec![],
            facts: vec![],
            error: Some(AccountError { code }),
        }
    }
    pub fn success(
        provider: Provider,
        account: &str,
        scopes: Vec<QuotaScope>,
        facts: Vec<QuotaFact>,
    ) -> Result<Self, ErrorCode> {
        let primary_scope = scopes
            .iter()
            .find(|s| s.role == Role::Primary)
            .map(|s| s.id.clone());
        let headroom_percent = facts
            .iter()
            .filter_map(|f| match f {
                QuotaFact::RateWindow {
                    scope,
                    remaining_percent,
                    ..
                } if Some(scope) == primary_scope.as_ref() => Some(*remaining_percent),
                _ => None,
            })
            .min();
        let result = Self {
            provider,
            account: account.into(),
            status: Status::Ok,
            primary_scope,
            headroom_percent,
            scopes,
            facts,
            error: None,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), ErrorCode> {
        let bad = ErrorCode::UpstreamSchemaChanged;
        if !crate::config::valid_name(&self.account) {
            return Err(bad);
        }
        if self.status == Status::Error {
            return if self.error.is_some()
                && self.primary_scope.is_none()
                && self.headroom_percent.is_none()
                && self.scopes.is_empty()
                && self.facts.is_empty()
            {
                Ok(())
            } else {
                Err(bad)
            };
        }
        if self.error.is_some() {
            return Err(bad);
        }
        let mut ids = std::collections::HashSet::new();
        for scope in &self.scopes {
            if scope.id.is_empty() || !ids.insert(&scope.id) {
                return Err(bad);
            }
        }
        let primary: Vec<_> = self
            .scopes
            .iter()
            .filter(|s| s.role == Role::Primary)
            .collect();
        if primary.len() > 1 || self.primary_scope.as_ref() != primary.first().map(|s| &s.id) {
            return Err(bad);
        }
        let valid_time = |s: &Option<String>| {
            s.as_ref().is_none_or(|s| {
                OffsetDateTime::parse(s, &Rfc3339).is_ok_and(|t| t.offset().is_utc())
            })
        };
        for fact in &self.facts {
            if !ids.contains(&fact.scope().to_string()) {
                return Err(bad);
            }
            let valid = match fact {
                QuotaFact::RateWindow {
                    duration_seconds,
                    canonical_window,
                    used_percent,
                    remaining_percent,
                    reset_at,
                    used_value,
                    limit_value,
                    unit,
                    ..
                } => {
                    duration_seconds != &Some(0)
                        && *canonical_window == canonical(*duration_seconds)
                        && u16::from(*used_percent) + u16::from(*remaining_percent) == 100
                        && valid_time(reset_at)
                        && match (used_value, limit_value, unit) {
                            (None, None, None) => true,
                            (Some(u), Some(l), Some(_)) => u.is_finite() && l.is_finite(),
                            _ => false,
                        }
                }
                QuotaFact::Balance { value, .. } => value.is_none_or(f64::is_finite),
                QuotaFact::SpendLimit {
                    used_value,
                    limit_value,
                    remaining_percent,
                    reset_at,
                    ..
                } => {
                    used_value.is_finite()
                        && limit_value.is_finite()
                        && remaining_percent.is_none_or(|p| p <= 100)
                        && valid_time(reset_at)
                }
                QuotaFact::Availability { reason_code, .. } => {
                    reason_code.as_ref().is_none_or(|s| {
                        !s.is_empty()
                            && s.bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                    })
                }
            };
            if !valid {
                return Err(bad);
            }
        }
        let headroom = self
            .facts
            .iter()
            .filter_map(|f| match f {
                QuotaFact::RateWindow {
                    scope,
                    remaining_percent,
                    ..
                } if Some(scope) == self.primary_scope.as_ref() => Some(*remaining_percent),
                _ => None,
            })
            .min();
        if headroom != self.headroom_percent {
            return Err(bad);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_and_atomic_validation() {
        for (raw, expected) in [
            (30.0, 30),
            (30.1, 31),
            (100.0, 100),
            (-1.0, 0),
            (101.0, 100),
        ] {
            assert_eq!(used_percent(raw), Ok(expected));
        }
        for raw in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(used_percent(raw).is_err());
        }
        assert_eq!(canonical(Some(18000)).as_deref(), Some("5h"));
        assert_eq!(canonical(Some(604800)).as_deref(), Some("7d"));
        assert_eq!(canonical(Some(900)), None);
        let scopes = vec![
            QuotaScope {
                id: "codex".into(),
                role: Role::Primary,
                label: None,
            },
            QuotaScope {
                id: "extra".into(),
                role: Role::Auxiliary,
                label: None,
            },
        ];
        let facts = vec![
            QuotaFact::window("codex", Some(18000), 30.1, None).unwrap(),
            QuotaFact::window("codex", Some(604800), 50.0, None).unwrap(),
            QuotaFact::window("extra", Some(900), 100.0, None).unwrap(),
        ];
        let mut account = AccountQuota::success(Provider::Codex, "test", scopes, facts).unwrap();
        assert_eq!(account.headroom_percent, Some(50));
        account.headroom_percent = Some(49);
        assert!(account.validate().is_err());
        assert!(
            AccountQuota::failure(Provider::Codex, "test", ErrorCode::Timeout)
                .validate()
                .is_ok()
        );
    }
}
