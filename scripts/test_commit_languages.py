from __future__ import annotations

import unittest
import urllib.error
from collections import Counter

from commit_languages import (
    PAGE_SIZE,
    CommitRef,
    count_commits_per_language,
    languages_of_commit,
    list_authored_commits,
    list_owned_repos,
    list_repo_commits,
    render_svg,
    retrying,
)


class FakeGithubApi:
    def __init__(
        self,
        search_items: list[dict[str, object]] | None = None,
        owned_repos: list[dict[str, object]] | None = None,
        repo_commits: dict[str, list[dict[str, object]]] | None = None,
        commit_files: dict[str, list[str]] | None = None,
    ) -> None:
        self.search_items = search_items or []
        self.owned_repos = owned_repos or []
        self.repo_commits = repo_commits or {}
        self.commit_files = commit_files or {}
        self.requested: list[str] = []

    def __call__(self, url: str) -> object:
        self.requested.append(url)
        if "/search/commits" in url:
            return {"items": self.search_items}
        if "/user/repos" in url:
            return _page(self.owned_repos, url)
        if "/commits?" in url:
            return _page(self.repo_commits[_repo_of(url)], url)
        return self._commit_detail(url)

    def _commit_detail(self, url: str) -> dict[str, object]:
        sha = url.rsplit("/", 1)[-1]
        if sha not in self.commit_files:
            raise urllib.error.HTTPError(url, 404, "Not Found", None, None)
        return {"files": [{"filename": name} for name in self.commit_files[sha]]}


def _page(items: list[dict[str, object]], url: str) -> list[dict[str, object]]:
    page = int(url.rsplit("page=", 1)[-1])
    start = (page - 1) * PAGE_SIZE
    return items[start : start + PAGE_SIZE]


def _repo_of(url: str) -> str:
    return url.split("/repos/", 1)[1].split("/commits?", 1)[0]


def make_repo(full_name: str, fork: bool = False) -> dict[str, object]:
    return {"full_name": full_name, "fork": fork}


def make_commit(sha: str) -> dict[str, object]:
    return {"sha": sha}


def make_search_item(repo: str, sha: str) -> dict[str, object]:
    return {"sha": sha, "repository": {"full_name": repo}}


class FlakyServerFetch:
    def __init__(self, failures: int, status: int = 500) -> None:
        self.failures = failures
        self.status = status
        self.calls = 0

    def __call__(self, url: str) -> object:
        self.calls += 1
        if self.calls <= self.failures:
            raise urllib.error.HTTPError(url, self.status, "Server Error", None, None)
        return {"ok": True}


class RecordingPause:
    def __init__(self) -> None:
        self.delays: list[float] = []

    def __call__(self, seconds: float) -> None:
        self.delays.append(seconds)


class RetryingTest(unittest.TestCase):
    def test_retries_server_errors_with_exponential_pause(self) -> None:
        flaky = FlakyServerFetch(failures=2)
        pause = RecordingPause()
        result = retrying(flaky, attempts=4, pause=pause)("https://api.github.com/x")
        self.assertEqual(result, {"ok": True})
        self.assertEqual(flaky.calls, 3)
        self.assertEqual(pause.delays, [1.0, 2.0])

    def test_gives_up_after_last_attempt(self) -> None:
        flaky = FlakyServerFetch(failures=10)
        with self.assertRaises(urllib.error.HTTPError):
            retrying(flaky, attempts=3, pause=RecordingPause())("https://api.github.com/x")
        self.assertEqual(flaky.calls, 3)

    def test_does_not_retry_client_errors(self) -> None:
        flaky = FlakyServerFetch(failures=10, status=404)
        with self.assertRaises(urllib.error.HTTPError):
            retrying(flaky, attempts=4, pause=RecordingPause())("https://api.github.com/x")
        self.assertEqual(flaky.calls, 1)


class ListAuthoredCommitsTest(unittest.TestCase):
    def test_merges_public_search_and_private_repo_commits_without_duplicates(self) -> None:
        api = FakeGithubApi(
            search_items=[make_search_item("klockmt/public", "s1")],
            owned_repos=[make_repo("klockmt/public"), make_repo("klockmt/secret")],
            repo_commits={
                "klockmt/public": [make_commit("s1")],
                "klockmt/secret": [make_commit("p1"), make_commit("p2")],
            },
        )
        refs = list_authored_commits("klockmt", api)
        self.assertEqual(
            set(refs),
            {
                CommitRef("klockmt/public", "s1"),
                CommitRef("klockmt/secret", "p1"),
                CommitRef("klockmt/secret", "p2"),
            },
        )
        self.assertEqual(len(refs), 3)

    def test_skips_forks_when_listing_owned_repos(self) -> None:
        api = FakeGithubApi(
            owned_repos=[make_repo("klockmt/mine"), make_repo("klockmt/upstream-fork", fork=True)],
            repo_commits={"klockmt/mine": [make_commit("m1")]},
        )
        self.assertEqual(list_owned_repos(api), ["klockmt/mine"])
        self.assertEqual(list_authored_commits("klockmt", api), [CommitRef("klockmt/mine", "m1")])


class ListRepoCommitsTest(unittest.TestCase):
    def test_follows_full_pages_until_short_one(self) -> None:
        commits = [make_commit(f"c{index}") for index in range(PAGE_SIZE + 1)]
        api = FakeGithubApi(repo_commits={"klockmt/big": commits})
        refs = list_repo_commits("klockmt/big", "klockmt", api)
        self.assertEqual(len(refs), PAGE_SIZE + 1)
        self.assertEqual(len(api.requested), 2)

    def test_returns_empty_for_empty_repository(self) -> None:
        def raise_conflict(url: str) -> object:
            raise urllib.error.HTTPError(url, 409, "Conflict", None, None)

        self.assertEqual(list_repo_commits("klockmt/empty", "klockmt", raise_conflict), [])


class LanguagesOfCommitTest(unittest.TestCase):
    def test_maps_extensions_case_insensitively_and_ignores_unknown(self) -> None:
        api = FakeGithubApi(commit_files={"s1": ["src/main.PY", "lib/util.rs", "README.md"]})
        self.assertEqual(languages_of_commit(CommitRef("klockmt/a", "s1"), api), {"Python", "Rust"})

    def test_returns_empty_set_when_commit_detail_is_missing(self) -> None:
        api = FakeGithubApi()
        self.assertEqual(languages_of_commit(CommitRef("klockmt/gone", "s9"), api), set())


class CountCommitsPerLanguageTest(unittest.TestCase):
    def test_counts_each_commit_once_per_language(self) -> None:
        refs = [CommitRef("klockmt/a", "s1"), CommitRef("klockmt/a", "s2")]
        api = FakeGithubApi(commit_files={"s1": ["a.py", "b.py", "c.sh"], "s2": ["d.py"]})
        counts = count_commits_per_language(refs, api)
        self.assertEqual(counts, Counter({"Python": 2, "Shell": 1}))


class RenderSvgTest(unittest.TestCase):
    def test_lists_only_top_five_languages(self) -> None:
        counts = Counter({"Python": 6, "Shell": 5, "VBA": 4, "Java": 3, "C": 2, "HTML": 1})
        svg = render_svg(counts, total_commits=21)
        self.assertIn("Python", svg)
        self.assertIn("C</text>", svg)
        self.assertNotIn("HTML", svg)

    def test_shows_share_of_all_commits(self) -> None:
        svg = render_svg(Counter({"Python": 3, "Shell": 1}), total_commits=4)
        self.assertIn("3 commits (75.0%)", svg)

    def test_renders_placeholder_when_no_language_found(self) -> None:
        svg = render_svg(Counter(), total_commits=0)
        self.assertIn("no commits found", svg)


if __name__ == "__main__":
    unittest.main()
