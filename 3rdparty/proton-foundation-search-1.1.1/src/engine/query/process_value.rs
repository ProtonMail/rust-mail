use super::*;

impl TermValue {
    /// Process the query term using the processor, splitting it up as necessary
    pub fn process(&self, processor: &dyn Processor) -> Vec<TermValue> {
        let whitespace = |c: char| c.is_whitespace();
        let mut join_next = false;
        let mut results: Vec<TermValue> = vec![];
        let Some(parts) = self.wildcard_text() else {
            return vec![self.clone()];
        };
        for part in parts {
            match part {
                TermValuePart::Wildcard => {
                    if join_next {
                        if let Some(last) = results.pop() {
                            results.push(last.wildcard());
                        } else {
                            results.push(TermValue::wild());
                        }
                    } else {
                        results.push(TermValue::wild());
                    }
                }
                TermValuePart::Text(text) => {
                    // if the text doesn't end with whitespace,
                    // the next part that follows shall be joined with it.
                    join_next = !text.ends_with(whitespace);
                    // if the text doesn't start with whitespace,
                    // this part should be joined with previous one.
                    let join_current = !text.starts_with(whitespace);
                    let mut tokens = processor.process_query_term(text.as_ref());
                    let mut end = 0;
                    if join_current
                        && !tokens.is_empty()
                        && let Some(last) = results.pop()
                    {
                        let (pos, first) = tokens.remove(0);
                        results.push(last.then(first.as_ref()));
                        end = pos + first.len();
                    }

                    for (pos, token) in tokens {
                        let skipped = &text[end..pos];
                        end = pos + token.len();

                        if !skipped.is_empty() && !skipped.starts_with(whitespace) {
                            // if the skipped text has non-whitespace at the start,
                            // such as "abc- def", then add wildcard after "abc"
                            if let Some(last) = results.pop() {
                                results.push(last.wildcard());
                            }
                        }

                        if skipped.contains(whitespace) {
                            // tokens were split (also) on whitespace => separate terms
                            if !skipped.is_empty() && !skipped.ends_with(whitespace) {
                                // if the skipped text has non-whitespace at the end,
                                // such as "abc -def", then add wildcard before "def"
                                results.push(TermValue::wild().then(&token));
                            } else {
                                results.push(token.into());
                            }
                        } else if !skipped.is_empty() {
                            // tokens were split on non-whitespace => single term
                            if let Some(last) = results.pop() {
                                results.push(last.wildcard().then(&token));
                            }
                        } else {
                            // no token splitting
                            results.push(token.into());
                        }
                    }

                    // handle trailing wildcard replacement, such as "other-"
                    let skipped = &text[end..];
                    if !skipped.is_empty()
                        && !skipped.starts_with(whitespace)
                        && let Some(last) = results.pop()
                    {
                        results.push(last.wildcard());
                    }
                }
            }
        }
        results
    }
}
