import re
from dataclasses import asdict, dataclass
from datetime import date

from git import Commit, Tag
from jinja2 import Environment

from changelog.types import Commits

TEMPLATE = """\
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

{% for release in releases -%}
## [{{ release.name }}]{% if release.date %} - {{ release.date }}{% endif %}

{% for section in release.sections -%}
### {{ section.name }}

{% for entry in section.entries -%}
- {{ entry }}
{% endfor %}
{% endfor %}
{% endfor %}
"""


def render(commits: Commits, only: re.Pattern | None, name: str | None) -> str:
    env = Environment()
    tmp = env.from_string(TEMPLATE)
    ctx = build_context(commits, only, name)

    return tmp.render(asdict(ctx)).strip()


@dataclass
class Section:
    name: str
    entries: list[str]


@dataclass
class Release:
    name: str
    date: date | None
    sections: list[Section]


@dataclass
class Context:
    releases: list[Release]


def build_context(cmts: Commits, only: re.Pattern | None, name: str | None) -> Context:
    releases = list()

    for tag, commits in cmts.items():
        if tag and only and not only.match(tag.name):
            continue

        if (release := build_release(tag, commits, name)) and release.sections:
            releases.append(release)

    return Context(sorted(releases, key=lambda r: r.date or date.max, reverse=True))


def build_release(tag: Tag | None, commits: list[Commit], name: str | None) -> Release:
    if tag:
        release_name = tag.name
        release_date = tag.commit.committed_datetime.date()
    elif name:
        release_name = name
        release_date = date.today()
    else:
        release_name = "Unreleased"
        release_date = None

    return Release(
        release_name,
        release_date,
        build_sections(commits),
    )


def build_sections(commits: list[Commit]) -> list[Section]:
    sections = dict()

    def decode(s: str | bytes) -> str:
        return s.decode() if isinstance(s, bytes) else s

    for c in commits:
        try:
            lhs, msg = decode(c.summary).split(":", 1)
        except Exception:
            continue
        else:
            lhs, msg = lhs.strip(), msg.strip()

        if lhs.endswith("*"):
            continue

        if m := re.search(r"(?:\(|\])([A-Z]+-[0-9]+)(?:\)|\])", lhs):
            msg = f"[{m.group(1)}] {msg}"

        if lhs.startswith("feat"):
            sections.setdefault("Features", []).append(msg)

        if lhs.startswith("fix"):
            sections.setdefault("Fixes", []).append(msg)

        if lhs.startswith("refactor"):
            sections.setdefault("Changed", []).append(msg)

    return [Section(name, entries) for name, entries in sorted(sections.items())]
