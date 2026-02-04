Search query expressions.

This module provides a flexible query DSL (Domain Specific Language) for building and executing
search queries across different attribute types. It supports complex boolean operations,
field-specific filters, and full-text search with wildcards.

# Expression Types

The query expressions trees are built from these key types:

- [`Expression`]: Composable boolean expressions (AND/OR/Term) of conditions
- [`TermValue`]: Attribute value condition argument
- [`TermValuePart`]: Part of the [`TermValue`] which may be a wildcard

# Example Usage

```rust
use proton_foundation_search::document::Value;
use proton_foundation_search::engine::{Engine, QueryEvent};
use proton_foundation_search::query::expression::{Expression, Func, TermValue};

# fn example(engine: Engine) -> Result<(), Box<dyn std::error::Error>> {
# let today = 42;

// Simple text search
for event in engine
    .query()
    .with_expression("hello world".parse()?)
    .search()
{
    // Matches are returned early. Scores at the end.
    match event {
        QueryEvent::Load(_) => todo!(),
        QueryEvent::Found(_) => todo!(),
        QueryEvent::Stats(_) => todo!(),
    }
}

// Complex boolean query
let query_iterator = engine
    .query()
    .with_expression(Expression::And(vec![
        Expression::attr("title", Func::Matches, TermValue::text("urgent")),
        Expression::attr("body", Func::Matches, TermValue::text("report")),
    ]))
    .with_expression(Expression::Or(vec![
        Expression::attr("date", Func::GreaterThan, today),
        Expression::attr("priority", Func::Equals, 1),
    ]))
    .search();
# Ok(())
# }
```

# Query Building

Queries are built using a combination of:

- Boolean expressions (AND/OR)
- Field conditions with type-specific filters
- Full-text search terms
- Logical groups in braces (...)

The builder enforces type safety by validating that conditions match field types.

# Supported Query Types

## Text Search
- Word matching
- Phrase matching
- Field-specific or all-fields search
- Existence check with `=*` and `!=*`

## Integer Fields
- Only matches specific attributes (no all attrs search)
- Exact match
- Range queries (>, >=, <, <=)
- Existence check with `attr=*` and `!attr=*`

## Boolean Fields
- Only matches specific attributes (no all attrs search)
- True/false matching
- Existence check with `attr=*` and `!attr=*`

## Tag Fields
- Only matches specific attributes (no all attrs search)
- Exact and fuzzy matching
- Tag wildcard matching
- Existence check with `attr=*` and `!attr=*`

# Query Execution

A query search is a state machine that must be iterated.
The machine will request loading required blobs from the app.

# Wildcards

Wildcards can be added either programatically or in query DSL:

```rust
use proton_foundation_search::query::expression::*;

let wildcard: Expression = "*".parse().unwrap();
assert_eq!(wildcard, Expression::any_attr(Func::Matches, TermValue::wild()));
assert_eq!(wildcard.to_string(), "*");

let prefix: Expression = "rev*".parse().unwrap();
assert_eq!(prefix, Expression::any_attr(Func::Matches, TermValue::text("rev").wildcard()));
assert_eq!(prefix.to_string(), "rev*");

let suffix: Expression = "*tion".parse().unwrap();
assert_eq!(suffix, Expression::any_attr(Func::Matches, TermValue::wild().then("tion")));
assert_eq!(suffix.to_string(), "*tion");

let middle: Expression = "rev*tion".parse().unwrap();
assert_eq!(middle, Expression::any_attr(Func::Matches, TermValue::text("rev").wildcard().then("tion")));
assert_eq!(middle.to_string(), "rev*tion");
```

## Wildcard Behaviour with Whitespace

**Important to Note**: Whitespace around wildcards affects how they are processed:

- **No whitespace**: `"abc*def"` → Single term with wildcard between text parts
- **With whitespace**: `"abc * def"` → Three separate terms: `["abc", "*", "def"]`

It should be noted that when a wildcard is separated by whitespace, it becomes a **standalone wildcard term** that matches all entries with any value.

```rust
use proton_foundation_search::query::expression::Expression;

// This works: wildcard adjacent to text
let working: Expression = "abc*def".parse().unwrap();
// Results in: TermValue("abc").wildcard().then("def")

// This will fail: whitespace separates the wildcard
let failing: Expression = "abc * def".parse().unwrap();
// Results in: AND([Term("abc"), Term("*"), Term("def")])
// The standalone "*" term matches nothing, so AND always fails
```

**Standalone wildcards** (wildcards with no adjacent text) produce no search results because they generate no trigrams for matching. In AND expressions, this causes the entire query to return no results.

## Recommended Patterns

- Use `"text*"` for prefix matching
- Use `"*text"` for suffix matching  
- Use `"text1*text2"` for middle wildcards
- Avoid `"text1 * text2"` (whitespace around wildcard)
- Avoid standalone `"*"` in AND expressions

## Wildcard Processing

The [`TermValue`] is further split by the engine when processing the query using the processor. This in effect forms an implicit sub grammar that defines expected wildcard structure, pattern semantics and rules that determine how text parts are joined or separated based on whitespace. This is by design as an application can substitute this logic through custom [`Processor`]. Individual tokens produced by the processor are extracted into separate expressions. The processing logic handles:

1. **Tokenization**: Text is split into tokens by the processor
2. **Whitespace handling**: Whitespace around wildcards determines if they merge with adjacent text
3. **Non-tokenizable characters**: Characters that don't form tokens (like `.`, `-`) are replaced with wildcards

**Note**: The exact behaviour depends on the processor's tokenization rules and whitespace handling. See the built-in [`TextProcessor`] documentation for details.

```rust
use proton_foundation_search::query::expression::*;
use proton_foundation_search::processor::TextProcessor;

let mut phrase: Expression = "\"talking about a rev*tion sounds\"".parse().unwrap();
assert_eq!(phrase, Expression::any_attr(Func::Matches, 
    TermValue::text("talking about a rev")
        .wildcard()
        .then("tion sounds")
));

phrase.process(&TextProcessor::default());
assert_eq!(phrase, 
    Expression::and(
        Expression::any_attr(Func::Matches, "talking"),
        Expression::and(
            Expression::any_attr(Func::Matches, "about"), Expression::and(
                Expression::any_attr(Func::Matches, "a"),
                Expression::and(
                    Expression::any_attr(Func::Matches, TermValue::text("rev").wildcard().then("tion")),
                    Expression::any_attr(Func::Matches, "sounds"),
                )
            )
        )
    ));

assert_eq!(phrase.to_string(), "(talking AND about AND a AND rev*tion AND sounds)");
```

Parts of text that are not recognized as tokens by the processor are also turned into wildcards
for fuzzy search ([`Func::Matches`]):

```rust
use proton_foundation_search::query::expression::*;
use proton_foundation_search::processor::TextProcessor;

let mut phrase: Expression = "\"settings.e*.highlight\"".parse().unwrap();
assert_eq!(phrase, Expression::any_attr(Func::Matches, 
    TermValue::text("settings.e")
        .wildcard()
        .then(".highlight")
));

phrase.process(&TextProcessor::default());
assert_eq!(phrase, 
    Expression::any_attr(Func::Matches, 
        TermValue::text("settings")
            .wildcard()
            .then("e")
            .wildcard()
            .then("highlight")
    ));

assert_eq!(phrase.to_string(), "settings*e*highlight");
```

# Performance Considerations

Query performance is optimized by:

- TODO: optimized execution planning
- caching

# Error Handling

Programatic query expression tree building is infallible.
The query parser will fail on invalid input, though.

The query parser validates:

- Expression validity
- Parse errors

It does not check fields against a schema as we do not have any.

Considering that the term values can be boolean, integer, string,
values are converted to appropriate type during query execution.
For example the integer index will convert a term value to integer 
and skip search if the value is not an integer or a string representation of an integer.

# Thread Safety

Queries are safe for concurrent execution:

- Multiple queries can run simultaneously
- Query state is immutable after building

# Cancellation

Queries can be canceled by simply not polling the iterator anymore.
