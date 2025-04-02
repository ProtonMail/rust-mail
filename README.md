# Proton Rust libraries

This repo maps part of the Core proton REST api. It's designed so that each team can extend this
with their own primitives, requests and domain types as needed.

## Releases

### Conventions

When your product/crate is ready for a release, create a new branch with the following syntax:

* `releases/$PRODUCT/$MAJOR.$MINOR`

When you are ready to release create a tag with the following syntax:

* `$PRODUCT-v$MAJOR.$MINOR.$PATCH`

For instance for the `mail-uniffi` crate this would translate into:

* Branch: `releases/mail-uniffi/0.55`
* Tag: `mail-uniffi/0.55`

### Guidelines

* After creating a release, the version in the master branch should be bumped to the next release
  candidate.
* Tags should **always be created from the release branches**.
* If you are working on bug fix or improvement directly related to the release, create a merge
  request targeting the release branch and then port the release to master.
* Backporting fixes from master should be avoided if possible as you are likely to pull in new
  unrelated changes. This may not be avoidable in all cases and it may be better to create a new
  release if the merge conflicts are too great.

### Generating the Changelog

You should use [git-cliff] to generate the Changelog:

```bash
# Everything
git-cliff > CHANGELOG.md

# Specific range
git-cliff mail-uniffi-v0.62.0..mail-uniffi-v0.65.0

```

To skip commits from the changelog you should add an `*` before the `:` in the commit message.

Example:
```
feat*: this will not be in the changelog

```



## Crate Publishing

Until we move to the mono-repo, any time changes need to be made available to
the mail repo a new version of the crates in this repo needs to be published.

To publish a crate the version of the crate needs to be bumped as well as all
the dependees. This needs to be repeated recursively for any dependee. Finally
the root repo's `Cargo.toml` also needs to be updated.

Once the version change has been merged, you need to run the publish jobs
manually for each crate in the correct order. The publish jobs are only
available after the master pipeline has finished.

Each publish job will build the crate and open an MR in the registry. The link
to the MR will be printed in the job output:

```shell
Cloning into 'registry'...
warning: redirecting to https: //gitlab.protontech.ch/rust/shared/registry.git/
Switched to a new branch 'package / proton-core-common-0.6.31'
[package/proton-core-common-0.6.31 3f76300] Package: proton-core-common-0.6.31
2 files changed, 1 insertion( + )
create mode 100644 downloads/proton-core-common@0.6.31.crate
create mode 100644 downloads/proton-core-common@0.6.31.json
warning: redirecting to https: //gitlab.protontech.ch/rust/shared/registry.git/
remote:
remote: View merge request for package/proton-core-common-0.6.31:
remote:   https: //gitlab.protontech.ch/rust/shared/registry/-/merge_requests/456  # <----- click this
remote:
To https: //gitlab.protontech.ch/rust/shared/registry
* [new branch]      package/proton-core-common-0.6.31 -> package/proton-core-common-0.6.31
```

A code owner needs to approve this MR and then it can be merged in the registry.

Once the registry MR has been merged, you can proceed with the publish job for
the direct dependees of the published crate.

These steps need to be repeated for each update crate.

### Publish order

This sections contains a collection of update orders for the most common
changes made in this repository.

#### stash updates

* stash-macros
* stash
* proton-sqlite
* proton-event-loop
* proton-action-queue
* proton-core-common

#### proton-vcard updates

* proton-vcard
* proton-api-core
* proton-event-loop
* proton-action-queue
* proton-core-common

#### proton-api-core updates

* proton-api-core
* proton-event-loop
* proton-action-queue
* proton-core-common

#### proton-event-loop updates

* proton-event-loop
* proton-core-common

## Nix package manager and Devenv

Affected files: `devenv.nix`, `devenv.lock` and `devenv.yaml`

### What is it?

Devenv (https://devenv.sh) is a Nix language framework for having reusable, portable and stable developer environments across all machines.
It is based on Nix, the language and package manager, but it does not require full NixOS or having nix-darwin distribution installed.

Note, this is an experiment to see if Nix package manager can be useful for sharing common setup across developers.

### Why is it included?

Devenv allows us to setup all necessary dependencies including how to build the codebase for iOS, once, in a declarative manner.

Currently it provides complete environment for building monorepo + building frameworks for iOS.

### Do I have to install nix now?

No!
It is **opt-in** and developers not interested in the Nix ecosystem are **not required to maintain** files.
If something breaks it is the responsibility of Nix enthusiasts to fix the config files.

Moreover, this setup is not going to be used in the CI.

### What if I want to try?

Follow guide on https://devenv.sh/getting-started in order to setup the devenv itself. It works on most of linux distributions as well as in WSL or on macOS.

Then, in the root of this repository create file `devenv.local.nix` (its gitignored) and add following:

```nix
{ pkgs: ...}:
{
  env.IOS_REPO_ROOT="<path to your ET apple inbox repository>";
}
```

Now you will be able to build iOS by using `proton-build-ios` command
