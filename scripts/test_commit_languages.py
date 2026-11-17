from __future__ import annotations

import unittest
import urllib.error
from collections import Counter

from commit_languages import (
    SEARCH_PAGE_SIZE,
    CommitRef,
    count_commits_per_language,
    languages_of_commit,
    list_authored_commits,
    render_svg,
)


class FakeGithubApi:
    def __init__(self, search_pages: list[list[dict[str, object]]], commit_files: dict[str, list[str]]) -> None:
        self.search_pages = search_pages
        self.commit_files = commit_files
        self.requested: list[str] = []

    def __call__(self, url: str) -> object:
        self.requested.append(url)
        if "/search/commits" in url:
            return self._search_page(url)
        sha = url.rsplit("/", 1)[-1]
        if sha in self.commit_files:
            return {"files": [{"filename": name} for name in self.commit_files[sha]]}
        raise urllib.error.HTTPError(url, 404, "Not Found", None, None)

    def _search_page(self, url: str) -> dict[str, object]:
        page = int(url.split("page=")[-1])
        items = self.search_pages[page - 1] if page <= len(self.search_pages) else []
        return {"items": items}


def make_item(repo: str, sha: str) -> dict[str, object]:
    return {"sha": sha, "repository": {"full_name": repo}}


class ListAuthoredCommitsTest(unittest.TestCase):
    def test_stops_after_short_page(self) -> None:
        api = FakeGithubApi([[make_item("klockmt/a", "s1"), make_item("klockmt/b", "s2")]], {})
        refs = list_authored_commits("klockmt", api)
        self.assertEqual(refs, [CommitRef("klockmt/a", "s1"), CommitRef("klockmt/b", "s2")])
        self.assertEqual(len(api.requested), 1)

    def test_follows_full_pages_until_short_one(self) -> None:
        full_page = [make_item("klockmt/a", f"s{i}") for i in range(SEARCH_PAGE_SIZE)]
        api = FakeGithubApi([full_page, [make_item("klockmt/b", "last")]], {})
        refs = list_authored_commits("klockmt", api)
        self.assertEqual(len(refs), SEARCH_PAGE_SIZE + 1)
        self.assertEqual(len(api.requested), 2)


class LanguagesOfCommitTest(unittest.TestCase):
    def test_maps_extensions_case_insensitively(self) -> None:
        api = FakeGithubApi([], {"s1": ["src/main.PY", "lib/util.rs", "README.md"]})
        self.assertEqual(languages_of_commit(CommitRef("klockmt/a", "s1"), api), {"Python", "Rust"})

    def test_returns_empty_set_when_commit_detail_is_missing(self) -> None:
        api = FakeGithubApi([], {})
        self.assertEqual(languages_of_commit(CommitRef("klockmt/gone", "s9"), api), set())


class CountCommitsPerLanguageTest(unittest.TestCase):
    def test_counts_each_commit_once_per_language(self) -> None:
        refs = [CommitRef("klockmt/a", "s1"), CommitRef("klockmt/a", "s2")]
        api = FakeGithubApi([], {"s1": ["a.py", "b.py", "c.sh"], "s2": ["d.py"]})
        counts = count_commits_per_language(refs, api)
        self.assertEqual(counts, Counter({"Python": 2, "Shell": 1}))


class RenderSvgTest(unittest.TestCase):
    def test_lists_top_languages_with_share_of_commits(self) -> None:
        svg = render_svg(Counter({"Python": 3, "Shell": 1}), total_commits=4)
        self.assertIn("Python", svg)
        self.assertIn("3 commits (75.0%)", svg)

    def test_renders_placeholder_when_no_language_found(self) -> None:
        svg = render_svg(Counter(), total_commits=0)
        self.assertIn("no commits found", svg)


if __name__ == "__main__":
    unittest.main()
