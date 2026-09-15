use crate::model::ErrorCode;
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, HeaderValue},
};
use serde::Deserialize;
use std::{path::Path, time::Duration};

pub const CODEX_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const COMMAND_CODE_URL: &str = "https://api.commandcode.ai/alpha/billing/credits";

pub struct Credential {
    authorization: HeaderValue,
    account_id: Option<HeaderValue>,
}

fn read(path: &Path) -> Result<Vec<u8>, ErrorCode> {
    std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::CredentialMissing
        } else {
            ErrorCode::CredentialInvalid
        }
    })
}
fn bearer(token: &str) -> Result<HeaderValue, ErrorCode> {
    let mut header = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| ErrorCode::CredentialInvalid)?;
    header.set_sensitive(true);
    Ok(header)
}
pub fn codex_credential(path: &Path) -> Result<Credential, ErrorCode> {
    parse_codex(&read(path)?)
}
fn parse_codex(bytes: &[u8]) -> Result<Credential, ErrorCode> {
    #[derive(Deserialize)]
    struct Auth {
        tokens: Tokens,
    }
    #[derive(Deserialize)]
    struct Tokens {
        access_token: String,
        account_id: String,
    }
    let auth: Auth = serde_json::from_slice(bytes).map_err(|_| ErrorCode::CredentialInvalid)?;
    if auth.tokens.access_token.is_empty() || auth.tokens.account_id.is_empty() {
        return Err(ErrorCode::CredentialInvalid);
    }
    let mut id =
        HeaderValue::from_str(&auth.tokens.account_id).map_err(|_| ErrorCode::CredentialInvalid)?;
    id.set_sensitive(true);
    Ok(Credential {
        authorization: bearer(&auth.tokens.access_token)?,
        account_id: Some(id),
    })
}
pub fn command_credential(path: &Path) -> Result<Credential, ErrorCode> {
    parse_command(&read(path)?)
}
fn parse_command(bytes: &[u8]) -> Result<Credential, ErrorCode> {
    let key = std::str::from_utf8(bytes)
        .map_err(|_| ErrorCode::CredentialInvalid)?
        .trim_matches(|c: char| c.is_ascii_whitespace());
    if key.is_empty() || key.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(ErrorCode::CredentialInvalid);
    }
    Ok(Credential {
        authorization: bearer(key)?,
        account_id: None,
    })
}
pub fn client() -> Result<Client, ErrorCode> {
    Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .https_only(true)
        .retry(reqwest::retry::never())
        .user_agent(concat!("hquota/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| ErrorCode::UpstreamUnavailable)
}
pub fn fetch(
    client: &Client,
    provider: crate::config::Provider,
    credential: Credential,
) -> Result<Vec<u8>, ErrorCode> {
    let url = match provider {
        crate::config::Provider::Codex => CODEX_URL,
        crate::config::Provider::CommandCode => COMMAND_CODE_URL,
    };
    let mut request = client
        .get(url)
        .header(AUTHORIZATION, credential.authorization)
        .header("Accept", "application/json");
    if let Some(id) = credential.account_id {
        request = request.header("ChatGPT-Account-Id", id);
    }
    let response = request.send().map_err(transport_error)?;
    match response.status().as_u16() {
        200..=299 => response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(transport_error),
        401 | 403 => Err(ErrorCode::AuthenticationRequired),
        _ => Err(ErrorCode::UpstreamUnavailable),
    }
}
fn transport_error(error: reqwest::Error) -> ErrorCode {
    if error.is_timeout() {
        ErrorCode::Timeout
    } else {
        ErrorCode::UpstreamUnavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_credentials_and_sanitized_failures() {
        assert!(parse_codex(br#"{"tokens":{"access_token":"synthetic-token","account_id":"synthetic-id","refresh_token":"ignored"},"extra":true}"#).is_ok());
        for bytes in [
            b"not-json".as_slice(),
            br#"{"tokens":{"access_token":"synthetic"}}"#,
            br#"{"tokens":{"access_token":"","account_id":"synthetic"}}"#,
        ] {
            assert!(matches!(
                parse_codex(bytes),
                Err(ErrorCode::CredentialInvalid)
            ));
        }
        assert!(parse_command(b" \tsynthetic-key\r\n").is_ok());
        for bytes in [b"".as_slice(), b" \n", b"two keys", b"two\nkeys", &[255]] {
            assert!(matches!(
                parse_command(bytes),
                Err(ErrorCode::CredentialInvalid)
            ));
        }
        assert_eq!(
            serde_json::to_string(&ErrorCode::CredentialInvalid).unwrap(),
            "\"credential_invalid\""
        );
    }
    #[test]
    fn https_only_rejects_plain_http() {
        let error = client()
            .unwrap()
            .get("http://127.0.0.1:1/")
            .send()
            .unwrap_err();
        assert_eq!(transport_error(error), ErrorCode::UpstreamUnavailable);
    }
}
