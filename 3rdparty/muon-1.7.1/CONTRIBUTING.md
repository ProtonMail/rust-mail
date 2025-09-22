# Contributing to Muon

There are opportunities to contribute to Muon at any level. It doesn't matter if
you are just getting started with Rust or are the most weathered expert, we can
use your help.

**No contribution is too small and all contributions are valued.**

This guide will help you get started. **Do not let this guide intimidate you**.
It should be considered a map to help you navigate the process.

The [#Muon channel][slack] is available for any concerns not covered in this guide, please join
us!

[slack]: https://protonmail.slack.com/archives/C06FUDRJ9MJ
[muon-gitlab]: https://gitlab.protontech.ch/ProtonVPN/rust/muon

## Contributing in Issues

For any issue, there are fundamentally two ways an individual can contribute:

1. By opening the issue for discussion: For instance, if you believe that you
   have discovered a bug in Muon, creating a new issue in [Muon JIRA project][issue] or opening a pull request in the [Muon gitlab project][muon-gitlab], and opening a related new thread on the [#Muon slack channel][slack] are the ways to report it.

2. By helping to resolve the issue: Typically this is done either in the form of
   demonstrating that the issue reported is not a problem after all, or more
   often, by opening a Pull Request that changes some bit of something in
   Muon in a concrete and reviewable manner.

[issue]: https://protonag.atlassian.net/jira/software/c/projects/MUON/boards/155

**Anybody can participate in any stage of contribution**. We urge you to
participate in the discussion around bugs and participate in reviewing PRs.

### Asking for General Help

If you have reviewed existing documentation and still have questions or are
having problems, you can [open a discussion][slack] asking for help.

In exchange for receiving help, we ask that you contribute back a documentation
PR that helps others avoid the problems that you encountered.

### Submitting a Bug Report

When opening a new issue in the [Muon JIRA project][issue] or a pull request in the [Muon gitlab project][muon-gitlab], you are asked to provide details about the encountered problems.

The two most important pieces of information we need in order to properly
evaluate the report is a description of the behavior you are seeing and a simple
test case we can use to recreate the problem on our own. If we cannot recreate
the issue, it becomes impossible for us to fix. As Muon is a Rust library, we require that the test case is reported in Rust even if the issue arises in another language that use some FFI bindings.
There is an exception if the issue is in the Muon/FFI bindings themselves.

In order to rule out the possibility of bugs introduced by userland code, test
cases should be limited, as much as possible, to using only Muon API.

See [How to create a Minimal, Complete, and Verifiable example][mcve].

[mcve]: https://stackoverflow.com/help/mcve

### Resolving a Bug Report

In the majority of cases, issues are resolved by opening a Pull Request with a fix. The
process enforces a review and approval workflow that ensures that the proposed changes meet the minimal quality and functional guidelines of the Muon project.

## Pull Requests

Pull Requests are the way concrete changes are made to the code, documentation,
and dependencies in the Muon repository.

Even tiny pull requests (e.g., one character pull request fixing a typo in API
documentation) are greatly appreciated.

Before making a large change or a change in the public API, it is
**mandatory** to first open an issue or a pull request describing the change to solicit feedback and guidance.

### Muon API
Muon is a library that is meant to be used across the whole Proton company. As such, **Muon guarantees to its users a stable API** and breaking this guarantee can only be done **in exceptional circumstances where there is no other choice**.

Hence, when creating PRs, you should ask yourself:
- Is my PR adding new things to the public interface, introducing breaking changes, or adding something else than business logic? If yes, have I opened a discussion on [the dedicated slack channel][slack] and either created a related Jira issue on [the Muon Jira project][issue] or opened a pull request in the [Muon gitlab project][muon-gitlab]? Maintainers and other contributors will help designing a solution that take into consideration all Muon end-users.
- Are my changes minimal? If no, you should consider opening several smaller PRs containing minimal changes that can be reviewed individually.

### Tests

If the change being proposed alters code (as opposed to only documentation for
example), it is either adding new functionality to Muon or it is fixing
existing, broken functionality. In both of these cases, the pull request should
include one or more tests to ensure that Muon does not regress in the future.
There are three ways to write tests: [integration tests][integration-tests], [documentation tests][documentation-tests], and [unit-test][unit-tests].

In general, Muon favors [integration tests][integration-tests] and [documentation tests][documentation-tests] as much as possible.

#### Integration tests

Integration tests go in the `muon` crate in folder `tests`.
Take a special care to test for all targets and not to overflow the production API with benchmarks or heavy tests.

Muon comes with a custom Server Mock API, so please, use it for local tests.

Every change to Muon should also ensure that it contains tests towards `Atlas`.

The best strategy for writing a new integration test is to look at existing
integration tests in the crate and follow the style.

#### Documentation tests

Ideally, every API has at least one [documentation test][documentation-tests] that demonstrates how to use the API. Documentation tests are run with `cargo test --doc`. This ensures
that the example is correct and provides additional test coverage.

The trick to documentation tests is striking a balance between being succinct
for a reader to understand and actually testing the API.

Same as with integration tests, when writing a documentation test, the full
`muon` crate is available.

### Benchmarks

> [!NOTE]  To Do

### Commits

It is a recommended best practice to keep your changes as logically grouped as
possible within individual commits. There is no limit to the number of commits
any single Pull Request may have, and many contributors find it easier to review
changes that are split across multiple commits.

That said, if you have a number of commits that are "checkpoints" and don't
represent a single logical change, please squash those together.

#### Commit message guidelines

Use [conventional commits][cc] as much as you reasonably can and link to the related Jira issue if a commit is fixing an issue entirely.

If a PR is fixing an issue, the PR title should clearly mention the Jira issue.

[cc]: https://www.conventionalcommits.org/en/v1.0.0/

### Discuss and update

You will probably get feedback or requests for changes to your Pull Request.
This is a big part of the submission process so don't be discouraged! Some
contributors may sign off on the Pull Request right away, others may have
more detailed comments or feedback. This is a necessary part of the process
in order to evaluate whether the changes are correct and necessary.

**Any community member can review a PR and you might get conflicting feedback**.
Keep an eye out for comments from code owners to provide guidance on conflicting
feedback.

## Reviewing Pull Requests

**Any Muon community member is welcome to review any pull request**.

All Muon contributors who choose to review and provide feedback on Pull
Requests have a responsibility to both the project and the individual making the
contribution. Reviews and feedback must be helpful, insightful, and geared
towards improving the contribution as opposed to simply blocking it. If there
are reasons why you feel the PR should not land, explain what those are. Do not
expect to be able to block a Pull Request from advancing simply because you say
"No" without giving an explanation. Be open to having your mind changed. Be open
to working with the contributor to make the Pull Request better.

When reviewing a Pull Request, the primary goals are for the codebase to improve
and for the person submitting the request to succeed. **Even if a Pull Request
does not land, the submitters should come away from the experience feeling like
their effort was not wasted or unappreciated**.

### Review a bit at a time.

Do not overwhelm new contributors.

It is tempting to micro-optimize and make everything about relative performance,
perfect grammar, or exact style matches. Do not succumb to that temptation.

Focus first on the most significant aspects of the change:

1. Does this change make sense for Muon?
2. Does this change make Muon better, even if only incrementally?
3. Are there clear bugs or larger scale issues that need attending to?
4. Is the commit message readable and correct? If it contains a breaking change
   is it clear enough?

Note that only **incremental** improvement is needed to land a PR. This means
that the PR does not need to be perfect, only better than the status quo. Follow
up PRs may be opened to continue iterating.

When changes are necessary, *request* them, do not *demand* them, and **do not
assume that the submitter already knows how to add a test or run a benchmark**.

Requests for small changes that are not essential are fine, but try to
avoid stalling the Pull Request.

## Versioning Policy

With Muon ≥1.0.0:

 * Patch (1.\_.x) releases _must only_ contain bug fixes or documentation
   changes. Besides this, these releases should not substantially change
   runtime behavior.
 * Minor (1.x.0) releases may contain new functionalities, minor dependency updates, deprecations, and larger internal implementation changes.
 * Major (x.0.0) releases are breaking the Muon stable API. Major changes are **not** expected to happen, but still can if the maintainers and contributors evaluate that **there is no other option**.

This is as defined by [Semantic Versioning 2.0](https://semver.org/).

## Integrating change into Muon

Since the Muon project consists of a number of crates, many of which depend on
each other, and many projects depend on Muon, publishing a new version involve some complexities.
When you want your change to be integrated into Muon, ensure that you follow these steps:

1. **Update Cargo metadata.** After releasing any path dependencies, update the
   `version` field in `Cargo.toml` to the new version.
2. **Update the changelog for the crate.** Each crate in the Muon repository
   has its own `CHANGELOG.md` in that crate's subdirectory. Any changes to that
   crate since the last release should be added to the changelog. Change
   descriptions may be taken from the Git history, but should be edited to
   ensure a consistent format, based on [Keep A Changelog][keep-a-changelog].
   Other entries in that crate's changelog may also be used for reference.
   Please, update the **unreleased** section of the crate's changelog with your changes.
   Maintainers will take care of creating the final changelog on release date.
3. **Perform a final audit for breaking changes.** Compare the HEAD version of
   crate with the master branch. If there are any breaking API changes,
   determine if those changes can be made without breaking existing APIs.
   If so, resolve those issues. Otherwise, if it is necessary to
   make a breaking release, update the version numbers to reflect this.
4. **Open a pull request with your changes.** Once that pull request has been
   approved by a maintainer and the pull request has been merged, your work is done.

## New release
Every Wednesday, maintainers will produce the next release based on last week changes.

[keep-a-changelog]: https://github.com/olivierlacan/keep-a-changelog/blob/master/CHANGELOG.md
[unit-tests]: https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html
[integration-tests]: https://doc.rust-lang.org/rust-by-example/testing/integration_testing.html
[documentation-tests]: https://doc.rust-lang.org/rust-by-example/testing/doc_testing.html
[conditional-compilation]: https://doc.rust-lang.org/reference/conditional-compilation.html