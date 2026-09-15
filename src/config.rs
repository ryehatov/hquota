use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Codex,
    CommandCode,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::CommandCode => "command-code",
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AccountConfig {
    Codex { name: String, auth_json: PathBuf },
    CommandCode { name: String, api_key_file: PathBuf },
}

impl AccountConfig {
    pub fn provider(&self) -> Provider {
        match self {
            Self::Codex { .. } => Provider::Codex,
            Self::CommandCode { .. } => Provider::CommandCode,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Codex { name, .. } | Self::CommandCode { name, .. } => name,
        }
    }
    pub fn credential_path(&self) -> &Path {
        match self {
            Self::Codex { auth_json, .. } => auth_json,
            Self::CommandCode { api_key_file, .. } => api_key_file,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub accounts: Vec<AccountConfig>,
}

pub fn valid_name(name: &str) -> bool {
    let valid = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    name.bytes().next().is_some_and(valid) && name.bytes().all(|b| valid(b) || b == b'-')
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let config: Self = serde_json::from_slice(bytes).map_err(|_| "invalid_config")?;
        let mut identities = HashSet::new();
        if config.schema_version != 1 {
            return Err("invalid_config_version");
        }
        for account in &config.accounts {
            if !valid_name(account.name()) || !account.credential_path().is_absolute() {
                return Err("invalid_account_config");
            }
            if !identities.insert((account.provider(), account.name())) {
                return Err("duplicate_account");
            }
        }
        Ok(config)
    }
    pub fn load(path: &Path) -> Result<Self, &'static str> {
        Self::parse(&std::fs::read(path).map_err(|_| "config_unreadable")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn parse(value: Value) -> Result<Config, &'static str> {
        Config::parse(&serde_json::to_vec(&value).unwrap())
    }
    #[test]
    fn strict_configuration() {
        assert!(parse(json!({"schema_version":1,"accounts":[]})).is_ok());
        let codex = json!({"provider":"codex","name":"business","auth_json":"/not-read/auth.json"});
        let command =
            json!({"provider":"command-code","name":"business","api_key_file":"/not-read/key"});
        let good = json!({"schema_version":1,"accounts":[codex,command]});
        let config = parse(good.clone()).unwrap();
        assert_eq!(config.accounts[0].provider(), Provider::Codex);
        assert_eq!(config.accounts[1].provider(), Provider::CommandCode);
        for (pointer, value) in [
            ("/schema_version", json!(2)),
            ("/schema_version", json!(1.0)),
            ("/accounts/0/name", json!("Bad")),
            ("/accounts/0/name", json!("-bad")),
            ("/accounts/0/name", json!("")),
            ("/accounts/0/auth_json", json!("relative")),
            ("/accounts/0/provider", json!("unknown")),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(parse(bad).is_err(), "{pointer}");
        }
        for pointer in ["", "/accounts/0", "/accounts/1"] {
            let mut bad = good.clone();
            bad.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(true));
            assert!(parse(bad).is_err());
        }
        for (index, key) in [(0, "api_key_file"), (1, "auth_json")] {
            let mut bad = good.clone();
            bad["accounts"][index][key] = json!("/wrong");
            assert!(parse(bad).is_err());
        }
        assert!(
            parse(json!({"schema_version":1,"accounts":[good["accounts"][0],good["accounts"][0]]}))
                .is_err()
        );
        assert!(
            Config::parse(br#"{"schema_version":1,"schema_version":1,"accounts":[]}"#).is_err()
        );
    }
}
