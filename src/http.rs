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
fn builder() -> reqwest::blocking::ClientBuilder {
    Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .https_only(true)
        .retry(reqwest::retry::never())
        .user_agent(concat!("hquota/", env!("CARGO_PKG_VERSION")))
}
pub fn client() -> Result<Client, ErrorCode> {
    builder()
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
    response_body(request.send().map_err(transport_error)?)
}
fn response_body(response: reqwest::blocking::Response) -> Result<Vec<u8>, ErrorCode> {
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
    fn read_request(stream: &mut std::net::TcpStream) {
        use std::io::Read;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            assert!(head.len() < 4096);
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
    }
    #[test]
    fn redirects_and_error_bodies_are_not_exposed() {
        use std::{io::Write, net::TcpListener};
        for status in [302, 401, 403, 500] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                stream.write_all(format!("HTTP/1.1 {status} Test\r\nLocation: http://127.0.0.1:1/forbidden\r\nContent-Length: 14\r\nConnection: close\r\n\r\nprivate-marker").as_bytes()).unwrap();
            });
            // Only the test disables HTTPS for a local plaintext fixture server.
            let response = builder()
                .https_only(false)
                .build()
                .unwrap()
                .get(format!("http://{address}"))
                .send()
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            let result = response_body(response);
            assert_eq!(
                result,
                Err(if status == 401 || status == 403 {
                    ErrorCode::AuthenticationRequired
                } else {
                    ErrorCode::UpstreamUnavailable
                })
            );
            assert!(
                !serde_json::to_string(&result.unwrap_err())
                    .unwrap()
                    .contains("private-marker")
            );
            server.join().unwrap();
        }
    }
    #[test]
    fn proxy_environment_child() {
        let Ok(address) = std::env::var("HQUOTA_TEST_PROXY_TARGET") else {
            return;
        };
        let address: std::net::SocketAddr = address.parse().unwrap();
        let response = builder()
            .https_only(false)
            .resolve("fixture.invalid", address)
            .build()
            .unwrap()
            .get(format!("http://fixture.invalid:{}/", address.port()))
            .send()
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
    }
    #[test]
    fn environment_proxy_is_disabled() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "http::tests::proxy_environment_child"])
            .env("HQUOTA_TEST_PROXY_TARGET", address.to_string())
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("http_proxy", "http://127.0.0.1:1")
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .env("all_proxy", "http://127.0.0.1:1")
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    read_request(&mut stream);
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait().unwrap() {
                        panic!("proxy test child exited before direct request: {status}");
                    }
                    if start.elapsed() > Duration::from_secs(5) {
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("proxy test timed out");
                    }
                    std::thread::yield_now();
                }
                Err(e) => panic!("{e}"),
            }
        }
        assert!(child.wait().unwrap().success());
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
