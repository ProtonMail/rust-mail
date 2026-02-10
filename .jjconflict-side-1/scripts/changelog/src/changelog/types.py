from collections import OrderedDict

from git import Commit, Tag

type Commits = OrderedDict[Tag | None, list[Commit]]

type Tags = dict[Commit, Tag]
