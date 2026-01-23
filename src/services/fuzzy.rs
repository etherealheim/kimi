//! Fuzzy string matching utilities using Levenshtein distance
//!
//! Provides fuzzy matching for the command palette and other search features.

/// Calculate the Levenshtein (edit) distance between two strings.
/// This is the minimum number of single-character edits (insertions, deletions,
/// or substitutions) required to change one string into the other.
///
/// # Arguments
/// * `source` - The source string
/// * `target` - The target string to compare against
///
/// # Returns
/// The edit distance as a usize
#[must_use]
pub fn levenshtein_distance(source: &str, target: &str) -> usize {
    let source_chars: Vec<char> = source.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();

    let source_len = source_chars.len();
    let target_len = target_chars.len();

    // Early exits for empty strings
    if source_len == 0 {
        return target_len;
    }
    if target_len == 0 {
        return source_len;
    }

    // Use two rows instead of full matrix for O(min(m,n)) space complexity
    let mut previous_row: Vec<usize> = (0..=target_len).collect();
    let mut current_row: Vec<usize> = vec![0; target_len + 1];

    for (source_idx, source_char) in source_chars.iter().enumerate() {
        // Safe: current_row always has target_len + 1 elements, index 0 is valid
        if let Some(first) = current_row.first_mut() {
            *first = source_idx + 1;
        }

        for (target_idx, target_char) in target_chars.iter().enumerate() {
            let cost = if source_char == target_char { 0 } else { 1 };

            let deletion = previous_row
                .get(target_idx + 1)
                .map_or(usize::MAX, |v| v + 1);
            let insertion = current_row
                .get(target_idx)
                .map_or(usize::MAX, |v| v + 1);
            let substitution = previous_row
                .get(target_idx)
                .map_or(usize::MAX, |v| v + cost);

            let min_cost = deletion.min(insertion).min(substitution);

            // Safe: target_idx + 1 is always <= target_len (row has target_len + 1 elements)
            if let Some(cell) = current_row.get_mut(target_idx + 1) {
                *cell = min_cost;
            }
        }

        std::mem::swap(&mut previous_row, &mut current_row);
    }

    previous_row
        .get(target_len)
        .copied()
        .unwrap_or(source_len.max(target_len))
}

/// Calculate a fuzzy match score between a query and a target string.
/// Returns a score where higher is better (more similar).
///
/// # Arguments
/// * `query` - The search query (user input)
/// * `target` - The target string to match against
///
/// # Returns
/// A score from 0.0 to 1.0 where 1.0 is a perfect match
#[must_use]
pub fn fuzzy_score(query: &str, target: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let target_lower = target.to_lowercase();

    // Perfect match
    if query_lower == target_lower {
        return 1.0;
    }

    // Exact substring match (prioritize prefix matches)
    if target_lower.starts_with(&query_lower) {
        return 0.95;
    }
    if target_lower.contains(&query_lower) {
        return 0.85;
    }

    // Check if all query characters appear in order (subsequence match)
    if is_subsequence(&query_lower, &target_lower) {
        let subsequence_bonus = query_lower.len() as f64 / target_lower.len() as f64;
        return 0.6 + (subsequence_bonus * 0.2);
    }

    // Fall back to Levenshtein distance for typo tolerance
    let distance = levenshtein_distance(&query_lower, &target_lower);
    let max_len = query_lower.len().max(target_lower.len());

    if max_len == 0 {
        return 1.0;
    }

    // Convert distance to similarity score
    let similarity = 1.0 - (distance as f64 / max_len as f64);

    // Only return positive scores for reasonable matches
    // Threshold: allow up to ~50% edit distance (handles transpositions)
    if similarity >= 0.5 {
        similarity * 0.5 // Scale down compared to exact/substring matches
    } else {
        0.0
    }
}

/// Check if `query` is a subsequence of `target`.
/// A subsequence means all characters of query appear in target in order,
/// but not necessarily consecutively.
///
/// Example: "cmd" is a subsequence of "command" (c-o-m-m-a-n-d)
fn is_subsequence(query: &str, target: &str) -> bool {
    let mut query_chars = query.chars().peekable();

    for target_char in target.chars() {
        if let Some(&query_char) = query_chars.peek() {
            if query_char == target_char {
                query_chars.next();
            }
        } else {
            break;
        }
    }

    query_chars.peek().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "hello"), 5);
        assert_eq!(levenshtein_distance("hello", ""), 5);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn test_levenshtein_single_edit() {
        assert_eq!(levenshtein_distance("hello", "hallo"), 1); // substitution
        assert_eq!(levenshtein_distance("hello", "hell"), 1); // deletion
        assert_eq!(levenshtein_distance("hello", "helloo"), 1); // insertion
    }

    #[test]
    fn test_levenshtein_multiple_edits() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("saturday", "sunday"), 3);
    }

    #[test]
    fn test_fuzzy_score_exact_match() {
        assert!((fuzzy_score("help", "help") - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuzzy_score_prefix_match() {
        let score = fuzzy_score("hel", "help");
        assert!(score > 0.9);
    }

    #[test]
    fn test_fuzzy_score_substring_match() {
        let score = fuzzy_score("elp", "help");
        assert!(score > 0.8);
    }

    #[test]
    fn test_fuzzy_score_subsequence() {
        let score = fuzzy_score("hp", "help");
        assert!(score > 0.6);
    }

    #[test]
    fn test_fuzzy_score_typo() {
        let score = fuzzy_score("hlep", "help");
        assert!(score > 0.0); // Should still match with typo
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        let score = fuzzy_score("xyz", "help");
        assert!(score < 0.1);
    }

    #[test]
    fn test_is_subsequence() {
        assert!(is_subsequence("cmd", "command"));
        assert!(is_subsequence("hp", "help"));
        assert!(!is_subsequence("xyz", "help"));
    }
}
