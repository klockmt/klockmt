use std::collections::{BTreeMap, BTreeSet};

use crate::github::{ApiError, GithubApi};
use crate::json::Json;

pub const PAGE_SIZE: usize = 100;
pub const MAX_PAGES: usize = 10;

const EXCLUDED_REPOS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommitRef {
    pub repo: String,
    pub sha: String,
}

pub fn list_authored_commits(
    author: &str,
    api: &dyn GithubApi,
) -> Result<Vec<CommitRef>, ApiError> {
    let mut unique: BTreeSet<CommitRef> = BTreeSet::new();
    unique.extend(search_public_commits(author, api)?);
    unique.extend(list_owned_repo_commits(author, api)?);
    Ok(unique.into_iter().collect())
}

pub fn count_commits_per_language(
    commits: &[CommitRef],
    api: &dyn GithubApi,
) -> Result<BTreeMap<&'static str, usize>, ApiError> {
    let mut counts = BTreeMap::new();
    for commit in commits {
        for language in languages_of_commit(commit, api)? {
            *counts.entry(language).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

pub fn top_languages<'a>(counts: &BTreeMap<&'a str, usize>, limit: usize) -> Vec<(&'a str, usize)> {
    let mut ranked: Vec<(&str, usize)> =
        counts.iter().map(|(name, count)| (*name, *count)).collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
    ranked.truncate(limit);
    ranked
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsTotals {
    pub commits: u64,
    pub pull_requests: u64,
    pub issues: u64,
    pub stars: u64,
    pub contributed_repos: u64,
}

pub fn collect_totals(
    author: &str,
    commits: &[CommitRef],
    api: &dyn GithubApi,
) -> Result<StatsTotals, ApiError> {
    Ok(StatsTotals {
        commits: commits.len() as u64,
        pull_requests: search_total(&format!("author:{author}+type:pr"), api)?,
        issues: search_total(&format!("author:{author}+type:issue"), api)?,
        stars: owned_star_count(author, api)?,
        contributed_repos: external_repo_count(author, commits),
    })
}

fn external_repo_count(author: &str, commits: &[CommitRef]) -> u64 {
    let prefix = format!("{author}/");
    let external: BTreeSet<&str> = commits
        .iter()
        .map(|commit| commit.repo.as_str())
        .filter(|repo| !repo.starts_with(&prefix))
        .collect();
    external.len() as u64
}

fn search_total(query: &str, api: &dyn GithubApi) -> Result<u64, ApiError> {
    let body = api.get(&format!("/search/issues?q={query}&per_page=1"))?;
    Ok(body.get("total_count").and_then(Json::as_u64).unwrap_or(0))
}

fn owned_star_count(author: &str, api: &dyn GithubApi) -> Result<u64, ApiError> {
    let repos = fetch_all(
        api,
        |page| {
            format!("/user/repos?affiliation=owner&visibility=all&per_page={PAGE_SIZE}&page={page}")
        },
        Json::as_array,
    )?;
    Ok(repos
        .iter()
        .filter(|repo| repo.get("fork").and_then(Json::as_bool) != Some(true))
        .filter(|repo| owner_login(repo) == Some(author))
        .filter_map(|repo| repo.get("stargazers_count").and_then(Json::as_u64))
        .sum())
}

fn owner_login(repo: &Json) -> Option<&str> {
    repo.get("owner")?.get("login")?.as_str()
}

fn search_public_commits(author: &str, api: &dyn GithubApi) -> Result<Vec<CommitRef>, ApiError> {
    let items = fetch_all(
        api,
        |page| format!("/search/commits?q=author:{author}&per_page={PAGE_SIZE}&page={page}"),
        |body| body.get("items").and_then(Json::as_array),
    )?;
    Ok(items
        .iter()
        .filter_map(search_ref)
        .filter(|commit| !is_excluded(&commit.repo))
        .collect())
}

fn search_ref(item: &Json) -> Option<CommitRef> {
    let repo = item.get("repository")?.get("full_name")?.as_str()?;
    let sha = item.get("sha")?.as_str()?;
    Some(CommitRef {
        repo: repo.to_owned(),
        sha: sha.to_owned(),
    })
}

fn list_owned_repo_commits(author: &str, api: &dyn GithubApi) -> Result<Vec<CommitRef>, ApiError> {
    let mut refs = Vec::new();
    for repo in owned_private_repo_names(api)? {
        refs.extend(list_repo_commits(&repo, author, api)?);
    }
    Ok(refs)
}

// Public repositories are already covered by the author search. Listing commits per
// repository is only needed for private ones, and it is too slow on very large histories.
fn owned_private_repo_names(api: &dyn GithubApi) -> Result<Vec<String>, ApiError> {
    let items = fetch_all(
        api,
        |page| {
            format!(
                "/user/repos?affiliation=owner,collaborator&visibility=all&per_page={PAGE_SIZE}&page={page}"
            )
        },
        Json::as_array,
    )?;
    Ok(items
        .iter()
        .filter(|repo| repo.get("private").and_then(Json::as_bool) == Some(true))
        .filter(|repo| repo.get("fork").and_then(Json::as_bool) != Some(true))
        .filter_map(|repo| repo.get("full_name").and_then(Json::as_str))
        .filter(|name| !is_excluded(name))
        .map(str::to_owned)
        .collect())
}

fn list_repo_commits(
    repo: &str,
    author: &str,
    api: &dyn GithubApi,
) -> Result<Vec<CommitRef>, ApiError> {
    let outcome = fetch_all(
        api,
        |page| format!("/repos/{repo}/commits?author={author}&per_page={PAGE_SIZE}&page={page}"),
        Json::as_array,
    );
    match outcome {
        Ok(items) => Ok(items
            .iter()
            .filter_map(|item| item.get("sha").and_then(Json::as_str))
            .map(|sha| CommitRef {
                repo: repo.to_owned(),
                sha: sha.to_owned(),
            })
            .collect()),
        Err(error) if error.is_skippable() => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn languages_of_commit(
    commit: &CommitRef,
    api: &dyn GithubApi,
) -> Result<BTreeSet<&'static str>, ApiError> {
    let path = format!("/repos/{}/commits/{}", commit.repo, commit.sha);
    let detail = match api.get(&path) {
        Ok(body) => body,
        Err(error) if error.is_skippable() => return Ok(BTreeSet::new()),
        Err(error) => return Err(error),
    };
    let files = detail.get("files").and_then(Json::as_array).unwrap_or(&[]);
    Ok(files
        .iter()
        .filter_map(|file| file.get("filename").and_then(Json::as_str))
        .filter_map(language_of_file)
        .collect())
}

fn fetch_all(
    api: &dyn GithubApi,
    path_for_page: impl Fn(usize) -> String,
    items_of: impl Fn(&Json) -> Option<&[Json]>,
) -> Result<Vec<Json>, ApiError> {
    let mut all = Vec::new();
    for page in 1..=MAX_PAGES {
        let body = api.get(&path_for_page(page))?;
        let items = items_of(&body).unwrap_or(&[]);
        all.extend_from_slice(items);
        if items.len() < PAGE_SIZE {
            break;
        }
    }
    Ok(all)
}

fn is_excluded(repo: &str) -> bool {
    EXCLUDED_REPOS.contains(&repo)
}

pub fn language_of_file(filename: &str) -> Option<&'static str> {
    let base = filename.rsplit('/').next()?;
    let (_, extension) = base.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "py" => Some("Python"),
        "java" => Some("Java"),
        "c" | "h" => Some("C"),
        "rs" => Some("Rust"),
        "go" => Some("Go"),
        "sh" | "bash" => Some("Shell"),
        "fish" => Some("Fish"),
        "js" => Some("JavaScript"),
        "ts" => Some("TypeScript"),
        "kt" => Some("Kotlin"),
        "html" => Some("HTML"),
        "css" => Some("CSS"),
        "vba" | "bas" | "cls" | "frm" => Some("VBA"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeGithubApi;

    const SEARCH_PAGE_1: &str = "/search/commits?q=author:klockmt&per_page=100&page=1";
    const REPOS_PAGE_1: &str =
        "/user/repos?affiliation=owner,collaborator&visibility=all&per_page=100&page=1";

    fn repo_commits_path(repo: &str, page: usize) -> String {
        format!("/repos/{repo}/commits?author=klockmt&per_page=100&page={page}")
    }

    #[test]
    fn merges_public_search_and_private_repo_commits_without_duplicates() {
        let api = FakeGithubApi::new()
            .with_body(
                SEARCH_PAGE_1,
                r#"{"items":[{"sha":"s1","repository":{"full_name":"klockmt/public"}}]}"#,
            )
            .with_body(
                REPOS_PAGE_1,
                r#"[{"full_name":"klockmt/public","fork":false,"private":false},{"full_name":"klockmt/secret","fork":false,"private":true}]"#,
            )
            .with_body(&repo_commits_path("klockmt/public", 1), r#"[{"sha":"s1"}]"#)
            .with_body(&repo_commits_path("klockmt/secret", 1), r#"[{"sha":"p1"},{"sha":"p2"}]"#);
        let commits = list_authored_commits("klockmt", &api).unwrap();
        assert_eq!(commits.len(), 3);
        assert!(commits.contains(&CommitRef {
            repo: "klockmt/secret".into(),
            sha: "p2".into()
        }));
    }

    #[test]
    fn lists_commits_only_for_private_non_fork_repositories() {
        let api = FakeGithubApi::new()
            .with_body(SEARCH_PAGE_1, r#"{"items":[]}"#)
            .with_body(
                REPOS_PAGE_1,
                r#"[{"full_name":"klockmt/mine","fork":false,"private":true},{"full_name":"klockmt/fork","fork":true,"private":true},{"full_name":"klockmt/open","fork":false,"private":false}]"#,
            )
            .with_body(&repo_commits_path("klockmt/mine", 1), r#"[{"sha":"m1"}]"#);
        let commits = list_authored_commits("klockmt", &api).unwrap();
        assert_eq!(
            commits,
            vec![CommitRef {
                repo: "klockmt/mine".into(),
                sha: "m1".into()
            }]
        );
        let requested = api.requested_paths();
        assert!(!requested.iter().any(|path| path.contains("klockmt/fork/")));
        assert!(!requested.iter().any(|path| path.contains("klockmt/open/")));
    }

    #[test]
    fn follows_full_pages_until_a_short_one() {
        let full_page: String = format!(
            "[{}]",
            (0..PAGE_SIZE)
                .map(|i| format!(r#"{{"sha":"c{i}"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let api = FakeGithubApi::new()
            .with_body(&repo_commits_path("klockmt/big", 1), &full_page)
            .with_body(&repo_commits_path("klockmt/big", 2), r#"[{"sha":"last"}]"#);
        let refs = list_repo_commits("klockmt/big", "klockmt", &api).unwrap();
        assert_eq!(refs.len(), PAGE_SIZE + 1);
        assert_eq!(api.requested_paths().len(), 2);
    }

    #[test]
    fn empty_or_missing_repository_yields_no_commits() {
        let api = FakeGithubApi::new().with_status(&repo_commits_path("klockmt/empty", 1), 409);
        assert!(list_repo_commits("klockmt/empty", "klockmt", &api)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn other_repository_errors_propagate() {
        let api = FakeGithubApi::new().with_status(&repo_commits_path("klockmt/locked", 1), 403);
        assert!(list_repo_commits("klockmt/locked", "klockmt", &api).is_err());
    }

    #[test]
    fn counts_each_commit_once_per_language() {
        let commits = vec![
            CommitRef {
                repo: "klockmt/a".into(),
                sha: "s1".into(),
            },
            CommitRef {
                repo: "klockmt/a".into(),
                sha: "s2".into(),
            },
        ];
        let api = FakeGithubApi::new()
            .with_body(
                "/repos/klockmt/a/commits/s1",
                r#"{"files":[{"filename":"a.py"},{"filename":"b.PY"},{"filename":"c.sh"}]}"#,
            )
            .with_body(
                "/repos/klockmt/a/commits/s2",
                r#"{"files":[{"filename":"d.py"}]}"#,
            );
        let counts = count_commits_per_language(&commits, &api).unwrap();
        assert_eq!(counts.get("Python"), Some(&2));
        assert_eq!(counts.get("Shell"), Some(&1));
    }

    #[test]
    fn missing_commit_detail_counts_as_no_languages() {
        let commits = vec![CommitRef {
            repo: "klockmt/gone".into(),
            sha: "s9".into(),
        }];
        let api = FakeGithubApi::new();
        assert!(count_commits_per_language(&commits, &api)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn top_languages_orders_by_count_then_name_and_limits() {
        let counts: BTreeMap<&str, usize> = BTreeMap::from([
            ("VBA", 4),
            ("Python", 6),
            ("C", 4),
            ("Shell", 5),
            ("HTML", 1),
            ("Java", 3),
        ]);
        let top = top_languages(&counts, 5);
        assert_eq!(
            top,
            vec![
                ("Python", 6),
                ("Shell", 5),
                ("C", 4),
                ("VBA", 4),
                ("Java", 3)
            ]
        );
    }

    #[test]
    fn collects_totals_counting_private_commits_and_owned_stars() {
        let commits = vec![
            CommitRef {
                repo: "klockmt/public".into(),
                sha: "s1".into(),
            },
            CommitRef {
                repo: "klockmt/secret".into(),
                sha: "p1".into(),
            },
            CommitRef {
                repo: "someone/project".into(),
                sha: "x1".into(),
            },
        ];
        let api = FakeGithubApi::new()
            .with_body("/search/issues?q=author:klockmt+type:pr&per_page=1", r#"{"total_count":12,"items":[]}"#)
            .with_body("/search/issues?q=author:klockmt+type:issue&per_page=1", r#"{"total_count":4,"items":[]}"#)
            .with_body(
                "/user/repos?affiliation=owner&visibility=all&per_page=100&page=1",
                r#"[{"full_name":"klockmt/public","fork":false,"owner":{"login":"klockmt"},"stargazers_count":7},{"full_name":"klockmt/secret","fork":false,"owner":{"login":"klockmt"},"stargazers_count":2},{"full_name":"x/theirs","fork":false,"owner":{"login":"other"},"stargazers_count":99}]"#,
            );
        let totals = collect_totals("klockmt", &commits, &api).unwrap();
        assert_eq!(
            totals,
            StatsTotals {
                contributed_repos: 1,
                commits: 3,
                pull_requests: 12,
                issues: 4,
                stars: 9
            }
        );
    }

    #[test]
    fn maps_extensions_case_insensitively() {
        assert_eq!(language_of_file("src/Main.PY"), Some("Python"));
        assert_eq!(language_of_file("README.md"), None);
        assert_eq!(language_of_file(".bashrc"), None);
    }
}
