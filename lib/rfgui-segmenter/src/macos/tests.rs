use super::*;

#[test]
fn empty() {
    let s = CfStringTokenizerSegmenter::new();
    assert_eq!(s.word_boundaries_char_indices(""), vec![0]);
}

#[test]
fn ascii() {
    let s = CfStringTokenizerSegmenter::new();
    let bs = s.word_boundaries_char_indices("foo bar");
    // Apple tokenizer: "foo" | " " | "bar" — same shape as UAX#29.
    assert_eq!(bs.first(), Some(&0));
    assert_eq!(bs.last(), Some(&7));
    assert!(bs.contains(&3));
    assert!(bs.contains(&4));
}

#[test]
fn cjk_dict() {
    let s = CfStringTokenizerSegmenter::new();
    // "今天天氣很好" should split into multi-char words via Apple's
    // CJK dict — we only assert that boundaries are non-trivial
    // (more than just per-char) when the OS dict is available.
    let bs = s.word_boundaries_char_indices("今天天氣很好");
    assert_eq!(bs.first(), Some(&0));
    assert_eq!(bs.last(), Some(&6));
    // Apple's tokenizer typically segments this into 2-3 words, not
    // 6 per-char boundaries. Allow either; assert sanity only.
    assert!(bs.len() >= 2);
    assert!(bs.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn mixed_ascii_cjk() {
    let s = CfStringTokenizerSegmenter::new();
    let bs = s.word_boundaries_char_indices("hello 世界");
    assert_eq!(bs.first(), Some(&0));
    // "hello" + " " + "世界" total 8 chars.
    assert_eq!(bs.last(), Some(&8));
    assert!(bs.contains(&5));
}

#[test]
fn emoji_surrogate_pair() {
    let s = CfStringTokenizerSegmenter::new();
    // Emoji is one char but two utf16 code units — our utf16->char
    // map must collapse the pair correctly.
    let bs = s.word_boundaries_char_indices("a 😀 b");
    assert_eq!(bs.first(), Some(&0));
    assert_eq!(bs.last(), Some(&5));
}

#[test]
fn byte_indices() {
    let s = CfStringTokenizerSegmenter::new();
    let bs = s.word_boundaries_byte_indices("a 世界");
    assert_eq!(bs.first(), Some(&0));
    assert_eq!(bs.last(), Some(&8));
}
