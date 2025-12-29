With v0.1.0 we follow rusts practice of semantic versioning, there will be breaking changes on
0.x releases but we try to keep these at a minimum. The planned 'unsynn-rust' and along that a
'unsynn-derive' will be implemented. When thes are ready and no major deficiencies in 'unsynn'
are found then it is time for a 1.0.0 release.


## Planned/Ideas

* can we add prettyprint for tokens_to_string?
  this needs a threadlocal storing some context (indent level, indent by (str of spaces), prettyprint flag)
* make proc_macro2 optional with a feature flag  
  this would disable parsing &str and related API's and most of the test suite. But should be
  sufficient for writing lean proc_macro parsers.
* `Enclosed<Begin, Content, End>` like `Cons<Begin, Cons<Except<End>, Content>, End>>`
* improve error handing
   - document how errors are reported and what the user can do to handle them
   - User can/should write forgiving grammars that are error tolerant
   - add tests error/span handling
   - v0.2.0 will improve the Span handling considerably. Probably by an extra feature flag. We
     aim for ergonomic/automagical correct spans, the user shouldnt be burdened by making
     things correct. Details for this need to be laied out. Maybe a `SpanOf<T: Parse>`
   - can we have some `Explain<T>` that explains what was expected and why it failed to simplify
     complex errors?
* Brainfart: Dynamic parser construction  
  instead `parse::<UnsynnType>()`
  create a parse function dynamically from a str parsed by unsynn itself
  "Either<This, That>".to_parser().parse()
  this will need a `trait DynUnsynn` implementing the common/dynamic parts of these
  and a registry where all entities supporting dynamic construction are registered.
  This will likely be factored out into a unsynn-dyn crate
  Add some scanf like DSL to generate these parsers.
  xmacro may use it like $(foo@Ident: values)
* Braintfart: Memoization
  in TokenIter:
  ```text
   Rc< enum {
     Countdown(Cell(usize)),
     Memo(HashMap<
       (counter, typeid, Option<NonZeroU64 hash_over_extra_parameters>),
       (Result, TokenIter_after)
     >>),
   }>
  ```

  Countdown counter which activates memoization only after certain number of tokens parsed,
  parsing small things does not need the overhead of memoizing.  can we somehow (auto) Trait
  which types become memoized? Small things don't need to be memoized.

  Note to my future self:
  ```text
    Result needs to be dyn Memo
    where  trait Memo: Clone   and clone is cheap:
             enum MaybeRc<T>{Direkt(T), Shared(Rc<T>)}
  ```
* add rust types
  * f32: 32-bit floating point number
  * f64: 64-bit floating point number (default)


# Design Priorities

Unsynn foremost goal is to make parsers easy and ergonomic to use. We deliberately provide
some duplicated functionality and type aliases to prioritize expressiveness. Fast compile
times with as little as necessary dependencies comes second. We do not focus explicitly on
rust syntax, this will be addressed by other crates.


# Development

unsynn is meant to evolve opportunistically. When you spot a problem or need a new feature
feel free to open an [issue](https://git.pipapo.org/cehteh/unsynn/issues) or (prefered!) send
a [PR](https://git.pipapo.org/cehteh/unsynn/pulls).

Commits and other git operations are augmented and validated with
[cehgit](https://git.pipapo.org/cehteh/cehgit). For contributors it is recommened to enable
cehgit too by calling `./.cehgit install-hook --all` within a checked out unsynn repository.


## Contribution/Coding Guidelines

Chances to get contributions merged increase when you:

 * Include documentation following the existing documentation practice. Write examples/doctests.
 * Passing `./.cehgit run` without errors or warnings.
 * Passing test-coverage with `cargo mutants`.
 * Implement reasonable complete things. Not everything needs to be included in a first
   version, but it must be usable.


### Git Branches

 * `main`  
   Will be updated on new releases. When you plan to make a small contribution that should be
   merged soon then you can work on top of `main`.
 * `release-*`  
   stable release may get their own branch for fixes and backported features.
 * `devel`  
   Development branch which will eventually be merged into `main`. Non-trivial contributions that
   may take some time to develop should use `devel` as starting point. But be prepared to
   rebase frequently on top of the ongoing `devel`.
 * `feature-*`  
   More complex features and experiments are developed in feature branches. Any non trivial
   contribution should be done in a `feature-*` branch as well. Once complete they become
   merged into `devel`. Some of these experiments may stall or be abandoned, do not base your
   contribution on an existing feature branch.
