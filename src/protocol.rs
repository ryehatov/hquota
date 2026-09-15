use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub const SOCKET: &str = "/run/hquota/hquota.sock";
pub const IO_TIMEOUT: Duration = Duration::from_secs(15);
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Quota,
    Health,
    Doctor,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub op: Operation,
}
#[derive(Deserialize, Serialize)]
pub struct Response {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub result: Outcome,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Ok { payload: Value },
    Error { error: ProtocolError },
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    pub code: String,
}
impl Response {
    pub fn ok(payload: Value) -> Self {
        Self {
            protocol_version: 1,
            result: Outcome::Ok { payload },
        }
    }
    pub fn error(code: &str) -> Self {
        Self {
            protocol_version: 1,
            result: Outcome::Error {
                error: ProtocolError { code: code.into() },
            },
        }
    }
}
pub fn configure(stream: &UnixStream, timeout: Duration) -> Result<(), &'static str> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "socket_io_failed")?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "socket_io_failed")
}
pub fn read_json<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
) -> Result<T, &'static str> {
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|_| "socket_io_failed")?;
    serde_json::from_slice(&bytes).map_err(|_| "invalid_protocol_json")
}
pub fn write_json<T: Serialize>(stream: &mut UnixStream, value: &T) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(value).map_err(|_| "serialization_failed")?;
    stream.write_all(&bytes).map_err(|_| "socket_io_failed")?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|_| "socket_io_failed")
}
pub fn request(path: &Path, op: Operation) -> Result<Value, &'static str> {
    let stream = UnixStream::connect(path).map_err(|_| "broker_unavailable")?;
    exchange(stream, op, IO_TIMEOUT)
}
pub fn exchange(
    mut stream: UnixStream,
    op: Operation,
    timeout: Duration,
) -> Result<Value, &'static str> {
    configure(&stream, timeout)?;
    write_json(
        &mut stream,
        &Request {
            protocol_version: 1,
            op,
        },
    )?;
    // Check the envelope version before decoding the payload or error shape.
    let raw: Value = read_json(&mut stream)?;
    if raw.get("protocol_version").and_then(Value::as_u64) != Some(1) {
        return Err("protocol_version_mismatch");
    }
    let response: Response =
        serde_json::from_value(raw).map_err(|_| "invalid_protocol_response")?;
    match response.result {
        Outcome::Ok { payload } => Ok(payload),
        Outcome::Error { .. } => Err("broker_request_failed"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_response_eof_and_version() {
        for version in [1, 2] {
            let (client, mut server) = UnixStream::pair().unwrap();
            let thread = std::thread::spawn(move || {
                let req: Request = read_json(&mut server).unwrap();
                assert_eq!(req.protocol_version, 1);
                write_json(&mut server,&serde_json::json!({"protocol_version":version,"status":"ok","payload":{"status":"ok"}})).unwrap();
            });
            let result = exchange(client, Operation::Health, Duration::from_secs(1));
            if version == 1 {
                assert!(result.is_ok(), "{result:?}");
            } else {
                assert_eq!(result, Err("protocol_version_mismatch"));
            }
            thread.join().unwrap();
        }
    }
    #[test]
    fn client_read_timeout() {
        let (client, _server) = UnixStream::pair().unwrap();
        assert_eq!(
            exchange(client, Operation::Health, Duration::from_millis(20)),
            Err("socket_io_failed")
        );
    }
}
