use std::fmt;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::json::{self, Json};

pub const API_ROOT: &str = "https://api.github.com";

#[derive(Debug)]
pub enum ApiError {
    Status {
        code: u16,
        path: String,
        body: String,
    },
    Transport(String),
    Malformed(String),
}

impl ApiError {
    pub fn is_skippable(&self) -> bool {
        matches!(
            self,
            ApiError::Status {
                code: 404 | 409,
                ..
            }
        )
    }

    fn is_server_error(&self) -> bool {
        matches!(self, ApiError::Status { code, .. } if *code >= 500)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Status { code, path, body } => {
                write!(formatter, "GitHub {code} for {path}: {body}")
            }
            ApiError::Transport(detail) => write!(formatter, "request failed: {detail}"),
            ApiError::Malformed(detail) => write!(formatter, "malformed response: {detail}"),
        }
    }
}

pub trait GithubApi {
    fn get(&self, path: &str) -> Result<Json, ApiError>;
}

pub struct CurlApi {
    token: String,
}

impl CurlApi {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    fn headers(&self) -> String {
        format!(
            "Authorization: Bearer {}\nAccept: application/vnd.github+json\n",
            self.token
        )
    }
}

impl GithubApi for CurlApi {
    fn get(&self, path: &str) -> Result<Json, ApiError> {
        let mut child = Command::new("curl")
            .args(["-sS", "-w", "\n%{http_code}", "-H", "@-"])
            .arg(format!("{API_ROOT}{path}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ApiError::Transport(error.to_string()))?;
        write_headers(&mut child, &self.headers())?;
        let output = child
            .wait_with_output()
            .map_err(|error| ApiError::Transport(error.to_string()))?;
        if !output.status.success() {
            return Err(ApiError::Transport(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        parse_response(path, &String::from_utf8_lossy(&output.stdout))
    }
}

fn write_headers(child: &mut std::process::Child, headers: &str) -> Result<(), ApiError> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| ApiError::Transport("curl stdin unavailable".to_owned()))?;
    stdin
        .write_all(headers.as_bytes())
        .map_err(|error| ApiError::Transport(error.to_string()))
}

pub fn parse_response(path: &str, raw: &str) -> Result<Json, ApiError> {
    let (body, code_line) = raw
        .rsplit_once('\n')
        .ok_or_else(|| ApiError::Malformed("missing status line".to_owned()))?;
    let code: u16 = code_line
        .trim()
        .parse()
        .map_err(|_| ApiError::Malformed(format!("invalid status {code_line}")))?;
    if code >= 400 {
        return Err(ApiError::Status {
            code,
            path: path.to_owned(),
            body: body.to_owned(),
        });
    }
    json::parse(body).map_err(ApiError::Malformed)
}

pub struct RetryingApi<A: GithubApi> {
    inner: A,
    attempts: u32,
    pause: fn(Duration),
}

impl<A: GithubApi> RetryingApi<A> {
    pub fn new(inner: A, attempts: u32, pause: fn(Duration)) -> Self {
        Self {
            inner,
            attempts,
            pause,
        }
    }
}

impl<A: GithubApi> GithubApi for RetryingApi<A> {
    fn get(&self, path: &str) -> Result<Json, ApiError> {
        let mut attempt: u32 = 0;
        loop {
            match self.inner.get(path) {
                Err(error) if error.is_server_error() && attempt + 1 < self.attempts => {
                    (self.pause)(Duration::from_secs(1 << attempt));
                    attempt += 1;
                }
                outcome => return outcome,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    struct ScriptedServerErrors {
        failures_left: Cell<u32>,
        status: u16,
        calls: Cell<u32>,
    }

    impl GithubApi for ScriptedServerErrors {
        fn get(&self, path: &str) -> Result<Json, ApiError> {
            self.calls.set(self.calls.get() + 1);
            if self.failures_left.get() == 0 {
                return Ok(Json::Null);
            }
            self.failures_left.set(self.failures_left.get() - 1);
            Err(ApiError::Status {
                code: self.status,
                path: path.to_owned(),
                body: "boom".to_owned(),
            })
        }
    }

    thread_local! {
        static PAUSES: RefCell<Vec<Duration>> = const { RefCell::new(Vec::new()) };
    }

    fn record_pause(duration: Duration) {
        PAUSES.with(|pauses| pauses.borrow_mut().push(duration));
    }

    #[test]
    fn retries_server_errors_with_doubling_pause() {
        PAUSES.with(|pauses| pauses.borrow_mut().clear());
        let scripted = ScriptedServerErrors {
            failures_left: Cell::new(2),
            status: 500,
            calls: Cell::new(0),
        };
        let retrying = RetryingApi::new(scripted, 4, record_pause);
        assert_eq!(retrying.get("/x").unwrap(), Json::Null);
        assert_eq!(retrying.inner.calls.get(), 3);
        PAUSES.with(|pauses| {
            assert_eq!(
                *pauses.borrow(),
                vec![Duration::from_secs(1), Duration::from_secs(2)]
            )
        });
    }

    #[test]
    fn gives_up_after_last_attempt() {
        let scripted = ScriptedServerErrors {
            failures_left: Cell::new(10),
            status: 502,
            calls: Cell::new(0),
        };
        let retrying = RetryingApi::new(scripted, 3, record_pause);
        assert!(retrying.get("/x").is_err());
        assert_eq!(retrying.inner.calls.get(), 3);
    }

    #[test]
    fn does_not_retry_client_errors() {
        let scripted = ScriptedServerErrors {
            failures_left: Cell::new(10),
            status: 404,
            calls: Cell::new(0),
        };
        let retrying = RetryingApi::new(scripted, 4, record_pause);
        assert!(retrying.get("/x").is_err());
        assert_eq!(retrying.inner.calls.get(), 1);
    }

    #[test]
    fn parse_response_splits_body_and_status() {
        let parsed = parse_response("/x", "{\"a\":1}\n200").unwrap();
        assert_eq!(parsed.get("a"), Some(&Json::Number(1.0)));
    }

    #[test]
    fn parse_response_reports_github_error_body() {
        let error =
            parse_response("/repos/a/b", "{\"message\":\"Server Error\"}\n500").unwrap_err();
        assert_eq!(
            error.to_string(),
            "GitHub 500 for /repos/a/b: {\"message\":\"Server Error\"}"
        );
        assert!(error.is_server_error());
    }

    #[test]
    fn skippable_covers_missing_and_empty_repositories() {
        let missing = ApiError::Status {
            code: 404,
            path: String::new(),
            body: String::new(),
        };
        let empty = ApiError::Status {
            code: 409,
            path: String::new(),
            body: String::new(),
        };
        let forbidden = ApiError::Status {
            code: 403,
            path: String::new(),
            body: String::new(),
        };
        assert!(missing.is_skippable());
        assert!(empty.is_skippable());
        assert!(!forbidden.is_skippable());
    }
}
