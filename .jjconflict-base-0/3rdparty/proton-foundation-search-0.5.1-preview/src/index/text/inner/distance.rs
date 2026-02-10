pub(super) use distance::levenshtein;

// Keep in mind that this normalization causes the output to no longer
// be usable as a proper metric as it violates the triangle inequality:
// https://en.wikipedia.org/wiki/Triangle_inequality
// Also ref https://rapidfuzz.github.io/Levenshtein/levenshtein.html#ratio
// Why do we compare the distance to max length, rather than to the sum of lengths?
pub(super) fn levenshtein_ratio(s: &str, t: &str, distance: usize) -> f64 {
    1.0 - ((distance as f64) / s.len().max(t.len()) as f64)
}
