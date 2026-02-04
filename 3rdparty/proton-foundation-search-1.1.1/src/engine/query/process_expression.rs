use super::*;

impl Expression {
    /// process the query using a processor, returns true when there is any expression remaining
    pub fn process(&mut self, processor: &dyn Processor) -> bool {
        match self {
            Expression::And(expressions) | Expression::Or(expressions) => {
                expressions.retain_mut(|exp| exp.process(processor));
                !expressions.is_empty()
            }
            Expression::Not(expression) => expression.process(processor),
            // tokenize the fuzzy (Matches) search text
            Expression::Term {
                field,
                function: function @ Func::Matches,
                value,
            } => {
                let mut expressions = value
                    .process(processor)
                    .into_iter()
                    .map(|value| Expression::Term {
                        field: field.clone(),
                        function: *function,
                        value,
                    })
                    .collect::<Vec<_>>();
                *self = if expressions.len() == 1 {
                    #[allow(clippy::unwrap_used, reason = "there is exactly one item")]
                    expressions.pop().unwrap()
                } else {
                    Expression::And(expressions)
                };
                !self.is_empty()
            }
            Expression::Term { .. } => true,
        }
    }
}
