use proton_foundation_search::query::expression::{Expression, Func, QueryParseError, TermValue};

fn parse_expression(query: &str) -> Result<Expression, QueryParseError> {
    query.parse()
}

#[test]
fn parsing_expression_with_single_term() {
    assert_eq!(
        parse_expression("hello"),
        Ok(Expression::any_attr(
            Func::Matches,
            TermValue::text("hello")
        ))
    );
}

#[test]
fn parsing_expression_with_multiple_terms_no_operator() {
    assert_eq!(
        parse_expression("hello world"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::any_attr(Func::Matches, TermValue::text("world"))
        ))
    );
}

#[test]
fn parsing_expression_with_multiple_terms_with_operator() {
    assert_eq!(
        parse_expression("hello and world AND foo"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::and(
                Expression::any_attr(Func::Matches, TermValue::text("world")),
                Expression::any_attr(Func::Matches, TermValue::text("foo"))
            )
        ))
    );
}

#[test]
fn parsing_expression_with_multiple_terms_with_or_operator() {
    assert_eq!(
        parse_expression("hello or world OR foo"),
        Ok(Expression::or(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::or(
                Expression::any_attr(Func::Matches, TermValue::text("world")),
                Expression::any_attr(Func::Matches, TermValue::text("foo"))
            )
        ))
    );
}

#[test]
fn parsing_expression_with_multiple_operators() {
    assert_eq!(
        parse_expression("hello and world OR foo AND bar"),
        Ok(Expression::or(
            Expression::and(
                Expression::any_attr(Func::Matches, TermValue::text("hello")),
                Expression::any_attr(Func::Matches, TermValue::text("world")),
            ),
            Expression::and(
                Expression::any_attr(Func::Matches, TermValue::text("foo")),
                Expression::any_attr(Func::Matches, TermValue::text("bar"))
            )
        ))
    );
}

#[test]
fn parsing_expression_with_parenthesis() {
    assert_eq!(
        parse_expression("(hello)"),
        Ok(Expression::any_attr(
            Func::Matches,
            TermValue::text("hello")
        ))
    );
    assert_eq!(
        parse_expression("hello and (world)"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::any_attr(Func::Matches, TermValue::text("world")),
        ))
    );
    assert_eq!(
        parse_expression("hello and (world greetings)"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::and(
                Expression::any_attr(Func::Matches, TermValue::text("world")),
                Expression::any_attr(Func::Matches, TermValue::text("greetings")),
            )
        ))
    );
    assert_eq!(
        parse_expression("hello and (world or you) and foo"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::and(
                Expression::or(
                    Expression::any_attr(Func::Matches, TermValue::text("world")),
                    Expression::any_attr(Func::Matches, TermValue::text("you")),
                ),
                Expression::any_attr(Func::Matches, TermValue::text("foo")),
            )
        ))
    );
    // nested
    assert_eq!(
        parse_expression("hello and ((me and world) or you) and foo"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("hello")),
            Expression::and(
                Expression::or(
                    Expression::and(
                        Expression::any_attr(Func::Matches, TermValue::text("me")),
                        Expression::any_attr(Func::Matches, TermValue::text("world")),
                    ),
                    Expression::any_attr(Func::Matches, TermValue::text("you")),
                ),
                Expression::any_attr(Func::Matches, TermValue::text("foo")),
            )
        ))
    );
}

#[test]
fn parsing_negation() {
    assert_eq!(
        parse_expression("!hello"),
        Ok(Expression::Not(Box::new(Expression::any_attr(
            Func::Matches,
            TermValue::text("hello")
        ))))
    );
    assert_eq!(
        parse_expression("well !(hello world)"),
        Ok(Expression::and(
            Expression::any_attr(Func::Matches, TermValue::text("well")),
            Expression::Not(Box::new(Expression::and(
                Expression::any_attr(Func::Matches, TermValue::text("hello")),
                Expression::any_attr(Func::Matches, TermValue::text("world"))
            )))
        ))
    );
    assert_eq!(
        parse_expression("!\"hello mea culpa\""),
        Ok(Expression::Not(Box::new(Expression::any_attr(
            Func::Matches,
            TermValue::text("hello mea culpa")
        ))))
    );
}

#[test]
fn parsing_term_with_field() {
    // without function
    assert_eq!(
        parse_expression("foo~hello"),
        Ok(Expression::attr(
            "foo",
            Func::Matches,
            TermValue::text("hello")
        ))
    );
    assert_eq!(
        parse_expression("foo~\"hello world\""),
        Ok(Expression::attr(
            "foo",
            Func::Matches,
            TermValue::text("hello world")
        ))
    );
}

#[test]
fn parsing_term_with_function() {
    let cases = [
        (
            "=hello",
            Expression::any_attr(Func::Equals, TermValue::text("hello")),
        ),
        (
            "=\"hello world\"",
            Expression::any_attr(Func::Equals, TermValue::text("hello world")),
        ),
        (
            ">2",
            Expression::any_attr(Func::GreaterThan, TermValue::int(2)),
        ),
        (
            ">=2",
            Expression::any_attr(Func::GreaterThanOrEqual, TermValue::int(2)),
        ),
        (
            "<2",
            Expression::any_attr(Func::LessThan, TermValue::int(2)),
        ),
        (
            "<=2",
            Expression::any_attr(Func::LessThanOrEqual, TermValue::int(2)),
        ),
        (
            "~abcdef*",
            Expression::any_attr(Func::Matches, TermValue::text("abcdef").wildcard()),
        ),
        (
            "matcher",
            Expression::any_attr(Func::Matches, TermValue::text("matcher")),
        ),
        (
            "~bad\\ trap*",
            Expression::any_attr(Func::Matches, TermValue::text("bad trap").wildcard()),
        ),
    ];

    for (test, expect) in cases {
        assert_eq!(parse_expression(test), Ok(expect), "{test:?}");
    }
}

#[test]
fn parsing_term_with_field_and_function() {
    let cases = [
        (
            "foo=hello",
            Expression::attr("foo", Func::Equals, TermValue::text("hello")),
        ),
        (
            "foo=\"hello world\"",
            Expression::attr("foo", Func::Equals, TermValue::text("hello world")),
        ),
        (
            "foo>2",
            Expression::attr("foo", Func::GreaterThan, TermValue::int(2)),
        ),
        (
            "foo>=2",
            Expression::attr("foo", Func::GreaterThanOrEqual, TermValue::int(2)),
        ),
        (
            "foo<2",
            Expression::attr("foo", Func::LessThan, TermValue::int(2)),
        ),
        (
            "foo<=2",
            Expression::attr("foo", Func::LessThanOrEqual, TermValue::int(2)),
        ),
        (
            "foo~abcdef*",
            Expression::attr("foo", Func::Matches, TermValue::text("abcdef").wildcard()),
        ),
        (
            "foo~fighter",
            Expression::attr("foo", Func::Matches, TermValue::text("fighter")),
        ),
        (
            r###"foo~"a \"hóó\" j!""###,
            Expression::attr("foo", Func::Matches, TermValue::text("a \"hóó\" j!")),
        ),
        (
            "foo>bad\\ trap",
            Expression::attr("foo", Func::GreaterThan, TermValue::text("bad trap")),
        ),
        (
            "foo~'$fail*'",
            Expression::attr("foo", Func::Matches, TermValue::text("$fail").wildcard()),
        ),
    ];

    for (test, expect) in cases {
        assert_eq!(parse_expression(test), Ok(expect), "{test:?}");
    }
}

#[test]
fn parsing_string() {
    assert_eq!(
        parse_expression("abc"),
        Ok(Expression::any_attr(Func::Matches, TermValue::text("abc")))
    );
    assert_eq!(
        parse_expression("\"hello world\""),
        Ok(Expression::any_attr(
            Func::Matches,
            TermValue::text("hello world")
        ))
    );
    assert_eq!(
        parse_expression("hello\\ world"),
        Ok(Expression::any_attr(
            Func::Matches,
            TermValue::text("hello world")
        ))
    );
}

#[test]
fn parsing_trims() {
    assert_eq!(
        parse_expression("  elide  "),
        Ok(Expression::Term {
            field: None,
            function: Func::Matches,
            value: TermValue::text("elide")
        })
    );
}

#[test]
fn parsing_empty() {
    assert_eq!(parse_expression(""), Ok(Expression::And(vec![])));
    assert_eq!(parse_expression("  \r\n"), Ok(Expression::And(vec![])));
}

#[test]
fn parsing_unicode() {
    assert_eq!(
        parse_expression("žlabava"),
        Ok(Expression::Term {
            field: None,
            function: Func::Matches,
            value: TermValue::text("žlabava")
        })
    );
}

#[test]
fn parsing_quoted_unicode() {
    assert_eq!(
        parse_expression("\"žlabava mává\""),
        Ok(Expression::Term {
            field: None,
            function: Func::Matches,
            value: TermValue::text("žlabava mává")
        })
    );
}

#[test]
fn parsing_quoted_formula() {
    assert_eq!(
        parse_expression("\"(a₀, a₁, …, aₙ)\""),
        Ok(Expression::Term {
            field: None,
            function: Func::Matches,
            value: TermValue::text("(a₀, a₁, …, aₙ)")
        })
    );
}

#[test]
fn parsing_quoted_emojis() {
    assert_eq!(
        parse_expression("\"cool😎🤓that o👨🏻‍❤️‍💋‍👨🏻o\""),
        Ok(Expression::Term {
            field: None,
            function: Func::Matches,
            value: TermValue::text("cool😎🤓that o👨🏻‍❤️‍💋‍👨🏻o")
        })
    );
}

#[test]
fn parsing_unquoted_emojis_not_supported() {
    // Emoji category includes for instance * and #.
    // To support unquoted emojis, we need more research
    // TODO: add unquoted emoji support to query parser
    assert!(parse_expression("👨").is_err());
    assert!(parse_expression("👨🏻‍❤️‍💋‍👨🏻").is_err());
}
