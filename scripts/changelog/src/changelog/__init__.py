import re
from collections import OrderedDict
from contextlib import contextmanager
from functools import cache
from subprocess import run
from typing import Iterator

import click
from git import Commit, Repo, Tag

from changelog.template import render
from changelog.types import Commits, Tags


@click.command()
@click.option("--path", type=str, help="Path to the git repository")
@click.option("--only", type=str, help="Only include matching tags")
@click.option("--head", type=str, help="The current commit")
@click.option("--init", type=str, help="The initial commit")
@click.option("--name", type=str, help="The release name")
def main(
    path: str | None,
    only: str | None,
    head: str | None,
    init: str | None,
    name: str | None,
) -> None:
    with open_repo(path) as repo:
        tags = {t.commit: t for t in repo.tags}

        head_rev = repo.commit(head)
        over_rev = repo.iter_commits(f"{init or ''}..{head_rev}")

        commits = collect_commits(tags, head_rev, set(over_rev))
        deduped = dedupe_commits(commits)

        print(render(deduped, re.compile(only) if only else None, name))


def collect_commits(tags: Tags, head: Commit, over: set[Commit]) -> Commits:
    cmts, jobs, seen = OrderedDict(), [(head, tags.get(head))], set()

    def pop_job() -> tuple[Commit, Tag | None] | None:
        return jobs.pop(0) if jobs else None

    for c, t in iter(pop_job, None):
        if (c, t) in seen:
            continue

        if not over or c in over:
            cmts.setdefault(t, []).append(c)
            jobs.extend((p, tags.get(p) or t) for p in c.parents)
            seen.add((c, t))

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
