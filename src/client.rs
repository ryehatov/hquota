use crate::{
    cli::Command,
    doctor::{Diagnostic, Severity},
    model::QuotaReport,
    protocol::{self, Operation},
};
use std::path::Path;

pub fn run(command: Command, path: &Path) -> Result<(String, bool), &'static str> {
    match command {
        Command::Quota {
            json,
            provider,
            account,
        } => {
            let payload = protocol::request(path, Operation::Quota)?;
            let mut report = report(payload)?;
            report.accounts.retain(|a| {
                provider.is_none_or(|p| p == a.provider)
                    && account.as_ref().is_none_or(|name| name == &a.account)
            });
            let output = if json {
                serde_json::to_string(&report).map_err(|_| "serialization_failed")? + "\n"
            } else {
                crate::render::human(&report)
            };
            Ok((output, true))
        }
        Command::Doctor => {
            let diagnostics: Vec<Diagnostic> =
                serde_json::from_value(protocol::request(path, Operation::Doctor)?)
                    .map_err(|_| "invalid_diagnostic_response")?;
            let mut output = String::new();
            let mut success = true;
            for diagnostic in diagnostics {
                if !crate::config::valid_name(&diagnostic.account)
                    || ![
                        "checks_passed",
                        "unknown_provider_fields",
                        "credential_missing",
                        "credential_invalid",
                        "authentication_required",
                        "timeout",
                        "upstream_unavailable",
                        "upstream_schema_changed",
                    ]
                    .contains(&diagnostic.code.as_str())
                {
                    return Err("invalid_diagnostic_response");
                }
                let severity = match diagnostic.severity {
                    Severity::Ok => "ok",
                    Severity::Warning => "warning",
                    Severity::Error => {
                        success = false;
                        "error"
                    }
                };
                output.push_str(&format!(
                    "{}/{} {severity}: {}\n",
                    diagnostic.provider.as_str(),
                    diagnostic.account,
                    diagnostic.code
                ));
            }
            Ok((output, success))
        }
        Command::Serve { .. } => Err("invalid_client_command"),
    }
}
pub fn report(payload: serde_json::Value) -> Result<QuotaReport, &'static str> {
    if payload
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err("schema_version_mismatch");
    }
    let report: QuotaReport =
        serde_json::from_value(payload).map_err(|_| "invalid_quota_report")?;
    let fetched = time::OffsetDateTime::parse(
        &report.fetched_at,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| "invalid_quota_report")?;
    if !fetched.offset().is_utc() {
        return Err("invalid_quota_report");
    }
    let mut identities = std::collections::HashSet::new();
    for account in &report.accounts {
        account.validate().map_err(|_| "invalid_quota_report")?;
        if !identities.insert((account.provider, &account.account)) {
            return Err("invalid_quota_report");
        }
    }
    Ok(report)
}
