use crate::model::{QuotaFact, QuotaReport, Role, Status};
use std::fmt::Write;
fn bar(percent: u8) -> String {
    let filled = (usize::from(percent) + 5) / 10;
    format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
}
fn safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
pub fn human(report: &QuotaReport) -> String {
    let mut output = format!("Quota · {}\n", report.fetched_at);
    let mut providers = Vec::new();
    for account in &report.accounts {
        if !providers.contains(&account.provider) {
            providers.push(account.provider);
        }
    }
    for provider in providers {
        writeln!(output, "\n{}", provider.as_str()).unwrap();
        for account in report.accounts.iter().filter(|a| a.provider == provider) {
            writeln!(output, "  {}", safe(&account.account)).unwrap();
            if account.status == Status::Error {
                let code = serde_json::to_value(account.error.as_ref().unwrap().code).unwrap();
                writeln!(output, "    error: {}", code.as_str().unwrap()).unwrap();
                continue;
            }
            let mut scopes: Vec<_> = account.scopes.iter().collect();
            scopes.sort_by_key(|s| match s.role {
                Role::Primary => 0,
                Role::Auxiliary => 1,
                Role::Unknown => 2,
            });
            for scope in scopes {
                writeln!(
                    output,
                    "    {}",
                    safe(scope.label.as_deref().unwrap_or(&scope.id))
                )
                .unwrap();
                let mut facts: Vec<_> = account
                    .facts
                    .iter()
                    .filter(|f| f.scope() == scope.id)
                    .collect();
                facts.sort_by_key(|f| match f {
                    QuotaFact::RateWindow {
                        canonical_window,
                        duration_seconds,
                        ..
                    } => (
                        match canonical_window.as_deref() {
                            Some("5h") => 0,
                            Some("7d") => 1,
                            _ => 2,
                        },
                        duration_seconds.unwrap_or(u64::MAX),
                    ),
                    QuotaFact::Balance { .. } => (3, 0),
                    QuotaFact::SpendLimit { .. } => (4, 0),
                    QuotaFact::Availability { .. } => (5, 0),
                });
                for fact in facts {
                    match fact {
                        QuotaFact::RateWindow{canonical_window,duration_seconds,remaining_percent,reset_at,..}=>{
                            let window=canonical_window.clone().unwrap_or_else(||duration_seconds.map(|d|format!("{d}s")).unwrap_or_else(||"window".into()));
                            write!(output,"      {window} {} {remaining_percent}% left",bar(*remaining_percent)).unwrap();
                            if let Some(reset)=reset_at {write!(output," · reset {reset}").unwrap();}output.push('\n');
                        },
                        QuotaFact::Balance{value,unit,unlimited,..}=>writeln!(output,"      balance {} {:?}{}",value.map(|v|v.to_string()).unwrap_or_else(||"unknown".into()),unit,if *unlimited {" (unlimited)"}else{""}).unwrap(),
                        QuotaFact::SpendLimit{used_value,limit_value,unit,remaining_percent,reset_at,..}=>writeln!(output,"      spend {used_value}/{limit_value} {unit:?} · remaining {} · reset {}",remaining_percent.map(|p|format!("{p}%")).unwrap_or_else(||"unknown".into()),reset_at.as_deref().unwrap_or("unknown")).unwrap(),
                        QuotaFact::Availability{allowed,reason_code,..}=>writeln!(output,"      allowed {allowed}{}",reason_code.as_ref().map(|r|format!(" ({})",safe(r))).unwrap_or_default()).unwrap(),
                    }
                }
            }
            if let Some(headroom) = account.headroom_percent {
                writeln!(output, "    headroom {headroom}%").unwrap();
            }
        }
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_bar_and_empty_report() {
        assert_eq!(bar(78), "████████░░");
        assert!(
            human(&QuotaReport {
                schema_version: 1,
                fetched_at: "2026-09-15T00:00:00Z".into(),
                accounts: vec![]
            })
            .contains("Quota")
        );
    }
}
