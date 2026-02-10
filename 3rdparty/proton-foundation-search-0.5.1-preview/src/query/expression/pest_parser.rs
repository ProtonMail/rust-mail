use std::str::FromStr;

use pest::Parser;
use pest_derive::Parser;

use super::{Expression, Func};
use crate::document::Value;

#[cfg(test)]
#[path = "pest_test.rs"]
mod pest_test;

/// Parse query expressions from strings
impl FromStr for Expression {
    type Err = QueryParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_to_expression(input).map_err(QueryParseError::new)
    }
}

#[derive(Parser)]
#[grammar = "src/query/expression/grammar.pest"]
struct QueryParser;

/// Error while parsing a query expression
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Failed to parse the query: {message}")]
pub struct QueryParseError {
    message: Box<str>,
}

impl QueryParseError {
    fn new(message: impl ToString) -> Self {
        Self {
            message: message.to_string().into_boxed_str(),
        }
    }
}

/// Convert Pest parse tree to Expression AST
pub fn parse_to_expression(input: &str) -> Result<Expression, String> {
    let pairs =
        QueryParser::parse(Rule::query, input).map_err(|e| format!("Parse error: {}", e))?;

    let pair = pairs.into_iter().next().ok_or("Empty parse result")?;
    pair.to_expression()
}

/// Generic trait for converting Pest pairs to Expressions
trait ToExpression {
    fn to_expression(self) -> Result<Expression, String>;
}

impl ToExpression for pest::iterators::Pair<'_, Rule> {
    fn to_expression(self) -> Result<Expression, String> {
        match self.as_rule() {
            // Accept empty expression
            Rule::EOI => Ok(Expression::And(vec![])),
            // Pass-through rules
            Rule::query | Rule::expression => {
                // For query/expression, handle optional content (empty queries)
                match self.into_inner().next() {
                    Some(pair) => pair.to_expression(),
                    None => Err("Empty query".to_string()),
                }
            }
            Rule::primary_expr | Rule::term_expr => self
                .into_inner()
                .next()
                .ok_or("Empty rule")?
                .to_expression(),

            // Binary operators
            Rule::or_seq => self.convert_binary_operator(Expression::or),
            Rule::and_seq => self.convert_binary_operator(Expression::and),

            // Unary operators
            Rule::not_expr => {
                let expr = self
                    .into_inner()
                    .next()
                    .ok_or("Empty NOT")?
                    .to_expression()?;
                Ok(Expression::Not(Box::new(expr)))
            }

            // Grouping
            Rule::group_expr => self
                .into_inner()
                .next()
                .ok_or("Empty group")?
                .to_expression(),

            // Term types
            Rule::field_term => {
                let mut pairs = self.into_inner();
                let field = pairs.next().ok_or("Missing field")?.as_str();
                let operator = pairs.next().ok_or("Missing operator")?.as_str();
                let value = pairs.next().ok_or("Missing value")?.to_value();
                let (function, value) = fix_prefix(operator, value)?;
                Ok(Expression::Term {
                    field: Some(field.into()),
                    function,
                    value,
                })
            }
            // Term types
            Rule::operator_term => {
                let mut pairs = self.into_inner();
                let operator = pairs.next().ok_or("Missing operator")?.as_str();
                let value = pairs.next().ok_or("Missing value")?.to_value();
                let (function, value) = fix_prefix(operator, value)?;
                Ok(Expression::Term {
                    field: None,
                    function,
                    value,
                })
            }
            Rule::simple_term => {
                let value = self
                    .into_inner()
                    .next()
                    .ok_or("Empty simple term")?
                    .to_value();
                let (function, value) = fix_prefix("~", value)?;
                Ok(Expression::Term {
                    field: None,
                    function,
                    value,
                })
            }

            _ => Err(format!("Unexpected rule: {:?}", self.as_rule())),
        }
    }
}

/// Todo: this should be replaced with wildcard support
fn fix_prefix(operator: &str, value: Value) -> Result<(Func, Value), String> {
    // Convert "~" with trailing wildcard to Prefix function
    Ok(if operator == "~" {
        if let Value::Text(text) = &value {
            if let Some(prefix) = text.as_ref().strip_suffix('*') {
                // field~value* → Prefix(value)
                (Func::Prefix, Value::text(prefix))
            } else {
                (parse_operator(operator)?, value)
            }
        } else {
            (parse_operator(operator)?, value)
        }
    } else {
        (parse_operator(operator)?, value)
    })
}

/// Extension trait for additional conversion methods
trait PairExt {
    fn convert_binary_operator<F>(self, operator: F) -> Result<Expression, String>
    where
        F: Fn(Expression, Expression) -> Expression;
}

impl PairExt for pest::iterators::Pair<'_, Rule> {
    /// Convert binary operators (AND, OR) with their operands using iterator methods
    fn convert_binary_operator<F>(self, operator: F) -> Result<Expression, String>
    where
        F: Fn(Expression, Expression) -> Expression,
    {
        let mut pairs = self.into_inner();
        let mut expression = pairs
            .next()
            .ok_or("Empty binary expression")?
            .to_expression()?;

        // Process remaining pairs
        for item in pairs {
            match item.as_rule() {
                Rule::and_op | Rule::or_op => {
                    // Skip explicit operators
                }
                _ => {
                    // Treat as operand (WHITESPACE is silent, won't appear here)
                    expression = operator(expression, item.to_expression()?)
                }
            }
        }
        Ok(expression)
    }
}

/// Trait for converting Pest pairs to Values
trait ToValue {
    fn to_value(self) -> Value;
}

impl ToValue for pest::iterators::Pair<'_, Rule> {
    fn to_value(self) -> Value {
        assert_eq!(self.as_rule(), Rule::value);

        let string = self
            .clone()
            .into_inner()
            .flat_map(|string| string.into_inner())
            .flat_map(|string| string.into_inner())
            .map(|token| match token.as_rule() {
                Rule::QSTRING | Rule::USTRING => token.as_str(),
                Rule::ESCAPED_CHAR => &token.as_str()[1..],
                // TODO: special treatment for wildcards
                Rule::WILDCARD => token.as_str(),
                _ => unreachable!("Invalid token {token:?}"),
            })
            .collect::<String>();

        println!("{:?} {:?}\n{self:?}", self.as_str(), string);

        string
            .parse()
            .map(Value::Boolean)
            .or_else(|_| string.parse().map(Value::Integer))
            .unwrap_or_else(|_| Value::text(string))
    }
}

/// Parse function name to Func enum
fn parse_operator(op_str: &str) -> Result<Func, String> {
    match op_str {
        "~" => Ok(Func::Matches),
        "=" => Ok(Func::Equals),
        "<" => Ok(Func::LessThan),
        "<=" => Ok(Func::LessThanOrEqual),
        ">" => Ok(Func::GreaterThan),
        ">=" => Ok(Func::GreaterThanOrEqual),
        _ => Err(format!("Unknown operator: {}", op_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_expression() {
        let result = parse_to_expression("hello").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: None,
                function: Func::Matches,
                value: Value::text("hello"),
            }
        );
    }

    #[test]
    fn test_and_expression() {
        let result = parse_to_expression("hello AND world").unwrap();
        assert_eq!(
            result,
            Expression::And(vec![
                Expression::Term {
                    field: None,
                    function: Func::Matches,
                    value: Value::text("hello"),
                },
                Expression::Term {
                    field: None,
                    function: Func::Matches,
                    value: Value::text("world"),
                },
            ])
        );
    }

    #[test]
    fn test_or_expression() {
        let result = parse_to_expression("hello OR world").unwrap();
        assert_eq!(
            result,
            Expression::Or(vec![
                Expression::Term {
                    field: None,
                    function: Func::Matches,
                    value: Value::text("hello"),
                },
                Expression::Term {
                    field: None,
                    function: Func::Matches,
                    value: Value::text("world"),
                },
            ])
        );
    }

    #[test]
    fn test_complex_expression() {
        let result = parse_to_expression("hello AND world OR foo AND bar").unwrap();
        assert_eq!(
            result,
            Expression::Or(vec![
                Expression::And(vec![
                    Expression::Term {
                        field: None,
                        function: Func::Matches,
                        value: Value::text("hello"),
                    },
                    Expression::Term {
                        field: None,
                        function: Func::Matches,
                        value: Value::text("world"),
                    },
                ]),
                Expression::And(vec![
                    Expression::Term {
                        field: None,
                        function: Func::Matches,
                        value: Value::text("foo"),
                    },
                    Expression::Term {
                        field: None,
                        function: Func::Matches,
                        value: Value::text("bar"),
                    },
                ]),
            ])
        );
    }

    #[test]
    fn test_field_matches() {
        let result = parse_to_expression("title~hello").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("title".into()),
                function: Func::Matches,
                value: Value::text("hello"),
            }
        );
    }

    #[test]
    fn test_field_equals() {
        let result = parse_to_expression("title=hello").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("title".into()),
                function: Func::Equals,
                value: Value::text("hello"),
            }
        );
    }

    #[test]
    fn test_field_less_than() {
        let result = parse_to_expression("age<18").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("age".into()),
                function: Func::LessThan,
                value: Value::Integer(18),
            }
        );
    }

    #[test]
    fn test_field_greater_than_or_equal() {
        let result = parse_to_expression("score>=100").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("score".into()),
                function: Func::GreaterThanOrEqual,
                value: Value::Integer(100),
            }
        );
    }

    #[test]
    fn test_quoted_string() {
        let result = parse_to_expression("\"hello world\"").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: None,
                function: Func::Matches,
                value: Value::text("hello world"),
            }
        );
    }

    #[test]
    fn test_number() {
        let result = parse_to_expression("123").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: None,
                function: Func::Matches,
                value: Value::Integer(123),
            }
        );
    }

    #[test]
    fn test_boolean() {
        let result = parse_to_expression("true").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: None,
                function: Func::Matches,
                value: Value::Boolean(true),
            }
        );
    }

    #[test]
    fn test_not_expression() {
        let result = parse_to_expression("!hello").unwrap();
        assert_eq!(
            result,
            Expression::Not(Box::new(Expression::Term {
                field: None,
                function: Func::Matches,
                value: Value::text("hello"),
            }))
        );
    }

    #[test]
    fn test_grouped_expression() {
        let result = parse_to_expression("(hello OR world)").unwrap();
        assert_eq!(
            result,
            Expression::Or(vec![
                Expression::Term {
                    field: None,
                    function: Func::Matches,
                    value: Value::text("hello"),
                },
                Expression::Term {
                    field: None,
                    function: Func::Matches,
                    value: Value::text("world"),
                },
            ])
        );
    }

    #[test]
    fn test_wildcard_suffix() {
        // Trailing wildcard is converted to Prefix function
        let result = parse_to_expression("title~hom*").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("title".into()),
                function: Func::Prefix,
                value: Value::text("hom"),
            }
        );
    }

    #[test]
    fn test_wildcard_prefix() {
        let result = parse_to_expression("author~*smith").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("author".into()),
                function: Func::Matches,
                value: Value::text("*smith"),
            }
        );
    }

    #[test]
    fn test_wildcard_middle() {
        let result = parse_to_expression("subject~pre*post").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: Some("subject".into()),
                function: Func::Matches,
                value: Value::text("pre*post"),
            }
        );
    }

    #[test]
    fn test_empty_query_error() {
        // Empty queries should return an error
        let result = parse_to_expression("");
        assert_eq!(result, Ok(Expression::And(vec![])));

        // Whitespace-only queries should also return an error
        let result = parse_to_expression("   ");
        assert_eq!(result, Ok(Expression::And(vec![])));
    }
}
