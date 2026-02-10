unsynn (from german 'unsinn' for nonsense) is a minimalist rust parser library. It achieves
this by leaving out the actual grammar implementations and doing only minimal
[error](Parse#errors) handling.

In exchange it offers simple composeable Parsers and ergonomic Parser construction. Grammars
will be implemented in their own crates (see unsynn-rust).

It is primarily intended use is when one wants to create proc macros for rust that define their
own grammar or need only sparse rust parsers.

Other uses can be building parsers for gramars outside a rust/proc-macro context. Unsynn can
parse any `&str` data (The tokenizer step relies on proc_macro2).


# Examples

## Creating and Parsing Custom Types

The [`unsynn!{}`] macro will generate the [`Parser`] and [`ToTokens`] impls (and more).  This
is optional, the impls could be written by hand when necessary.

Notice that unsynn implements [`Parser`] and [`ToTokens`] for many standard rust types. Like
we use `u32` in this example.

```rust
# use unsynn::*;
let mut token_iter = "foo ( 1, 2, 3 )".to_token_iter();

unsynn!{
    struct IdentThenParenthesisedNumbers {
        ident: Ident,
        numbers: ParenthesisGroupContaining::<CommaDelimitedVec<u32>>,
    }
}

// iter.parse() is from the IParse trait
let ast: IdentThenParenthesisedNumbers = token_iter.parse().unwrap();

assert_eq!(
    ast.tokens_to_string(),
    "foo(1,2,3)".tokens_to_string()
)
```

## Using Composition

Composition can be used without defining new datatypes. This is useful for simple parsers or
when one wants to parse things on the fly which are desconstructed immediately.

```rust
# use unsynn::*;
// We parse this below
let mut token_iter = "foo ( 1, 2, 3 )".to_token_iter();

// Type::parse() is from the Parse trait
let ast =
    Cons::<Ident, ParenthesisGroupContaining::<CommaDelimitedVec<u32>>>
        ::parse(&mut token_iter).unwrap();

assert_eq!(
    ast.tokens_to_string(),
    "foo ( 1, 2, 3 )".tokens_to_string()
)
```

## Custom Operators and Keywords

To define keywords and operators we provide the `keyword!` and `operator!` macros:

```rust
# use unsynn::*;
keyword! {
    Calc = "CALC";
}

operator! {
    Add = "+";
    Substract = "-";
    Multiply = "*";
    Divide = "/";
}

// The above can be written within a unsynn!
// See next example about parsing recursive grammars

// looks like BNF, but can't do recursive types
type Expression = Cons<Calc, AdditiveExpr, Semicolon>;
type AdditiveOp = Either<Add, Substract>;
type AdditiveExpr = Either<Cons<MultiplicativeExpr, AdditiveOp, MultiplicativeExpr>, MultiplicativeExpr>;
type MultiplicativeOp = Either<Multiply, Divide>;
type MultiplicativeExpr = Either<Cons<LiteralInteger, MultiplicativeOp, LiteralInteger>, LiteralInteger>;

let ast = "CALC 2*3+4/5 ;".to_token_iter()
    .parse::<Expression>().expect("syntax error");
```

## Parsing Recursive Grammars

Recursive grammars can be parsed using structs and resolving the recursive parts in a `Box` or
`Rc`. This looks less BNF like but acts closer to it:

```rust
# use unsynn::*;
# use std::rc::Rc;
unsynn! {
    keyword Calc = "CALC";
    operator Add = "+";
    operator Substract = "-";
    operator Multiply = "*";
    operator Divide = "/";

    struct Expression(Calc, AdditiveExpr, Semicolon);
    // we preserve nested Either and Cons here instead defining new enums and structs because that would be more noisy
    struct AdditiveOp(Either<Add, Substract>);
    // with a Rc (or Box) here we can resolve the recursive nature of the grammar
    struct AdditiveExpr(Either<Cons<MultiplicativeExpr, AdditiveOp, Either<Rc<AdditiveExpr>,MultiplicativeExpr>>, MultiplicativeExpr>);
    struct MultiplicativeOp(Either<Multiply, Divide>);
    struct MultiplicativeExpr(Either<Cons<LiteralInteger, MultiplicativeOp, Rc<MultiplicativeExpr>>, LiteralInteger>);
}

// now we can parse more complex expressions. Adding parenthesis is left as excercise to the reader
let ast = "CALC 10+1-2*3+4/5*100 ;".to_token_iter()
    .parse::<Expression>().expect("syntax error");
```

# Feature Flags

* `hash_keywords`  
  This enables hash tables for larger keyword groups.  This is **enabled by default** since it
  guarantees fast lookup in all use-cases and the extra dependency it introduces is very
  small. Nevertheless this feature can be disabled when keyword grouping is not or rarely used
  to remove the dependency on `fxhash`. Keyword lookups then fall back to a binary search
  implementation. Note that the implementation already optimizes the cases where only one or
  only a few keywords are in a group.

* `docgen`  
  The [`unsynn!{}`], [`keyword!{}`] and [`operator!{}`] will automatically generate some
  additional docs. This is **enabled by default**.
