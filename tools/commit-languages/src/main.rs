mod collect;
#[cfg(test)]
mod fake;
mod github;
mod json;
mod svg;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;

use collect::{count_commits_per_language, list_authored_commits, top_languages};
use github::{CurlApi, RetryingApi};

const DEFAULT_OUTPUT_DIR: &str = "dist";
const DEFAULT_AUTHOR: &str = "klockmt";
const MAX_ATTEMPTS: u32 = 4;
const TOP_LANGUAGES: usize = 5;

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let output_dir = PathBuf::from(
        env::args()
            .nth(1)
            .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_owned()),
    );
    let token = env::var("PROFILE_TOKEN").map_err(|_| "PROFILE_TOKEN is not set".to_owned())?;
    let author = env::var("COMMIT_AUTHOR").unwrap_or_else(|_| DEFAULT_AUTHOR.to_owned());
    let api = RetryingApi::new(CurlApi::new(token), MAX_ATTEMPTS, thread::sleep);
    let commits = list_authored_commits(&author, &api).map_err(|error| error.to_string())?;
    let counts = count_commits_per_language(&commits, &api).map_err(|error| error.to_string())?;
    let top = top_languages(&counts, TOP_LANGUAGES);
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    fs::write(
        output_dir.join("commit-languages.svg"),
        svg::render(&top, commits.len()),
    )
    .map_err(|error| error.to_string())?;
    Ok(format!(
        "{} commits, {} languages",
        commits.len(),
        counts.len()
    ))
}
