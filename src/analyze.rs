use std::collections::HashMap;

use crate::models::Post;

/// Word count statistics for a set of texts.
#[derive(Debug)]
pub struct WordCountStats {
    pub avg: f64,
    pub median: f64,
    pub min: usize,
    pub max: usize,
    pub count: usize,
}

/// Sentence-level statistics.
#[derive(Debug)]
pub struct SentenceStats {
    pub avg_sentence_word_count: f64,
    pub avg_sentences_per_post: f64,
}

/// Punctuation usage entry.
#[derive(Debug)]
pub struct PunctuationEntry {
    pub char: char,
    pub count: usize,
    pub posts_with: usize,
}

/// An ngram with its frequency.
#[derive(Debug)]
pub struct Ngram {
    pub words: String,
    pub count: usize,
}

/// Full voice profile analysis result.
#[derive(Debug)]
pub struct VoiceProfile {
    pub total_posts: usize,
    pub word_count: WordCountStats,
    pub sentence: SentenceStats,
    pub lowercase_start_rate: f64,
    pub contraction_rate: f64,
    pub question_rate: f64,
    pub punctuation: Vec<PunctuationEntry>,
    pub bigrams: Vec<Ngram>,
    pub trigrams: Vec<Ngram>,
}

/// Per-type summary stats.
#[derive(Debug)]
pub struct TypeSummary {
    pub post_type: String,
    pub count: usize,
    pub word_count: WordCountStats,
    pub lowercase_start_rate: f64,
    pub question_rate: f64,
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn word_counts(bodies: &[&str]) -> Vec<usize> {
    bodies.iter().map(|b| word_count(b)).collect()
}

pub fn word_count_stats(bodies: &[&str]) -> WordCountStats {
    if bodies.is_empty() {
        return WordCountStats {
            avg: 0.0,
            median: 0.0,
            min: 0,
            max: 0,
            count: 0,
        };
    }

    let mut counts = word_counts(bodies);
    counts.sort();

    let sum: usize = counts.iter().sum();
    let len = counts.len();

    let median = if len % 2 == 0 {
        (counts[len / 2 - 1] + counts[len / 2]) as f64 / 2.0
    } else {
        counts[len / 2] as f64
    };

    WordCountStats {
        avg: sum as f64 / len as f64,
        median,
        min: counts[0],
        max: counts[len - 1],
        count: len,
    }
}

/// Split text into sentences (simple heuristic: split on `. `, `! `, `? `, or end-of-string after `.!?`).
fn sentences(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        let is_terminal = bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?';
        if is_terminal {
            // Check if next char is whitespace or end of string
            let at_end = i + 1 >= len;
            let next_is_space = !at_end && (bytes[i + 1] == b' ' || bytes[i + 1] == b'\n');
            if at_end || next_is_space {
                let sentence = text[start..=i].trim();
                if !sentence.is_empty() {
                    result.push(sentence);
                }
                start = i + 1;
            }
        }
    }

    // Remaining text that didn't end with punctuation
    if start < len {
        let remaining = text[start..].trim();
        if !remaining.is_empty() {
            result.push(remaining);
        }
    }

    result
}

pub fn sentence_stats(bodies: &[&str]) -> SentenceStats {
    if bodies.is_empty() {
        return SentenceStats {
            avg_sentence_word_count: 0.0,
            avg_sentences_per_post: 0.0,
        };
    }

    let mut total_sentences = 0usize;
    let mut total_sentence_words = 0usize;

    for body in bodies {
        let sents = sentences(body);
        total_sentences += sents.len();
        for s in &sents {
            total_sentence_words += word_count(s);
        }
    }

    let avg_sentence_word_count = if total_sentences > 0 {
        total_sentence_words as f64 / total_sentences as f64
    } else {
        0.0
    };

    let avg_sentences_per_post = total_sentences as f64 / bodies.len() as f64;

    SentenceStats {
        avg_sentence_word_count,
        avg_sentences_per_post,
    }
}

/// Fraction of posts that start with a lowercase letter.
pub fn lowercase_start_rate(bodies: &[&str]) -> f64 {
    if bodies.is_empty() {
        return 0.0;
    }

    let count = bodies
        .iter()
        .filter(|b| {
            b.trim()
                .chars()
                .next()
                .map(|c| c.is_ascii_lowercase())
                .unwrap_or(false)
        })
        .count();

    count as f64 / bodies.len() as f64
}

/// Fraction of posts containing common contractions.
pub fn contraction_rate(bodies: &[&str]) -> f64 {
    if bodies.is_empty() {
        return 0.0;
    }

    let contractions = [
        "don't",
        "doesn't",
        "didn't",
        "can't",
        "won't",
        "wouldn't",
        "shouldn't",
        "couldn't",
        "isn't",
        "aren't",
        "wasn't",
        "weren't",
        "haven't",
        "hasn't",
        "hadn't",
        "it's",
        "that's",
        "there's",
        "here's",
        "what's",
        "who's",
        "let's",
        "I'm",
        "I've",
        "I'd",
        "I'll",
        "you're",
        "you've",
        "you'd",
        "you'll",
        "we're",
        "we've",
        "we'd",
        "we'll",
        "they're",
        "they've",
        "they'd",
        "they'll",
        "he's",
        "she's",
    ];

    let count = bodies
        .iter()
        .filter(|b| {
            let lower = b.to_lowercase();
            contractions.iter().any(|c| lower.contains(c))
        })
        .count();

    count as f64 / bodies.len() as f64
}

/// Fraction of posts containing a question mark.
pub fn question_rate(bodies: &[&str]) -> f64 {
    if bodies.is_empty() {
        return 0.0;
    }

    let count = bodies.iter().filter(|b| b.contains('?')).count();
    count as f64 / bodies.len() as f64
}

/// Inventory of punctuation characters used across all posts.
pub fn punctuation_inventory(bodies: &[&str]) -> Vec<PunctuationEntry> {
    let mut char_counts: HashMap<char, usize> = HashMap::new();
    let mut char_post_counts: HashMap<char, usize> = HashMap::new();

    let interesting_punct: &[char] = &[
        '.', ',', '!', '?', ':', ';', '-', '—', '–', '\'', '"', '(', ')', '[', ']', '{', '}', '/',
        '\\', '@', '#', '`', '~', '*', '&', '+', '=', '<', '>',
    ];

    for body in bodies {
        let mut seen_in_post: HashMap<char, bool> = HashMap::new();
        for ch in body.chars() {
            if interesting_punct.contains(&ch) {
                *char_counts.entry(ch).or_insert(0) += 1;
                if !seen_in_post.contains_key(&ch) {
                    *char_post_counts.entry(ch).or_insert(0) += 1;
                    seen_in_post.insert(ch, true);
                }
            }
        }
    }

    let mut entries: Vec<PunctuationEntry> = char_counts
        .into_iter()
        .map(|(ch, count)| PunctuationEntry {
            char: ch,
            count,
            posts_with: *char_post_counts.get(&ch).unwrap_or(&0),
        })
        .collect();

    entries.sort_by(|a, b| b.count.cmp(&a.count));
    entries
}

/// Extract the top-k ngrams of size `n` from the bodies.
pub fn common_ngrams(bodies: &[&str], n: usize, top_k: usize) -> Vec<Ngram> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for body in bodies {
        let words: Vec<&str> = body
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation()))
            .filter(|w| !w.is_empty())
            .collect();

        if words.len() < n {
            continue;
        }

        for window in words.windows(n) {
            let ngram = window.join(" ").to_lowercase();
            // Skip ngrams that are just stopwords
            if n <= 2 && is_all_stopwords(window) {
                continue;
            }
            *counts.entry(ngram).or_insert(0) += 1;
        }
    }

    let mut ngrams: Vec<Ngram> = counts
        .into_iter()
        .filter(|(_, count)| *count >= 2) // Only include ngrams that appear at least twice
        .map(|(words, count)| Ngram { words, count })
        .collect();

    ngrams.sort_by(|a, b| b.count.cmp(&a.count));
    ngrams.truncate(top_k);
    ngrams
}

fn is_all_stopwords(words: &[&str]) -> bool {
    const STOPWORDS: &[&str] = &[
        "a", "an", "the", "is", "it", "in", "on", "to", "of", "for", "and", "or", "but", "not",
        "be", "this", "that", "with", "as", "at", "by", "from", "if", "so", "no", "do", "i", "you",
        "we", "he", "she", "they", "my", "your", "our", "his", "her", "its", "their", "was",
        "were", "are", "am", "been", "has", "have", "had", "will", "would", "could", "should",
        "can", "may", "might",
    ];

    words
        .iter()
        .all(|w| STOPWORDS.contains(&w.to_lowercase().as_str()))
}

/// Build a full voice profile from a set of post bodies.
pub fn build_profile(bodies: &[&str]) -> VoiceProfile {
    VoiceProfile {
        total_posts: bodies.len(),
        word_count: word_count_stats(bodies),
        sentence: sentence_stats(bodies),
        lowercase_start_rate: lowercase_start_rate(bodies),
        contraction_rate: contraction_rate(bodies),
        question_rate: question_rate(bodies),
        punctuation: punctuation_inventory(bodies),
        bigrams: common_ngrams(bodies, 2, 15),
        trigrams: common_ngrams(bodies, 3, 10),
    }
}

/// Build per-type summaries.
pub fn type_summaries(posts: &[Post]) -> Vec<TypeSummary> {
    let mut by_type: HashMap<String, Vec<&str>> = HashMap::new();

    for post in posts {
        by_type
            .entry(post.post_type.clone())
            .or_default()
            .push(&post.body);
    }

    let mut summaries: Vec<TypeSummary> = by_type
        .into_iter()
        .map(|(post_type, bodies)| {
            let bodies_ref: Vec<&str> = bodies.into_iter().collect();
            TypeSummary {
                post_type,
                count: bodies_ref.len(),
                word_count: word_count_stats(&bodies_ref),
                lowercase_start_rate: lowercase_start_rate(&bodies_ref),
                question_rate: question_rate(&bodies_ref),
            }
        })
        .collect();

    summaries.sort_by(|a, b| b.count.cmp(&a.count));
    summaries
}

/// Select diverse examples from posts, spread across repos/dates/lengths.
/// Deduplicates by post id to avoid picking the same post twice.
pub fn select_diverse_examples(posts: &[Post], limit: usize) -> Vec<&Post> {
    if posts.len() <= limit {
        // Deduplicate even in the small case
        let mut seen = std::collections::HashSet::new();
        return posts.iter().filter(|p| seen.insert(p.id)).collect();
    }

    // Bucket by repo, then pick round-robin from buckets, preferring variety in length
    let mut by_repo: HashMap<Option<&str>, Vec<&Post>> = HashMap::new();
    for post in posts {
        by_repo.entry(post.repo.as_deref()).or_default().push(post);
    }

    // Sort each bucket by word count so we get length variety when picking
    for bucket in by_repo.values_mut() {
        bucket.sort_by_key(|p| word_count(&p.body));
    }

    let mut buckets: Vec<Vec<&Post>> = by_repo.into_values().collect();
    // Sort buckets by size descending so we start with the largest
    buckets.sort_by(|a, b| b.len().cmp(&a.len()));

    let mut selected: Vec<&Post> = Vec::with_capacity(limit);
    let mut seen_ids = std::collections::HashSet::new();
    let mut indices: Vec<usize> = vec![0; buckets.len()];

    // Round-robin, picking from alternating ends of each bucket for length variety
    let mut from_start = true;
    while selected.len() < limit {
        let mut picked_any = false;
        for (i, bucket) in buckets.iter().enumerate() {
            if selected.len() >= limit {
                break;
            }
            if indices[i] >= bucket.len() {
                continue;
            }

            let idx = if from_start {
                indices[i]
            } else {
                bucket.len() - 1 - indices[i]
            };

            // Bounds check (when bucket is small and indices[i] wraps)
            if idx < bucket.len() {
                let post = bucket[idx];
                indices[i] += 1;
                if seen_ids.insert(post.id) {
                    selected.push(post);
                    picked_any = true;
                }
            }
        }

        if !picked_any {
            break;
        }
        from_start = !from_start;
    }

    // Bucket by repo, then pick round-robin from buckets, preferring variety in length
    let mut by_repo: HashMap<Option<&str>, Vec<&Post>> = HashMap::new();
    for post in posts {
        by_repo.entry(post.repo.as_deref()).or_default().push(post);
    }

    // Sort each bucket by word count so we get length variety when picking
    for bucket in by_repo.values_mut() {
        bucket.sort_by_key(|p| word_count(&p.body));
    }

    let mut buckets: Vec<Vec<&Post>> = by_repo.into_values().collect();
    // Sort buckets by size descending so we start with the largest
    buckets.sort_by(|a, b| b.len().cmp(&a.len()));

    let mut selected: Vec<&Post> = Vec::with_capacity(limit);
    let mut indices: Vec<usize> = vec![0; buckets.len()];

    // Round-robin, picking from alternating ends of each bucket for length variety
    let mut from_start = true;
    while selected.len() < limit {
        let mut picked_any = false;
        for (i, bucket) in buckets.iter().enumerate() {
            if selected.len() >= limit {
                break;
            }
            if indices[i] >= bucket.len() {
                continue;
            }

            let idx = if from_start {
                indices[i]
            } else {
                bucket.len() - 1 - indices[i]
            };

            // Bounds check (when bucket is small and indices[i] wraps)
            if idx < bucket.len() {
                selected.push(bucket[idx]);
                indices[i] += 1;
                picked_any = true;
            }
        }

        if !picked_any {
            break;
        }
        from_start = !from_start;
    }

    // Sort final selection by date for readability
    selected.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_count_stats_basic() {
        let bodies = ["hello world", "one two three four", "hi"];
        let stats = word_count_stats(&bodies);
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, 1);
        assert_eq!(stats.max, 4);
        assert!((stats.median - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_word_count_stats_empty() {
        let bodies: [&str; 0] = [];
        let stats = word_count_stats(&bodies);
        assert_eq!(stats.count, 0);
    }

    #[test]
    fn test_lowercase_start_rate() {
        let bodies = ["hello", "World", "test", "Another"];
        let rate = lowercase_start_rate(&bodies);
        assert!((rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_contraction_rate() {
        let bodies = ["I don't know", "this is fine", "can't stop"];
        let rate = contraction_rate(&bodies);
        assert!((rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_question_rate() {
        let bodies = ["what?", "ok", "really?", "yep"];
        let rate = question_rate(&bodies);
        assert!((rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sentences() {
        let text = "hello world. this is a test. done";
        let sents = sentences(text);
        assert_eq!(sents.len(), 3);
        assert_eq!(sents[0], "hello world.");
        assert_eq!(sents[1], "this is a test.");
        assert_eq!(sents[2], "done");
    }

    #[test]
    fn test_common_ngrams() {
        let bodies = [
            "the quick brown fox",
            "the quick red fox",
            "the slow brown fox",
        ];
        let bigrams = common_ngrams(&bodies, 2, 5);
        // "brown fox" and "quick" phrases should appear
        assert!(!bigrams.is_empty());
    }
}
