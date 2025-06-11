# Proton Rust libraries

This repo maps part of the Core proton REST api. It's designed so that each team can extend this
with their own primitives, requests and domain types as needed.

## Style

All Rust code must be formatted with `cargo fmt` before committing.

All TOML files must be formatted with `taplo fmt`. You can install the Taplo CLI tool with:

```bash
cargo install taplo-cli --locked
```

Any code not formatted this way will be rejected by the CI.

## Releases

### Conventions

When your product/crate is ready for a release, create a new branch with the following syntax:

- `releases/$PRODUCT/$MAJOR.$MINOR`

When you are ready to release create a tag with the following syntax:

- `$PRODUCT-v$MAJOR.$MINOR.$PATCH`

For instance for the `mail-uniffi` crate this would translate into:

- Branch: `releases/mail-uniffi/0.55`
- Tag: `mail-uniffi/0.55`

### Procedure of the release

#### New release

- Bump version in **master**.
- Run the script for generating changelog.
- Create the respective tag an push it.
- Create the respective branch and push it.
- Notify Slack channel about the pipeline with gist from the changelog.

#### Fix Releases

- Bump version in the respective **Release Branch**
- Run the script for generating changelog.
- Create the tag and push it.
- Merge branch back to `master` (but do not delete source branch!)
- Notify Slack channel about the pipeline with gist from the changelog.

### Guidelines

- If you are working on bug fix or improvement directly related to the release, create a merge
  request targeting the release branch and then port the release to master.
- Backporting fixes from master should be avoided if possible as you are likely to pull in new
  unrelated changes. This may not be avoidable in all cases and it may be better to create a new
  release if the merge conflicts are too great.

### Generating the Changelog

You should use the `./scripts/changelog` tool to generate the Changelog.

This can be invoked with the prepared script:

```bash
$ pipx install uv # if needed
$ sh ./mail/mail-uniffi/scripts/gen_changelog.sh
```

To skip commits from the changelog you should add an `*` before the `:` in the commit message:

```
feat*: this will not be in the changelog
```

```
feat(ET-1234)*: This will also not be in the changelog
```

## Nix

If you're using Nix, you can use devenv to pull most of our dependencies - just
[install devenv](https://devenv.sh/getting-started) and run `devenv shell` to
get a shell with (mostly) everything in scope.

If you're not a fan of Nix, you don't have to install it, this is optional - you
will have to install dependencies (e.g. Go) by hand in this case, though.

Note that for building mail<->ios specific stuff you'll also need to provide a
custom envvar - create a file called `devenv.local.nix` with:

```nix
{ pkgs: ...}:

{
  env.IOS_REPO_ROOT="<path to your ET apple inbox repository>";
}
```

Having that, use the `proton-build-ios` command to build the iOS stuff.
