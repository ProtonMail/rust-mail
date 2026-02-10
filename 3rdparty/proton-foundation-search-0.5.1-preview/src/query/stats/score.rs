use tracing::{instrument, trace};

use crate::query::results::Score;

impl Score {
    /// Harmonize the entry score relative to the collection
    ///
    /// entries: total number of entries in collection/attribute
    /// average_size: average entry/attribute size in number of tokens
    /// frequency: how often does a token appear per entry/attribute
    /// size: size of this entry/attribute in number of tokens
    /// occurrences: total number of times term appears in this entry/attribute
    /// matching_score: the quality of the token match on the scale 0-1
    pub fn harmonize(
        &self,
        entries: usize,
        average_size: f64,
        frequency: f64,
        size: usize,
        occurrences: usize,
    ) -> Score {
        bm25_new(
            entries,
            average_size,
            frequency,
            size,
            occurrences,
            self.value(),
        )
        .into()
    }
}

#[instrument]
fn bm25_new(
    entries: usize,
    average_size: f64,
    frequency: f64,
    size: usize,
    occurrences: usize,
    matching_score: f64,
) -> f64 {
    let idf = idf(entries as f64, frequency);
    let tf = tf(occurrences as f64, size as f64, average_size);
    // helps to avoid too high a score
    let normalizer = (3.0 + entries as f64).ln();
    let score = matching_score * idf * tf / normalizer;
    trace!(tf, idf, normalizer, score);
    score
}

fn idf(entries: f64, frequency: f64) -> f64 {
    // larger values to avoid negative score
    let small = 2.0;
    let fix = 2.0;
    (fix + (entries - frequency + small) / (frequency + small)).ln()
}

fn tf(occurrences: f64, size: f64, avg_size: f64) -> f64 {
    let k1 = 1.5;
    let b = 0.75;
    (occurrences * (k1 + 1.0)) / (occurrences + k1 * (1.0 - b + b * (size / avg_size)))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// entries: total number of entries in collection/attribute
    /// average_size: average entry/attribute size in number of tokens
    /// frequency: how often does a token appear per entry/attribute
    /// size: size of this entry/attribute in number of tokens
    /// occurrences: total number of times term appears in this entry/attribute
    /// matching_score: the quality of the token match on the scale 0-1
    fn bm25_old(
        entries: usize,
        average_size: f64,
        frequency: f64,
        size: usize,
        occurrences: usize,
        matching_score: f64,
    ) -> f64 {
        let total_size = entries as f64 * average_size;
        let total_occurrences = entries as f64 * frequency;

        // Compute global term importance (weight)
        let weight = compute_proxy_weight(total_size, total_occurrences);

        //entry_freq: number of times term appears in this entry+attribute
        //entry_length: length of document d's attribute a
        // Compute proxy score
        let proxy_score = compute_proxy_score(occurrences as f64, size as f64, weight);

        // Combine with matching score (similar to how we do with BM25)
        let score = LEV_ALPHA * proxy_score + (1.0 - LEV_ALPHA) * matching_score;

        trace!(
            weight,
            total_size, total_occurrences, proxy_score, occurrences, size, score
        );

        score
    }

    const LEV_ALPHA: f64 = 0.6;

    // Constants for proxy BM25 scoring
    const PROXY_ALPHA: f64 = 0.5; // scaling exponent for length normalization
    const PROXY_DELTA: f64 = 0.1; // small constant to prevent zero weights for very common terms

    /// Computes the global term importance (weight) for a term in a specific attribute.
    /// This is the proxy equivalent of IDF, but with a different formula that allows for harmonization.
    ///
    /// The formula used is:
    /// weight(t, a) = log((1 + N) / (1 + n_t_a) + δ)
    ///
    /// Where:
    /// - N = collection_size: total number of documents in the collection
    /// - n_t_a = collection_term_freq: number of documents containing the term in attribute a
    /// - δ = PROXY_DELTA: small constant to prevent zero weights for very common terms
    fn compute_proxy_weight(collection_size: f64, collection_term_freq: f64) -> f64 {
        ((1.0 + collection_size) / (1.0 + collection_term_freq) + PROXY_DELTA).ln()
    }

    /// Computes the proxy BM25 score for a document-term-attribute combination.
    ///
    /// The formula used is:
    /// proxy_score(d, t, a) = (log(1 + f(t,d,a)) / |d_a|^α) · weight(t, a)
    ///
    /// Where:
    /// - f(t,d,a) = entry_term_freq: term frequency in document d's attribute a
    /// - |d_a| = entry_length: length of document d's attribute a
    /// - α = PROXY_ALPHA: scaling exponent for length normalization
    /// - weight(t, a) = global term importance from compute_proxy_weight
    fn compute_proxy_score(entry_term_freq: f64, entry_length: f64, weight: f64) -> f64 {
        let term_freq_saturation = (1.0 + entry_term_freq).ln();
        let length_normalization = entry_length.powf(PROXY_ALPHA);
        (term_freq_saturation / length_normalization) * weight
    }

    #[test]
    fn theoretical_bm25() {
        let mut cases = vec![
            // few entries, larger than average doc, poor matches
            // low score
            (3, 50.0, 3.0, 100, 2, 1.00000001, f64::NAN, f64::NAN),
            // few entries, larger than average doc, rarer matches
            // average score
            (3, 50.0, 0.5, 100, 2, 1.00000001, f64::NAN, f64::NAN),
            // few entries, smaller than average doc, rarer matches
            // higher score favouring smaller matching docs
            (3, 50.0, 0.5, 10, 1, 1.00000001, f64::NAN, f64::NAN),
            // few disproportionate entries, tiny doc, very rare match (extreme)
            // this should get a very high score
            (3, 50123.0, 0.5, 10, 1, 1.00000001, f64::NAN, f64::NAN),
            // few disproportionate entries, tiny doc, very poor match (extreme)
            // this should get very low score - searching for common words such as "the"
            (3, 50123.0, 300.0, 10, 1, 1.00000001, f64::NAN, f64::NAN),
            // many similar entries, average doc, good match
            // this should get a good score
            (30000, 503.0, 0.5, 500, 1, 1.00000001, f64::NAN, f64::NAN),
        ];
        for case in &mut cases {
            let (entries, average_size, frequency, size, occurrences, matching_score, ..) = *case;
            let (_, _, _, _, _, _, old, new) = case;

            *old = bm25_old(
                entries,
                average_size,
                frequency,
                size,
                occurrences,
                matching_score,
            );
            *new = bm25_new(
                entries,
                average_size,
                frequency,
                size,
                occurrences,
                matching_score,
            );
        }
        insta::assert_debug_snapshot!(cases, @r"
        [
            (
                3,
                50.0,
                3.0,
                100,
                2,
                1.00000001,
                0.5793789198880881,
                0.5282253085585928,
            ),
            (
                3,
                50.0,
                0.5,
                100,
                2,
                1.00000001,
                0.6704328690111265,
                0.8054900424338947,
            ),
            (
                3,
                50.0,
                0.5,
                10,
                1,
                1.00000001,
                0.939560912987379,
                1.1641848269552384,
            ),
            (
                3,
                50123.0,
                0.5,
                10,
                1,
                1.00000001,
                1.8472701266933145,
                1.3544667025293835,
            ),
            (
                3,
                50123.0,
                300.0,
                10,
                1,
                1.00000001,
                1.0730891332711567,
                0.02324837208768674,
            ),
            (
                30000,
                503.0,
                0.5,
                500,
                1,
                1.00000001,
                0.528589841342159,
                0.9135811347184613,
            ),
        ]
        ");
    }
}
