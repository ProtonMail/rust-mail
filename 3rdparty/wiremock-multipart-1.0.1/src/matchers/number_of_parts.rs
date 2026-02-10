use wiremock::{Match, Request};

use crate::request_utils::RequestUtils;

/// Match a multipart/form-data request with this many individual parts.
pub struct NumberOfParts(pub usize);

impl Match for NumberOfParts {
    fn matches(&self, request: &Request) -> bool {
        request.parts().len() == self.0
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use maplit::hashmap;

    use crate::test_utils::*;

    use super::*;

    #[test]
    fn should_compare_number_of_parts_with_expectation() {
        let request = requestb(
            hashmap!{
                name("content-type") => values("multipart/form-data; boundary=xyz"),
            },
        indoc!{"
                --xyz
                Content-Disposition: form-data; name=part1

                content
                --xyz--
            "}.as_bytes().into(),
        );

        assert_eq!(NumberOfParts(0).matches(&request), false);
        assert_eq!(NumberOfParts(1).matches(&request), true);
        assert_eq!(NumberOfParts(2).matches(&request), false);
    }
}
