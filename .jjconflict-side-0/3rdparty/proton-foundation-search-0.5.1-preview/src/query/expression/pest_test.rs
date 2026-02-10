#[cfg(test)]
mod tests {
    use pest::Parser;

    use crate::query::expression::pest_parser::{QueryParser, Rule};

    // Helper function to format parse tree for snapshots
    fn format_tree(pair: pest::iterators::Pair<'_, Rule>) -> String {
        fn format_inner(pair: pest::iterators::Pair<'_, Rule>, depth: usize) -> String {
            let prefix = "  ".repeat(depth);
            let mut result = format!("{}{:?}: '{}'\n", prefix, pair.as_rule(), pair.as_str());
            for inner in pair.into_inner() {
                result.push_str(&format_inner(inner, depth + 1));
            }
            result
        }
        format_inner(pair, 0)
    }

    #[test]
    fn test_simple_expression() {
        let test_input = "hello";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("simple_expression", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_and_expression() {
        let test_input = "hello AND world";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("and_expression", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_or_expression() {
        let test_input = "hello OR world";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("or_expression", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_complex_expression() {
        let test_input = "hello AND world OR foo AND bar";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("complex_expression", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_implicit_and_expression() {
        // Tests implicit AND via whitespace
        let test_input = "hello world";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("implicit_and_expression", tree);
                }
            }
            Err(e) => {
                panic!("Expected to parse implicit AND, but got error: {}", e);
            }
        }
    }

    #[test]
    fn test_text_expression_with_numbers() {
        // Tests implicit AND with alphanumeric terms
        let test_input = "amd64 64bit";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("text_expression_with_numbers", tree);
                }
            }
            Err(e) => {
                panic!("Expected to parse, but got error: {}", e);
            }
        }
    }

    #[test]
    fn test_quoted_string() {
        let test_input = "\"hello world\"";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("quoted_string", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_implicit_and_with_extra_space() {
        let test_input = "hello  world"; // Extra space

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("implicit_and_with_extra_space", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_quoted_string_with_escape_sequences() {
        // Test escape sequences in quoted string
        let test_input = r#""hello\nworld\ttab""#;

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("quoted_string_with_escapes", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_unquoted_string_with_escape() {
        // Test escape sequence in unquoted string (escaped space)
        let test_input = r#"hello\ world"#;

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("unquoted_string_with_escape", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_and_seq_rule_directly() {
        // Test and_seq rule directly
        let test_input = "hello world";

        match QueryParser::parse(Rule::and_seq, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("and_seq_rule_directly", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_empty_string() {
        let test_input = "";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("empty_string", tree);
                }
            }
            Err(e) => {
                panic!("Should parse empty string successfully, got error: {}", e);
            }
        }
    }

    #[test]
    fn test_whitespace_only() {
        let test_input = "   ";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("whitespace_only", tree);
                }
            }
            Err(e) => {
                panic!(
                    "Should parse whitespace-only string successfully, got error: {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_whitespace_trimming() {
        let test_input = "   elided   ";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("whitespace_trimming", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_truethy_string() {
        // Test that "truethy" is parsed as a string, not as boolean "true"
        let test_input = "truethy";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("truethy_string", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_wildcard_suffix() {
        // Test wildcard at end of unquoted string
        let test_input = "hello*";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("wildcard_suffix", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_wildcard_prefix() {
        // Test wildcard at start of unquoted string
        let test_input = "*world";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("wildcard_prefix", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_wildcard_middle() {
        // Test wildcard in middle of unquoted string
        let test_input = "hello*world";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("wildcard_middle", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_wildcard_in_quoted_string() {
        // Test wildcard inside quoted string
        let test_input = r#""hello*world""#;

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("wildcard_in_quoted_string", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_multiple_wildcards() {
        // Test multiple wildcards in one string
        let test_input = "hello*world*test";

        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("multiple_wildcards", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_escaped_expression() {
        use crate::document::Value;
        use crate::query::expression::pest_parser::parse_to_expression;
        use crate::query::expression::{Expression, Func};

        // Test escaped space in unquoted string (must start with letter)
        let result = parse_to_expression("he\\ llo").unwrap();
        assert_eq!(
            result,
            Expression::Term {
                field: None,
                function: Func::Matches,
                value: Value::text("he llo"),
            }
        );
    }

    #[test]
    fn test_operator_matches() {
        // Test the ~ (matches) operator with a field
        let test_input = "title~hello";
        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("operator_matches", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_operator_equals() {
        // Test the = (equals) operator with a field
        let test_input = "status=active";
        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("operator_equals", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_operator_less_than() {
        // Test the < (less than) operator with a field
        let test_input = "age<18";
        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("operator_less_than", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }

    #[test]
    fn test_operator_greater_than_or_equal() {
        // Test the >= (greater than or equal) operator with a field
        let test_input = "score>=100";
        match QueryParser::parse(Rule::query, test_input) {
            Ok(mut pairs) => {
                if let Some(pair) = pairs.next() {
                    let tree = format_tree(pair);
                    insta::assert_snapshot!("operator_greater_than_or_equal", tree);
                }
            }
            Err(e) => {
                panic!("Parse error: {}", e);
            }
        }
    }
}
