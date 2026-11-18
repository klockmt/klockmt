use std::cell::RefCell;
use std::collections::HashMap;

use crate::github::{ApiError, GithubApi};
use crate::json::{self, Json};

// Answers each exact path with a scripted body (Ok) or HTTP status (Err); unknown paths are 404.
pub struct FakeGithubApi {
    responses: HashMap<String, Result<String, u16>>,
    requested: RefCell<Vec<String>>,
}

impl FakeGithubApi {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            requested: RefCell::new(Vec::new()),
        }
    }

    pub fn with_body(mut self, path: &str, body: &str) -> Self {
        self.responses.insert(path.to_owned(), Ok(body.to_owned()));
        self
    }

    pub fn with_status(mut self, path: &str, code: u16) -> Self {
        self.responses.insert(path.to_owned(), Err(code));
        self
    }

    pub fn requested_paths(&self) -> Vec<String> {
        self.requested.borrow().clone()
    }
}

impl GithubApi for FakeGithubApi {
    fn get(&self, path: &str) -> Result<Json, ApiError> {
        self.requested.borrow_mut().push(path.to_owned());
        match self.responses.get(path) {
            Some(Ok(body)) => json::parse(body).map_err(ApiError::Malformed),
            Some(Err(code)) => Err(ApiError::Status {
                code: *code,
                path: path.to_owned(),
                body: String::new(),
            }),
            None => Err(ApiError::Status {
                code: 404,
                path: path.to_owned(),
                body: String::new(),
            }),
        }
    }
}
