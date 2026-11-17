from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Final, cast
from xml.sax.saxutils import escape

API_ROOT: Final[str] = "https://api.github.com"
PAGE_SIZE: Final[int] = 100
MAX_PAGES: Final[int] = 10
SKIPPABLE_REPO_STATUS: Final[frozenset[int]] = frozenset({404, 409})
TOP_LANGUAGES: Final[int] = 5
BAR_MAX_WIDTH: Final[int] = 260
ROW_HEIGHT: Final[int] = 30
LABEL_WIDTH: Final[int] = 110

EXTENSION_LANGUAGE: Final[dict[str, str]] = {
    ".py": "Python",
    ".java": "Java",
    ".c": "C",
    ".h": "C",
    ".rs": "Rust",
    ".go": "Go",
    ".sh": "Shell",
    ".bash": "Shell",
    ".fish": "Fish",
    ".js": "JavaScript",
    ".ts": "TypeScript",
    ".kt": "Kotlin",
    ".html": "HTML",
    ".css": "CSS",
    ".vba": "VBA",
    ".bas": "VBA",
    ".cls": "VBA",
    ".frm": "VBA",
}

Fetch = Callable[[str], object]


@dataclass(frozen=True)
class CommitRef:
    repo: str
    sha: str


def github_fetch(url: str, token: str) -> object:
    request = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def list_authored_commits(author: str, fetch: Fetch) -> list[CommitRef]:
    refs = search_public_commits(author, fetch) + list_owned_repo_commits(author, fetch)
    unique = {(ref.repo, ref.sha): ref for ref in refs}
    return list(unique.values())


def search_public_commits(author: str, fetch: Fetch) -> list[CommitRef]:
    refs: list[CommitRef] = []
    for page in range(1, MAX_PAGES + 1):
        query = urllib.parse.urlencode(
            {"q": f"author:{author}", "per_page": PAGE_SIZE, "page": page}
        )
        payload = cast(dict[str, object], fetch(f"{API_ROOT}/search/commits?{query}"))
        items = cast(list[dict[str, object]], payload["items"])
        refs.extend(_search_ref(item) for item in items)
        if len(items) < PAGE_SIZE:
            break
    return refs


def list_owned_repo_commits(author: str, fetch: Fetch) -> list[CommitRef]:
    refs: list[CommitRef] = []
    for repo in list_owned_repos(fetch):
        refs.extend(list_repo_commits(repo, author, fetch))
    return refs


def list_owned_repos(fetch: Fetch) -> list[str]:
    names: list[str] = []
    for page in range(1, MAX_PAGES + 1):
        url = (
            f"{API_ROOT}/user/repos?affiliation=owner,collaborator"
            f"&visibility=all&per_page={PAGE_SIZE}&page={page}"
        )
        items = cast(list[dict[str, object]], fetch(url))
        names.extend(str(item["full_name"]) for item in items if not item["fork"])
        if len(items) < PAGE_SIZE:
            break
    return names


def list_repo_commits(repo: str, author: str, fetch: Fetch) -> list[CommitRef]:
    refs: list[CommitRef] = []
    for page in range(1, MAX_PAGES + 1):
        url = f"{API_ROOT}/repos/{repo}/commits?author={author}&per_page={PAGE_SIZE}&page={page}"
        try:
            items = cast(list[dict[str, object]], fetch(url))
        except urllib.error.HTTPError as error:
            if error.code not in SKIPPABLE_REPO_STATUS:
                raise
            return refs
        refs.extend(CommitRef(repo=repo, sha=str(item["sha"])) for item in items)
        if len(items) < PAGE_SIZE:
            break
    return refs


def _search_ref(item: dict[str, object]) -> CommitRef:
    repository = cast(dict[str, str], item["repository"])
    return CommitRef(repo=repository["full_name"], sha=cast(str, item["sha"]))


def languages_of_commit(ref: CommitRef, fetch: Fetch) -> set[str]:
    url = f"{API_ROOT}/repos/{ref.repo}/commits/{ref.sha}"
    try:
        detail = cast(dict[str, object], fetch(url))
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        return set()
    files = cast(list[dict[str, str]], detail.get("files", []))
    return {
        language
        for changed in files
        if (language := EXTENSION_LANGUAGE.get(Path(changed["filename"]).suffix.lower()))
    }


def count_commits_per_language(refs: list[CommitRef], fetch: Fetch) -> Counter[str]:
    counts: Counter[str] = Counter()
    for ref in refs:
        counts.update(languages_of_commit(ref, fetch))
    return counts


def render_svg(counts: Counter[str], total_commits: int) -> str:
    top = counts.most_common(TOP_LANGUAGES)
    height = ROW_HEIGHT * max(len(top), 1) + 20
    width = LABEL_WIDTH + BAR_MAX_WIDTH + 170
    rows = [_svg_row(index, name, count, top[0][1], total_commits) for index, (name, count) in enumerate(top)]
    body = "\n".join(rows) if rows else '  <text x="10" y="30">no commits found</text>'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        'font-family="Segoe UI, Ubuntu, sans-serif" font-size="14">\n'
        f"{body}\n</svg>\n"
    )


def _svg_row(index: int, name: str, count: int, largest: int, total_commits: int) -> str:
    y = 20 + index * ROW_HEIGHT
    bar = max(1, round(BAR_MAX_WIDTH * count / largest))
    percent = 100 * count / total_commits
    return (
        f'  <text x="0" y="{y + 12}" fill="#888">{escape(name)}</text>\n'
        f'  <rect x="{LABEL_WIDTH}" y="{y}" width="{bar}" height="14" rx="3" fill="#2f81f7"/>\n'
        f'  <text x="{LABEL_WIDTH + bar + 8}" y="{y + 12}" fill="#888">'
        f"{count} commits ({percent:.1f}%)</text>"
    )


def main(argv: list[str]) -> int:
    output_dir = Path(argv[1])
    token = os.environ["PROFILE_TOKEN"]
    author = os.environ.get("COMMIT_AUTHOR", "klockmt")
    fetch: Fetch = lambda url: github_fetch(url, token)
    refs = list_authored_commits(author, fetch)
    counts = count_commits_per_language(refs, fetch)
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "commit-languages.svg").write_text(render_svg(counts, len(refs)), encoding="utf-8")
    print(f"{len(refs)} commits, {len(counts)} languages")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
