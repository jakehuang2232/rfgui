use super::*;

#[test]
fn empty() {
    let s = UnicodeSegmenter::new();
    assert_eq!(s.word_boundaries_char_indices(""), vec![0]);
}

#[test]
fn ascii_words() {
    let s = UnicodeSegmenter::new();
    // "foo bar" -> "foo" | " " | "bar" => boundaries 0,3,4,7
    assert_eq!(s.word_boundaries_char_indices("foo bar"), vec![0, 3, 4, 7]);
}

#[test]
fn ascii_punctuation() {
    let s = UnicodeSegmenter::new();
    // "a,b" -> "a" | "," | "b" => 0,1,2,3
    assert_eq!(s.word_boundaries_char_indices("a,b"), vec![0, 1, 2, 3]);
}

#[test]
fn cjk_per_char_known_limitation() {
    let s = UnicodeSegmenter::new();
    // UAX#29 without dict: each CJK char its own boundary.
    // "今天" -> 0,1,2
    let bs = s.word_boundaries_char_indices("今天");
    assert_eq!(bs.first(), Some(&0));
    assert_eq!(bs.last(), Some(&2));
}

#[test]
fn line_boundaries_are_byte_and_char_addressable() {
    let s = UnicodeSegmenter::new();
    assert_eq!(
        s.line_boundaries_byte_indices("Hello world!"),
        vec![0, 6, 12]
    );
    assert_eq!(
        s.line_boundaries_char_indices("Hello world!"),
        vec![0, 6, 12]
    );
}

#[test]
fn grapheme_keeps_emoji_cluster() {
    let s = UnicodeSegmenter::new();
    let text = "a🇹🇼b";
    assert_eq!(s.grapheme_boundaries_char_indices(text), vec![0, 1, 3, 4]);
}
