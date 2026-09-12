use super::*;

struct TestSegmenter;

impl WordSegmenter for TestSegmenter {
    fn word_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        let mut out = vec![0];
        for (idx, ch) in text.chars().enumerate() {
            if ch.is_whitespace() {
                out.push(idx);
                out.push(idx + 1);
            }
        }
        let total = text.chars().count();
        if out.last() != Some(&total) {
            out.push(total);
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[test]
fn prev_next_skip_whitespace() {
    let s = TestSegmenter;
    assert_eq!(prev_word_boundary("  foo  bar  ", &s, 11), 7);
    assert_eq!(next_word_boundary("  foo  bar  ", &s, 0), 5);
    assert_eq!(next_word_boundary("  foo  bar  ", &s, 5), 10);
    assert_eq!(prev_word_boundary("  foo  bar  ", &s, 5), 2);
}
