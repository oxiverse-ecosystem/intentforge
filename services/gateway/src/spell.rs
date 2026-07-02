// ─── Spelling Correction: SymSpell + LinSpell ───────────────────────
// Two-stage approach:
//   Stage 1 (SymSpell): O(1) hash-based lookup using pre-computed delete
//     variations of all dictionary words up to max edit distance.
//   Stage 2 (LinSpell): O(n) linear scan fallback for near-miss words
//     where SymSpell didn't find a high-confidence candidate.
//
// For each word in a query, we check:
//   1. Exact match in dictionary → keep original
//   2. SymSpell candidate with edit distance 1 → auto-correct
//   3. SymSpell candidate with edit distance 2 → use if confidence > threshold
//   4. LinSpell fallback → linear scan with early termination
//
// No external dependencies beyond the bundled dictionary.

use std::collections::HashMap;

/// Maximum edit distance for SymSpell pre-computation
const MAX_EDIT_DISTANCE: usize = 2;

/// Minimum word length to attempt correction (avoid correcting short words)
/// Set to 4 to avoid false positives on 3-letter words (doc→down, app→api).
/// All common 3-letter tech terms (npm, pip, git, vue, etc.) are in the dict.
const MIN_CORRECT_LENGTH: usize = 4;

/// Minimum frequency ratio for auto-correction (corrected word must be
/// more common than 5% of the most common word)
const MIN_FREQ_THRESHOLD: f64 = 0.001;

/// SymSpell index: delete-variation → list of (original_word, frequency)
pub(crate) struct SymSpellIndex {
    /// Maps a deletion string → list of (word_id, frequency) that could produce it
    deletions: HashMap<String, Vec<(u32, f64)>>,
    /// Original words indexed by ID
    words: Vec<String>,
    /// Frequencies indexed by ID
    frequencies: Vec<f64>,
    /// Quick lookup: word → word_id
    exact_map: HashMap<String, u32>,
    /// Character bigram language model for detecting tech-like words
    char_bigram_model: CharBigramModel,
}

impl SymSpellIndex {
    /// Build the SymSpell index from the embedded word frequency dictionary
    pub(crate) fn build() -> Self {
        let mut words: Vec<String> = Vec::new();
        let mut frequencies: Vec<f64> = Vec::new();
        let mut exact_map: HashMap<String, u32> = HashMap::new();
        let mut deletions: HashMap<String, Vec<(u32, f64)>> = HashMap::new();

        for (word, freq) in crate::dictionary::WORD_FREQUENCIES {
            let word_lower = word.to_lowercase();
            let word_id = words.len() as u32;
            words.push(word_lower.clone());
            frequencies.push(*freq);
            exact_map.insert(word_lower.clone(), word_id);

            // Generate all deletion variations up to MAX_EDIT_DISTANCE
            let word_chars: Vec<char> = word_lower.chars().collect();
            let len = word_chars.len();

            // Only generate delete variations for words >= 3 chars
            if len < 3 {
                continue;
            }

            // Generate delete variations for edit distance 1
            for i in 0..len {
                let mut deleted = String::with_capacity(len - 1);
                for j in 0..len {
                    if j != i {
                        deleted.push(word_chars[j]);
                    }
                }
                deletions
                    .entry(deleted)
                    .or_default()
                    .push((word_id, *freq));
            }

            // Generate delete variations for edit distance 2 (if word is long enough)
            if len >= 4 && MAX_EDIT_DISTANCE >= 2 {
                for i in 0..len {
                    for j in (i + 1)..len {
                        let mut deleted = String::with_capacity(len - 2);
                        for k in 0..len {
                            if k != i && k != j {
                                deleted.push(word_chars[k]);
                            }
                        }
                        deletions
                            .entry(deleted)
                            .or_default()
                            .push((word_id, *freq));
                    }
                }
            }
        }

        // Build character bigram model from dictionary words for tech-term detection
        let char_bigram_model = CharBigramModel::build(&words);

        tracing::info!(
            "SymSpell index built: {} words, {} deletion entries, char-bigram median_perp={:.2}",
            words.len(),
            deletions.len(),
            char_bigram_model.reference_perplexity
        );

        Self {
            deletions,
            words,
            frequencies,
            exact_map,
            char_bigram_model,
        }
    }

    /// Find best correction for a word.
    /// Returns `None` if the word is already correct or no good candidate found.
    pub(crate) fn correct(&self, word: &str) -> Option<String> {
        let word_lower = word.to_lowercase();
        if word_lower.len() < MIN_CORRECT_LENGTH {
            return None; // Don't correct very short words
        }

        // Stage 1: Exact match check — but skip very-low-frequency words
        // (like misspelling entries in the dictionary at freq 0.001) so they
        // fall through to SymSpell and get corrected to their proper form.
        let input_freq = self.exact_map.get(&word_lower).map(|&id| self.frequencies[id as usize]);
        if let Some(freq) = input_freq {
            if freq >= MIN_FREQ_THRESHOLD * 10.0 {
                return None; // Common enough → no correction needed
            }
        }

        // Stage 2: SymSpell lookup
        let symspell_result = self.symspell_lookup(&word_lower);

        // Stage 3: LinSpell fallback
        let linspell_result = self.linspell_lookup(&word_lower);

        // Pick the best candidate between SymSpell and LinSpell.
        // When the input word is a known misspelling entry (in dictionary at
        // freq < 0.01), a dist-2 word that's much more common may beat a
        // dist-1 misspelling entry. For true misspellings (input not in dict),
        // lower edit distance always wins.
        let candidate_freq = |w: &str| -> f64 {
            self.exact_map.get(w).map(|&id| self.frequencies[id as usize]).unwrap_or(0.0)
        };
        let best = match (&symspell_result, &linspell_result) {
            (Some(s), Some(l)) => {
                let sf = candidate_freq(s);
                let lf = candidate_freq(l);
                // Only override SymSpell result with LinSpell when SymSpell's
                // candidate is a misspelling entry (freq < 0.01) and LinSpell's
                // is >= 5x more common.
                if sf < MIN_FREQ_THRESHOLD * 10.0 && lf > sf * 5.0 { l } else { s }
            }
            (Some(s), None) => s,
            (None, Some(l)) => l,
            (None, None) => return None,
        };

        // Only accept if candidate is at least as common as the original word
        // (prevents correcting a legitimate rare word to a rarer misspelling)
        if input_freq.is_none_or(|f| candidate_freq(best) >= f) {
            Some(best.clone())
        } else {
            None
        }
    }

    /// Check if a word is a known misspelling entry (in the dictionary
    /// at a very low frequency below the normal threshold).
    /// Such words are explicitly there to be corrected, so the perplexity
    /// ratio guard should not block their corrections.
    fn is_known_misspelling(&self, word: &str) -> bool {
        if let Some(&word_id) = self.exact_map.get(word) {
            self.frequencies[word_id as usize] < MIN_FREQ_THRESHOLD * 10.0
        } else {
            false
        }
    }

    /// SymSpell O(1) lookup: generate deletions of the input word and check
    /// against the pre-computed index.
    fn symspell_lookup(&self, word: &str) -> Option<String> {
        let chars: Vec<char> = word.chars().collect();
        let len = chars.len();

        // Generate deletion variations of the input word and look them up
        let mut candidates: Vec<(u32, f64, usize)> = Vec::new(); // (word_id, freq, edit_dist)
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();

        // Check edit distance 1 deletions
        for i in 0..len {
            let mut deleted = String::with_capacity(len - 1);
            for j in 0..len {
                if j != i {
                    deleted.push(chars[j]);
                }
            }
            if let Some(entries) = self.deletions.get(&deleted) {
                for &(word_id, freq) in entries {
                    if seen.insert(word_id) {
                        let edit_dist = self.compute_edit_distance(word, &self.words[word_id as usize]);
                        if edit_dist <= MAX_EDIT_DISTANCE {
                            candidates.push((word_id, freq, edit_dist));
                        }
                    }
                }
            }
        }

        // Check edit distance 2 deletions (if word is long enough)
        if len >= 4 {
            for i in 0..len {
                for j in (i + 1)..len {
                    let mut deleted = String::with_capacity(len - 2);
                    for k in 0..len {
                        if k != i && k != j {
                            deleted.push(chars[k]);
                        }
                    }
                    if let Some(entries) = self.deletions.get(&deleted) {
                        for &(word_id, freq) in entries {
                            if seen.insert(word_id) {
                                let edit_dist = self.compute_edit_distance(word, &self.words[word_id as usize]);
                                if edit_dist <= MAX_EDIT_DISTANCE {
                                    candidates.push((word_id, freq, edit_dist));
                                }
                            }
                        }
                    }
                }
            }
        }

        if candidates.is_empty() {
            return None;
        }

        // Sort by: edit_distance asc, frequency desc
        candidates.sort_by(|a, b| {
            a.2.cmp(&b.2).then_with(|| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        let best = &candidates[0];

        // Use perplexity ratio to detect tech-term→English false positives.
        // If the input word has much higher character bigram perplexity than
        // the candidate (> 1.4x with the 15k-word dictionary), the correction
        // is likely English-ifying a tech term.
        //
        // EXCEPTION: if the input word is a known misspelling entry (explicitly
        // added to the dictionary at freq < 0.01), we skip the guard — the word
        // was put there specifically to be corrected to its proper form.
        let best_word = &self.words[best.0 as usize];
        let perp_ratio = self.char_bigram_model.perplexity_ratio(word, best_word);
        if perp_ratio > 1.4 && !self.is_known_misspelling(word) {
            // Input is a tech-like word being corrected to a natural English word
            return None;
        }

        // Apply a modest frequency boost for unusual-looking inputs
        let freq_boost = if perp_ratio > 1.5 { 5.0 } else { 1.0 };

        // Only auto-correct if confidence is reasonable:
        if best.2 == 1 {
            if best.1 >= MIN_FREQ_THRESHOLD * freq_boost {
                return Some(best_word.clone());
            }
        } else if best.2 == 2 {
            if best.1 >= MIN_FREQ_THRESHOLD * 10.0 * freq_boost {
                return Some(best_word.clone());
            }
        }

        None
    }

    /// LinSpell fallback: linear scan with early termination.
    /// Only checks words with similar length (±2 chars) and computes
    /// Damerau-Levenshtein distance with early cutoff.
    fn linspell_lookup(&self, word: &str) -> Option<String> {
        let word_len = word.len();
        let mut best_candidate: Option<(String, f64, usize)> = None; // (word, freq, edit_dist)

        for (word_id, dict_word) in self.words.iter().enumerate() {
            let dict_len = dict_word.len();
            let len_diff = if word_len > dict_len {
                word_len - dict_len
            } else {
                dict_len - word_len
            };

            // Length difference > 2 → not a likely correction
            if len_diff > 2 {
                continue;
            }

            let dist = self.compute_edit_distance(word, dict_word);
            if dist > 2 {
                continue; // Too far
            }

            // Fix: if it was an exact match (dist == 0), we already caught it in exact_map
            if dist == 0 {
                continue;
            }

            let freq = self.frequencies[word_id];

            match &best_candidate {
                None => {
                    best_candidate = Some((dict_word.clone(), freq, dist));
                }
                Some((_, best_freq, best_dist)) => {
                    // Prefer lower edit distance. Allow a 5x-frequency override
                    // only when a misspelling entry (freq < 0.01) at the closer
                    // distance would block a real word (freq >= 0.01) at +1 edit.
                    //
                    // Since the dictionary is sorted by frequency descending,
                    // the higher-frequency real word is encountered FIRST.
                    // When a misspelling entry arrives later at lower distance,
                    // we must prevent it from replacing the real word.
                    let new_is_real = freq >= MIN_FREQ_THRESHOLD * 10.0;
                    let new_is_misspelling = freq < MIN_FREQ_THRESHOLD * 10.0;
                    let best_is_real = *best_freq >= MIN_FREQ_THRESHOLD * 10.0;
                    let best_is_misspelling = *best_freq < MIN_FREQ_THRESHOLD * 10.0;

                    let replace = if dist < *best_dist {
                        // New word is closer. Replace unless it's a misspelling
                        // entry (+1 edit away) and the current best is a real
                        // word that's 5x more common.
                        if new_is_misspelling && best_is_real
                            && dist + 1 == *best_dist && *best_freq >= freq * 5.0
                        {
                            false
                        } else {
                            true
                        }
                    } else if dist > *best_dist {
                        // New word is farther. Accept only if it's a real word
                        // (+1 edit away, 5x more common) and current best is
                        // a misspelling entry.
                        if new_is_real && best_is_misspelling
                            && dist == *best_dist + 1 && freq >= *best_freq * 5.0
                        {
                            true
                        } else {
                            false
                        }
                    } else {
                        freq > *best_freq
                    };
                    if replace {
                        best_candidate = Some((dict_word.clone(), freq, dist));
                    }
                }
            }
        }

        best_candidate.and_then(|(candidate_word, freq, dist)| {
            // Apply perplexity ratio guard
            // Skip for known misspellings (explicitly added at freq < 0.01)
            let perp_ratio = self.char_bigram_model.perplexity_ratio(word, &candidate_word);
            if perp_ratio > 1.4 && !self.is_known_misspelling(word) {
                return None; // Input is tech-like, candidate is natural English → reject
            }
            let freq_boost = if perp_ratio > 1.5 { 5.0 } else { 1.0 };

            if dist == 1 {
                if freq >= MIN_FREQ_THRESHOLD * freq_boost {
                    Some(candidate_word)
                } else {
                    None
                }
            } else if dist == 2 {
                if freq >= MIN_FREQ_THRESHOLD * 10.0 * freq_boost {
                    Some(candidate_word)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Compute Damerau-Levenshtein edit distance between two strings.
    /// Includes transposition of adjacent characters as a single operation
    /// (cost 1 instead of 2 in standard Levenshtein). This is critical for
    /// spelling correction where transpositions like "recieve"→"receive"
    /// or "orcale"→"oracle" should be distance 1.
    fn compute_edit_distance(&self, a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let a_len = a_chars.len();
        let b_len = b_chars.len();

        // Early termination by length difference
        if a_len.abs_diff(b_len) > 2 {
            return a_len.abs_diff(b_len).max(MAX_EDIT_DISTANCE + 1);
        }

        // Damerau-Levenshtein: like standard Levenshtein but with
        // transposition of adjacent characters as a single operation.
        // Uses two preceding rows to check for transpositions.
        let mut prev_prev_row: Vec<usize> = vec![0; b_len + 1];
        let mut prev_row: Vec<usize> = (0..=b_len).collect();
        let mut curr_row: Vec<usize> = vec![0; b_len + 1];

        for i in 1..=a_len {
            curr_row[0] = i;
            let a_char = a_chars[i - 1];

            for j in 1..=b_len {
                let cost = if a_char == b_chars[j - 1] { 0 } else { 1 };
                let mut dist = (curr_row[j - 1] + 1) // insert
                    .min(prev_row[j] + 1)           // delete
                    .min(prev_row[j - 1] + cost);    // substitute

                // Transposition: if adjacent chars are swapped
                if i > 1 && j > 1
                    && a_char == b_chars[j - 2]
                    && a_chars[i - 2] == b_chars[j - 1]
                {
                    dist = dist.min(prev_prev_row[j - 2] + cost);
                }

                curr_row[j] = dist;
            }

            // Early cutoff
            if b_len > 0 {
                let min_in_row = curr_row[1..].iter().min().copied().unwrap_or(curr_row[b_len]);
                if min_in_row > MAX_EDIT_DISTANCE && i < a_len - 1 {
                    return MAX_EDIT_DISTANCE + 1;
                }
            }

            std::mem::swap(&mut prev_prev_row, &mut prev_row);
            std::mem::swap(&mut prev_row, &mut curr_row);
        }

        prev_row[b_len]
    }
}

// ─── Character Bigram Model for Tech-Term Detection ─────────────────
// Builds a character-level bigram probability model from the dictionary.
// Words with rare/unusual character bigrams (e.g., "dm" in "podman") get
// a high perplexity score. These are likely tech terms rather than
// misspellings, so we require stronger frequency evidence before correcting.
//
// This is a purely data-driven approach — no hardcoded whitelists needed.
// The model is computed entirely from the dictionary at build time.
struct CharBigramModel {
    /// Log probabilities of each character bigram (add-k smoothed)
    bigram_log_probs: HashMap<(char, char), f64>,
    /// Default log probability for unseen bigrams
    default_log_prob: f64,
    /// Reference perplexity for logging
    reference_perplexity: f64,
}

impl CharBigramModel {
    /// Build character bigram model from dictionary words.
    fn build(words: &[String]) -> Self {
        let mut counts: HashMap<(char, char), u64> = HashMap::new();
        let mut total_bigrams = 0u64;

        // Count all character bigrams in the dictionary
        for word in words {
            let chars: Vec<char> = word.chars().collect();
            for i in 0..chars.len().saturating_sub(1) {
                *counts.entry((chars[i], chars[i + 1])).or_default() += 1;
                total_bigrams += 1;
            }
        }

        // Add-k smoothing to handle unseen bigrams
        let vocab_size = counts.len() as f64;
        let smoothing_k = 0.5;
        let denom = total_bigrams as f64 + smoothing_k * vocab_size;

        let mut bigram_log_probs = HashMap::with_capacity(counts.len());
        for (bigram, cnt) in &counts {
            let prob = (*cnt as f64 + smoothing_k) / denom;
            bigram_log_probs.insert(*bigram, prob.ln());
        }
        let default_log_prob = (smoothing_k / denom).ln();

        // Compute perplexity for all dictionary words ≥ 4 chars to establish
        // a baseline. The 95th percentile is the threshold for "unusual".
        let mut perplexities: Vec<f64> = Vec::new();
        for word in words {
            if word.len() < 4 {
                continue;
            }
            let chars: Vec<char> = word.chars().collect();
            let mut total_lp = 0.0;
            for i in 0..chars.len() - 1 {
                total_lp += bigram_log_probs
                    .get(&(chars[i], chars[i + 1]))
                    .copied()
                    .unwrap_or(default_log_prob);
            }
            let perplexity = (-total_lp / (chars.len() - 1) as f64).exp();
            perplexities.push(perplexity);
        }

        perplexities.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // Store the median perplexity as a reference value (for logging)
        let median_idx = (perplexities.len() as f64 * 0.50) as usize;
        let median = if median_idx < perplexities.len() {
            perplexities[median_idx]
        } else {
            perplexities.last().copied().unwrap_or(10.0)
        };

        Self {
            bigram_log_probs,
            default_log_prob,
            reference_perplexity: median,
        }
    }

    /// Compute perplexity of a word against the character bigram model.
    /// Lower perplexity = more natural English. Higher = unusual/tech-like.
    fn perplexity(&self, word: &str) -> f64 {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() < 2 {
            return 0.0;
        }
        let mut total_lp = 0.0;
        for i in 0..chars.len() - 1 {
            total_lp += self
                .bigram_log_probs
                .get(&(chars[i], chars[i + 1]))
                .copied()
                .unwrap_or(self.default_log_prob);
        }
        (-total_lp / (chars.len() - 1) as f64).exp()
    }

    /// Returns the ratio of perplexity of the input word vs a candidate.
    /// If this ratio is high (> 3.0), the input is much more "unusual" than
    /// the candidate, suggesting the correction is English-ifying a tech term.
    /// If the ratio is ~1.0, both words have similar bigram character patterns,
    /// which is expected for genuine misspellings.
    fn perplexity_ratio(&self, input_word: &str, candidate_word: &str) -> f64 {
        let input_perp = self.perplexity(input_word);
        let candidate_perp = self.perplexity(candidate_word);
        if input_perp <= 0.0 || candidate_perp <= 0.0 {
            return 1.0; // Default to neutral
        }
        input_perp / candidate_perp
    }
}

/// Apply spelling correction to a full query string.
/// Corrects each word independently and returns the corrected query,
/// along with a flag indicating whether any correction was applied.
pub(crate) fn correct_query(index: &SymSpellIndex, query: &str) -> (String, bool) {
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut corrected_words: Vec<String> = Vec::with_capacity(words.len());
    let mut any_corrected = false;

    for word in &words {
        // Don't correct URLs, code terms, or words with numbers/special chars
        if word.contains('.') || word.contains('/') || word.contains('\\')
            || word.contains('@') || word.contains('#') || word.contains('$')
            || word.chars().any(|c| c.is_numeric())
        {
            corrected_words.push(word.to_string());
            continue;
        }

        // Don't correct very short words or empty
        if word.len() < MIN_CORRECT_LENGTH {
            corrected_words.push(word.to_string());
            continue;
        }

        match index.correct(word) {
            Some(corrected) => {
                corrected_words.push(corrected);
                any_corrected = true;
            }
            None => {
                corrected_words.push(word.to_string());
            }
        }
    }

    (corrected_words.join(" "), any_corrected)
}

/// Validate a spelling correction against actual search result signals.
///
/// After the corrected query fetches results, check whether the original
/// (uncorrected) words appear more frequently in result titles/URLs than
/// the corrected words. If the original words consistently appear more,
/// the correction is likely wrong — the original was actually valid and
/// the dictionary-based corrector misfired. This provides a web-data-driven
/// safety net on top of the pure dictionary-based SymSpell correction.
///
/// Returns `true` if the correction is valid (keep it), `false` if it
/// should be reverted (the original query was better).
pub(crate) fn validate_correction(
    original: &str,
    corrected: &str,
    titles: &[String],
    urls: &[String],
) -> bool {
    if titles.len() < 5 {
        return true;
    }

    let orig_words: Vec<&str> = original.split_whitespace().collect();
    let corr_words: Vec<&str> = corrected.split_whitespace().collect();

    if orig_words.len() != corr_words.len() {
        return true;
    }

    let changed: Vec<(&str, &str)> = orig_words.iter()
        .zip(corr_words.iter())
        .filter(|(o, c)| o != c)
        .map(|(o, c)| (*o, *c))
        .collect();

    if changed.is_empty() {
        return true;
    }

    let n = titles.len().min(20);
    let mut revert_count = 0usize;
    let mut keep_count = 0usize;

    for (orig_word, corr_word) in &changed {
        let mut orig_hits = 0usize;
        let mut corr_hits = 0usize;

        for i in 0..n {
            let title_lower = titles[i].to_lowercase();
            let url_lower = urls[i].to_lowercase();

            if word_present(orig_word, &title_lower) || word_present(orig_word, &url_lower) {
                orig_hits += 1;
            }
            if word_present(corr_word, &title_lower) || word_present(corr_word, &url_lower) {
                corr_hits += 1;
            }
        }

        if orig_hits >= corr_hits && orig_hits > 0 {
            revert_count += 1;
        } else if corr_hits > orig_hits {
            keep_count += 1;
        }
    }

    revert_count < keep_count
}

/// Check if a word appears as a distinct token in lowercased text.
fn word_present(word: &str, text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|t| t == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symspell_build() {
        let index = SymSpellIndex::build();
        assert!(index.words.len() > 100, "Dictionary too small");
        assert!(index.deletions.len() > 100, "Deletion index too small");
    }

    #[test]
    fn test_exact_match() {
        let index = SymSpellIndex::build();
        assert_eq!(index.correct("rust"), None); // Already correct
        assert_eq!(index.correct("python"), None);
        assert_eq!(index.correct("docker"), None);
    }

    #[test]
    fn test_distance_1_correction() {
        let index = SymSpellIndex::build();
        // "pthon" → "python" (one insertion needed)
        let result = index.correct("pthon");
        assert!(result.is_some(), "Should correct 'pthon'");
        assert_eq!(result.unwrap(), "python");
    }

    #[test]
    fn test_short_word_unchanged() {
        let index = SymSpellIndex::build();
        assert_eq!(index.correct("go"), None); // Too short, don't correct
        assert_eq!(index.correct("js"), None);
    }

    #[test]
    fn test_correct_query() {
        let index = SymSpellIndex::build();
        let (corrected, changed) = correct_query(&index, "rust programing");
        assert!(changed, "Should have corrected something");
        assert_eq!(corrected, "rust programming");
    }

    #[test]
    fn test_no_correction_needed() {
        let index = SymSpellIndex::build();
        let (corrected, changed) = correct_query(&index, "python web framework");
        assert!(!changed, "Should not change correct query");
        assert_eq!(corrected, "python web framework");
    }

    #[test]
    fn test_url_unchanged() {
        let index = SymSpellIndex::build();
        let (corrected, changed) = correct_query(&index, "github.com");
        assert!(!changed, "URLs should not be corrected");
        assert_eq!(corrected, "github.com");
    }

    #[test]
    fn test_tech_term_not_false_positive() {
        // Tech terms with unusual bigrams (like "podman" with "dm") should NOT
        // be corrected to common English words (like "woman").
        let index = SymSpellIndex::build();
        let result = index.correct("podman");
        assert_eq!(result, None,
            "Should not correct tech term 'podman' to common English word");

        // Common misspellings should STILL be corrected
        // "ngnix" → "nginx" (both have similar unusual bigrams)
        let result = index.correct("ngnix");
        assert_eq!(result, Some("nginx".to_string()),
            "Should correct misspelling 'ngnix' (similar bigram patterns)");
    }

    #[test]
    fn test_three_letter_words_not_corrected() {
        let index = SymSpellIndex::build();
        // MIN_CORRECT_LENGTH is now 4, so 3-letter words are never corrected
        assert_eq!(index.correct("doc"), None, "Should not correct 3-letter 'doc'");
        assert_eq!(index.correct("app"), None, "Should not correct 3-letter 'app'");
    }

    #[test]
    fn test_perplexity_ratio_blocks_tech_term_fp() {
        let index = SymSpellIndex::build();

        // "podman" → "woman": podman has unusual bigrams, woman is natural English.
        // With the 15k-word dictionary, the ratio is ~1.56 — still > 1.4 threshold.
        let ratio = index.char_bigram_model.perplexity_ratio("podman", "woman");
        assert!(ratio > 1.4,
            "podman→woman perplexity ratio should be > 1.4, got {:.2}", ratio);

        // "ngnix" → "nginx": both have similar unusual bigrams (gn, nx, ix)
        // Ratio should be ~1.0, allowing the correction
        let ratio = index.char_bigram_model.perplexity_ratio("ngnix", "nginx");
        assert!(ratio < 1.4,
            "ngnix→nginx perplexity ratio should be < 1.4, got {:.2}", ratio);

        // "programing" → "programming": very similar bigrams
        // Ratio should be ~1.0
        let ratio = index.char_bigram_model.perplexity_ratio("programing", "programming");
        assert!(ratio < 1.4,
            "programing→programming ratio should be < 1.4, got {:.2}", ratio);

        // Common words should have ratio ~1.0 (same word)
        let ratio = index.char_bigram_model.perplexity_ratio("python", "python");
        assert!((ratio - 1.0).abs() < 0.01,
            "Same word should have ratio ~1.0, got {:.2}", ratio);
    }

    #[test]
    fn test_embarrass_not_falsely_corrected() {
        let index = SymSpellIndex::build();
        // "embarrass" (correct spelling) should NOT be corrected
        let result = index.correct("embarrass");
        assert_eq!(result, None,
            "Should not correct correctly-spelled 'embarrass' to misspelling");
    }

    #[test]
    fn test_embaras_corrects_to_embarrass() {
        let index = SymSpellIndex::build();
        // "embaras" (one r, one s) should correct to "embarrass" (not "embarass")
        let result = index.correct("embaras");
        assert!(result.is_some(), "Should correct 'embaras'");
        assert_eq!(result.unwrap(), "embarrass",
            "Should correct 'embaras' to 'embarrass', not 'embarass'");
    }

    #[test]
    fn test_embarass_corrects_to_embarrass() {
        let index = SymSpellIndex::build();
        // "embarass" (one r, two s - common misspelling) should correct to "embarrass"
        let result = index.correct("embarass");
        assert!(result.is_some(), "Should correct 'embarass'");
        assert_eq!(result.unwrap(), "embarrass",
            "Should correct misspelling 'embarass' to 'embarrass'");
    }

    // ─── Result-based validation tests ─────────────────────────────

    #[test]
    fn test_word_present_exact() {
        assert!(word_present("rust", "learn rust programming"));
        assert!(word_present("rust", "rust programming"));
        assert!(!word_present("rust", "rustic")); // not a substring match
    }

    #[test]
    fn test_word_present_not_found() {
        assert!(!word_present("rust", "rusty tools"));
        assert!(!word_present("go", "going to the store"));
    }

    #[test]
    fn test_word_present_special_chars() {
        assert!(word_present("rust", "rust/programming"));
        assert!(word_present("rust", "rust-programming"));
        assert!(word_present("rust", "rust.programming"));
        assert!(word_present("rust", "rust.programming"));
    }

    #[test]
    fn test_validate_correction_keep_good_correction() {
        // If corrected word appears in many titles but original doesn't, keep
        let titles = vec![
            "learn rust programming".to_string(),
            "rust programming guide".to_string(),
            "programming in rust".to_string(),
            "advanced rust programming".to_string(),
            "rust programming tutorial".to_string(),
        ];
        let urls = vec![
            "https://example.com/programming".to_string();
            5
        ];
        // "programing" → "programming": corrected word appears in all titles
        assert!(validate_correction("programing", "programming", &titles, &urls));
    }

    #[test]
    fn test_validate_correction_revert_false_correction() {
        // If original word appears in many titles but corrected doesn't, revert
        let titles = vec![
            "how to embarrass yourself".to_string(),
            "embarrass public speaking".to_string(),
            "don't embarrass me".to_string(),
            "embarrassing moments".to_string(),
            "embarrass story".to_string(),
        ];
        let urls = vec![
            "https://example.com/story".to_string();
            5
        ];
        // "embarrass" → "embarass": original word appears in 4/5 titles
        assert!(!validate_correction("embarrass", "embarass", &titles, &urls));
    }

    #[test]
    fn test_validate_correction_too_few_results() {
        // Fewer than 5 results → trust dictionary (return true)
        let titles = vec!["hello".to_string(); 3];
        let urls = vec!["https://example.com".to_string(); 3];
        assert!(validate_correction("programing", "programming", &titles, &urls));
    }

    #[test]
    fn test_validate_correction_no_changed_words() {
        // No actual change → trust dictionary
        let titles = vec!["hello world".to_string(); 10];
        let urls = vec!["https://example.com".to_string(); 10];
        assert!(validate_correction("hello world", "hello world", &titles, &urls));
    }

    #[test]
    fn test_validate_correction_tie_goes_to_correction() {
        // Equal hits → keep correction (perplexity guard already handled tech terms)
        let titles = vec![
            "python versus rust".to_string(),
            "rust or python".to_string(),
            "comparing python and rust".to_string(),
            "rust python comparison".to_string(),
            "python vs rust".to_string(),
        ];
        let urls = vec!["https://example.com".to_string(); 5];
        // Both "python" and "pyton" would have similar hits → keep correction
        assert!(validate_correction("pyton", "python", &titles, &urls));
    }
}
