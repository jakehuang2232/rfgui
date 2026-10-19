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

/// The same text through a tokenizer created for this call: a new thread
/// starts without cached tokenizers.
fn fresh(text: &'static str, unit: CFOptionFlags) -> Vec<usize> {
    std::thread::spawn(move || cf_token_boundaries_byte_indices(text, unit))
        .join()
        .unwrap()
}

#[test]
fn released_tokenizer_matches_a_fresh_one() {
    const TEXTS: [&str; 4] = [
        "The quick brown fox jumps over the lazy dog.",
        "今天天氣很好，我們去公園散步吧。",
        "hello 世界 a 😀 b",
        "x",
    ];
    for unit in [
        kCFStringTokenizerUnitWordBoundary,
        kCFStringTokenizerUnitLineBreak,
    ] {
        for (previous, text) in TEXTS.iter().zip(TEXTS.iter().cycle().skip(1)) {
            cf_token_boundaries_byte_indices(previous, unit);
            release_cached_text();
            assert_eq!(
                cf_token_boundaries_byte_indices(text, unit),
                fresh(text, unit),
                "{text:?} after {previous:?}"
            );
        }
    }
}

#[test]
fn release_drops_the_tokenized_string() {
    use core_foundation_sys::base::CFGetRetainCount;
    // Long enough not to be a tagged pointer, whose retain count is fixed.
    let text = "A string long enough to be heap allocated, 足夠長的字串。".repeat(4);
    let cf_string = CFString::new(&text);
    let cf_ref = cf_string.as_concrete_TypeRef();
    let retains = || unsafe { CFGetRetainCount(cf_ref as _) };
    let before = retains();
    let map = build_utf16_to_byte_map(&text);
    cf_string_token_boundaries(cf_ref, kCFStringTokenizerUnitLineBreak, &map, text.len());
    assert!(retains() > before, "the cached tokenizer holds the text");
    release_cached_text();
    assert_eq!(retains(), before);
}
