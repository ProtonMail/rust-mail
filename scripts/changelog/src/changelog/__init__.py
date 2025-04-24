from collections import OrderedDict
from contextlib import contextmanager
from functools import cache
from subprocess import run
from typing import Iterator

import click
from git import Commit, Repo, Tag

from changelog.template import render
from changelog.types import Commits


@click.command()
@click.option("--path", type=str)
@click.option("--head", type=str)
@click.option("--after", type=str)
def main(path: str | None, head: str | None, after: str | None) -> None:
    with open_repo(path) as repo:
        tags = collect_tags(repo)

        head_ref = repo.commit(head)
        over_ref = set(repo.iter_commits(f"{after}..{head_ref}")) if after else None

        commits = collect_commits(tags, head_ref, over_ref)
        deduped = dedupe_commits(commits)

        print(render(deduped))


def collect_tags(repo: Repo) -> dict[Commit, Tag]:
    return {t.commit: t for t in repo.tags}


def collect_commits(tags: dict[Commit, Tag], head: Commit, over: set[Commit] | None) -> Commits:
    cmts, jobs, seen = OrderedDict(), OrderedDict({head: tags.get(head)}), set()

    def pop_job() -> tuple[Commit, Tag | None] | None:
        return jobs.popitem(False) if jobs else None

    for c, t in iter(pop_job, None):
        if c in seen:
            continue

        if over is not None and c not in over:
            continue

        cmts.setdefault(t, []).append(c)
        jobs.update({p: tags.get(p) or t for p in c.parents})
        seen.add(c)

    return cmts


def dedupe_commits(cmts: Commits) -> Commits:
    keep, seen = OrderedDict(), dict()

    def want(c: Commit, prev: list[Commit]) -> bool:
        return not any(patch_id(c) == patch_id(p) for p in prev)

    for tag, commits in reversed(cmts.items()):
        for c in reversed(commits):
            if want(c, seen.get(c.summary) or []):
                keep.setdefault(tag, []).append(c)
                seen.setdefault((c.summary), []).append(c)

    return OrderedDict(reversed(keep.items()))


@cache
def patch_id(c: Commit) -> str:
    diff = run(
        ["git", "diff-tree", "--patch", c.hexsha],
        capture_output=True,
        cwd=c.repo.working_dir,
        check=True,
    )

    pid = run(
        ["git", "patch-id", "--stable"],
        capture_output=True,
        input=diff.stdout,
        cwd=c.repo.working_dir,
        check=True,
    )

    match pid.stdout.decode().strip().split():
        case [pid, _]:
            return pid
        case _:
            return c.hexsha


@contextmanager
def open_repo(path: str | None) -> Iterator[Repo]:
    repo = Repo(path)

    try:
        yield repo
    finally:
        repo.close()
