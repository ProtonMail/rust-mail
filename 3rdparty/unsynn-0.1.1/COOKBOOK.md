# Cookbook


## Parsing

Parsing is done over a `ShadowCountedIter<<TokenStream as IntoIterator>::IntoIter>` which is
shortened as [`TokenIter`]. `TokenIter` are created with [`ToTokens::to_token_iter()`] or
[`ToTokens::into_token_iter()`], which should be implemented for for anyhing you need.

The main trait for parsing a [`TokenIter`] is the [`Parse`] trait. This traits methods are all
default implemented and can be used as is. [`Parse`] is implemented for all types that
implement [`Parser`] and [`ToTokens`]. [`Parser`] is the trait that has to be implemented for
each type that should be parsed.

The [`IParse`] trait is implemented for [`TokenIter`], this calls `Parse::parse()` in a
convenient way when types can be inferred.

[`ToTokens`] complements [`Parser`] as it will turn any parseable entity back into a
`TokenStream`.


### [`Parse::parse_with()`] transformations

The `Parse::parse_with()` method is used for parsing in more complex situations. In the
simplest case it can be used to validate the values of a parsed type. More complex usage will
fill in `HiddenState` and other not parsed members or construct completely new types from
parsed entities.


## ToTokens

The [`ToTokens`] trait is responsible for emiting tokens to a `TokenStream`. Unlike the trait
from the quote crate we define `ToTokens` for a lot more types and provide more methods.

Notably it provides the [`ToTokens::to_token_iter()`] and [`ToTokens::into_token_iter()`]
methods which create the entry points for parsing.

When textual representation of a parsed entity is required then [`ToTokens::tokens_to_string`]
can be used.  The standard `Display` trait is implemented on top of that, as such every type
that has [`ToTokens`] implented can be printed as text.


## Composition and Type Aliases

For moderately complex types it is possible to use composition with [`Cons`], [`Either`] and
other container types instead defining new enums or structures.

It is recommended to alias such composed types to give them useful names. This can be used for
creating grammars on the fly without any boilerplate code.


## The [`unsynn!{}`] Macro

The easiest way to describe grammars is to use the [`unsynn!{}`] macro. This allows to define
grammars just by defining enums and structures. The macro will generate the necessary
implementations for [`Parser`] and [`ToTokens`] but offers no flexibility in what is
generated. It is possible to add [`HiddenState<T>`] members to add non syntactic entries to
there custom structs.


## Implementing Parsers

### Transactions

The [`Parse`] Trait parses items within a transaction. This is with the
[`Transaction::transaction()`] method. Internally this clones the iterator, calls the
[`Parser::parser()`] method and copies the cloned iterator back to the original on success.
This means that if a parser fails, the input is reset to the state before the parser was
called. For efficiency reasons the [`Parser::parser()`] methods are themself not
transactional, when they fail they leave the input in a consumed state.

When one wants to manually parse *alternatives* within a [`Parser`] (like in a enum) it must be
manually called within a transaction. This is only necessary when the parsed entity is
compound and not the final alternative. For simple parsable entities one can just call the
[`Parse::parse()`] method which already provides the transaction.

```rust
# use unsynn::*;
enum MyEnum {
    // Complex variant
    Tuple(i32, i32),
    // Simple variant
    Simple(i32),
    // Another simple variant
    Another(Ident),
}

impl Parser for MyEnum {
    fn parser(input: &mut TokenIter) -> Result<Self> {
        // Use [`Transaction::transaction()`] to parse the tuple variant
        if let Ok(tuple) = input.transaction(
            |mut trans_input|
            Ok(MyEnum::Tuple(
                i32::parser(&mut trans_input)?,
                i32::parser(&mut trans_input)?,
            ))
        ) {
            Ok(tuple)
        } else
        // Try to parse the simple variant
        // can use the `Parse::parse()` or `IParse::parse()` method directly since a
        // single entity will be put in a transaction by those.
        if let Ok(i) = input.parse() {
            Ok(MyEnum::Simple(i))
        } else {
            // Try to parse the last variant
            // this can use the `Parser::parser()` method since this is the final alternative
            Ok(MyEnum::Another(Ident::parser(input)?))
        }
    }
}
```


### Different ways to implement Parsers

There are different approaches how one can implement parsers. Each has its own advantages and
disadvantages. unsynn supports both, so one can freely mix whatever makes most sense in a
particular case.

unsynn uses a rather simple, first come - first served approach when parsing. Parsers may be a
subset or share a common prefix with other parsers. This needs some attention.

In the case where parsers are subsets to other parsers and one puts them into a disjunction in
a `unsynn! { enum ..}` or in a `Either` combinator the more specific case must come first,
otherwise it will never match.

```rust
# use unsynn::*;
// Ident must come first since TokenTree matches any token.
type Example = Either<Ident, TokenTree>;
```

For the other case where parsers sharing longer prefixes (this should rarely happen in
practice) it may benefit performance to break these into a type with the shared prefix that
dispatches on the distinct parts.


#### Exact AST Representation

One approach is to define a structures that reflects the AST of the grammar exactly.  This is
what the [`unsynn!{}`] macro and composition does. The program later works with the parsed
structure directly. The advantage is that [`Parser`] and [`ToTokens`] are simple and come for
free and that the source structure of the AST stays available.

```rust
# use unsynn::*;

unsynn!{
    // define a list of Ident = "LiteralString",.. assignments
    struct Assignment {
        id: Ident,
        _equal: Assign,
        value: LiteralString,
    }

    struct AssignmentList {
        list: DelimitedVec<Assignment, Comma>
    }
}
```

When the implementation generated by the [`unsynn!{}`] macro is not sufficient one can
implement [`Parser`] and [`ToTokens`] for custom structs and enums  manually.


#### High level representation

Another approach is to represent the data more in the way further processing requires. This
simplifies working with the data but one **has** to implement the [`Parser`] and [`ToTokens`]
traits manually. Sometimes the [`Parse::parse_with()`] method will become useful in such
cases.

```rust
# use unsynn::*;
# use std::collections::HashMap;
// We could go with `unsynn!{struct Assignment{...}}` as above here. But lets use composition
// as example here. This stays internal so its complexity isnt exposed.
type Assignment = Cons<Ident, Assign, LiteralString>;

// Here we'll parse the list of assignments into a structure that represents the
// data in a way thats easier to use from a rust program
#[derive(Default)]
struct AssignmentList {
    // each 'Ident = LiteralString'
    list: Vec<(Ident, String)>,
    // We want to have a fast lookup to the entries
    lookup: HashMap<Ident, usize>,
}

impl Parser for AssignmentList {
    fn parser(input: &mut TokenIter) -> Result<Self> {
        let mut assignment_list = AssignmentList::default();

        // We construct the `AssignmentList` by parsing the content, appending and processing it.
        while let Ok(assignment) = Delimited::<Assignment, Comma>::parse(input) {
            assignment_list.list.push((
                assignment.value.first.clone(),
                // Create a String without the enclosing double quotes
                assignment.value.third.as_str().to_string()
            ));
            // add it to the lookup
            assignment_list.lookup.insert(
                assignment.value.first.clone(),
                assignment_list.list.len()-1
            );
            // No Comma, no more assignments
            if assignment.delimiter.is_none() {
                break;
            }
        }
        Ok(assignment_list)
    }
}

impl ToTokens for AssignmentList {
    fn to_tokens(&self, output: &mut TokenStream) {
        for a in &self.list {
            a.0.to_tokens(output);
            Assign::new().to_tokens(output);
            LiteralString::from_str(&a.1).to_tokens(output);
            Comma::new().to_tokens(output);
        }
    }
}
# fn test_assignment_list() {
#     let mut input = r#"a = "b", c = "d""#.to_token_iter();
#     let parsed = AssignmentList::parse(&mut input).unwrap();
#     assert_eq!(parsed.list.len(), 2);
#     assert_eq!(parsed.list[1].1, "d");
# }
```


## Errors

Unsynn parsers return the first error encountered. It does not try error recovery on its own
or being smart about what may have caused an error (typos, missing semicolons etc).  The only
exception to this is when parsing disjunct entities (`Either` or other enums) where errors are
expected to happen on the first branches.  When any branch succeeds the error is dropped and
parsing goes on, when all branches fail then that error which made the most progress is
returned. Progress is tracked with the `ShadowCountedIter`.  This is implemented for enums
created with the `unsynn!` macro as well for the `Either::parser()` method.  This covers all
normal cases.

When one needs to implement disunct parsers manually this has to be taken into account.
This is then done by creating an [`Error`] with [`ErrorKind::NoError`] by
`let mut err = Error::no_error()` within the `Parser::parse` implementation. Then any parser
that is called subsequently tries to `err.upgrade(Item::parser(..))` which handles storing the
error which made the most progress. Eventually a `Ok(...)` or the upgraded `Err(err)` is
returned. For details look at the source of [`Either::parser`].

Errors carry a iterator starting at the location where the error happened. This can be used
for further inspection.

Some parser types in unsynn are ZST's this means they don't carry the token they parsed and
consequently the have no `Span` thus the location of an error will be unavailable for them.
If that poses to be a problem this might be revised in future unsynn versions.

In other cases where spans are wrong this is considered a happy accident, please fill a Bug or
send a PR fixing this issue.


### Error Recovery

Trying to recover from syntax errors is usually not required/advised for compiled
languages. When there is a syntax error, report it to the user and let them fix the source
code.

Recoverable parsing would need knowledge of the grammar (or even context) being parsed. This
can not be supported by unsynn itself as it does not define any grammars. When one needs
recoverable parsers then this has to implemented into the grammar definition. Future versions
of unsynn may provide some tools to assist with this. The actual approach is still in
discussion.


## Implementation/Performance Notes

Unsynn is (as of now) implemented as recursive descent PEG with backtracking.  This has
worst-case exponential complexity, there is a plan to fix that in future releases. Currently
to avoid these cases it is recommended to formulate disjunctive parsers so that they fail early
and don't share long prefixes.

For example things like
`Either<Cons<LongValidCode, OneThing>, Cons<LongValidCode, OtherThing>>` should be
rewritten as `Cons<LongValidCode, Either<OneThing, OtherThing>>`.


## Stability Guarantees

Unsynn is in development, nevertheless we give some stability promises. These will only be
broken when we discover technical reasons that make it infeasible to keep them up. Eventually
things in the unstable category below will move up to stable.


### Stable

 * Operator name definitions in operator::names  
   These follow common (rust) naming, if not this will be fixed, otherwise you can rely on
   these to be stable.
 * Functionality  
   Existing types and parsers are there to stay. A few things especially when they are freshly
   added may be shaken out and refined/extended but existing functionality should be
   preserved. In some cases changes may require minor adjustments on code using it.


### Unstable

 * modules  
   The module organization is still in flux and may be refactored at any time. This shouldn't
   matter because unsynn reexports everything at the crate root. The only exception here is
   the `operators::names` which we try to keep stable.
 * internal representation  
   Don't rely on the internal representations there are some plans and ideas to change
   these. Some types that are currently ZST may become stateful, collections may become
   wrapped as `Rc<Vec<T>>` etc.
 * traits  
   Currently there are the main traits Parse, Parser, IParse and ToTokens. In future these may
   become refactored and split into smaller traits and some more may be added. This will then
   require some changes for the user. The existing functionality will be preserved nevertheless.
 * trait bounds  
   We don't have extra trait bounds on parsed type in the current implementation. This will
   likely change in the future for improving the parser. Expect that types eventually should
   be at least `Clone + Hash + PartialEq`. Possibly some helper trait needs to be implemented
   too. Details will be worked out later.
