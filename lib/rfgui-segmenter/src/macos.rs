//! macOS — `CFStringTokenizer` with `kCFStringTokenizerUnitWordBoundary`.
//!
//! Apple's tokenizer is dictionary-backed for Chinese / Japanese / Thai /
//! Khmer / Burmese and UAX#29 elsewhere — same engine that powers the
//! system text input pipeline.
//!
//! `CFStringTokenizer` reports ranges in **UTF-16 code units**. We
//! translate back to **char indices** for the trait API.

use std::ffi::c_void;

use core_foundation::base::{CFRange, TCFType};
use core_foundation::string::{CFString, CFStringRef};

use crate::{WordSegmenter, build_utf16_to_char_map};

type CFStringTokenizerRef = *mut c_void;
type CFLocaleRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFOptionFlags = usize;
type CFStringTokenizerTokenType = CFOptionFlags;

#[allow(non_upper_case_globals)]
const kCFStringTokenizerUnitWordBoundary: CFOptionFlags = 4;
#[allow(non_upper_case_globals)]
const kCFStringTokenizerTokenNone: CFStringTokenizerTokenType = 0;

unsafe extern "C" {
    fn CFStringTokenizerCreate(
        allocator: CFAllocatorRef,
        string: CFStringRef,
        range: CFRange,
        options: CFOptionFlags,
        locale: CFLocaleRef,
    ) -> CFStringTokenizerRef;

    fn CFStringTokenizerAdvanceToNextToken(
        tokenizer: CFStringTokenizerRef,
    ) -> CFStringTokenizerTokenType;

    fn CFStringTokenizerGetCurrentTokenRange(tokenizer: CFStringTokenizerRef) -> CFRange;

    fn CFRelease(cf: *const c_void);

    fn CFStringGetLength(s: CFStringRef) -> isize;
}

pub struct CfStringTokenizerSegmenter;

impl CfStringTokenizerSegmenter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CfStringTokenizerSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl WordSegmenter for CfStringTokenizerSegmenter {
    fn boundaries(&self, text: &str) -> Vec<usize> {
        let total_chars = text.chars().count();
        if total_chars == 0 {
            return vec![0];
        }

        let cf_string = CFString::new(text);
        let cf_ref = cf_string.as_concrete_TypeRef();
        let utf16_len = unsafe { CFStringGetLength(cf_ref) };

        let utf16_to_char = build_utf16_to_char_map(text);

        let mut out = Vec::new();
        out.push(0usize);

        let tokenizer = unsafe {
            CFStringTokenizerCreate(
                std::ptr::null(),
                cf_ref,
                CFRange {
                    location: 0,
                    length: utf16_len,
                },
                kCFStringTokenizerUnitWordBoundary,
                std::ptr::null(),
            )
        };
        if tokenizer.is_null() {
            // Couldn't create — degrade to {0, total}.
            out.push(total_chars);
            return out;
        }

        loop {
            let kind = unsafe { CFStringTokenizerAdvanceToNextToken(tokenizer) };
            if kind == kCFStringTokenizerTokenNone {
                break;
            }
            let range = unsafe { CFStringTokenizerGetCurrentTokenRange(tokenizer) };
            let end_utf16 = (range.location + range.length).max(0) as usize;
            let end_char = utf16_to_char.get(end_utf16).copied().unwrap_or(total_chars);
            if Some(&end_char) != out.last() {
                out.push(end_char);
            }
        }

        unsafe { CFRelease(tokenizer as *const c_void) };

        if out.last() != Some(&total_chars) {
            out.push(total_chars);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let s = CfStringTokenizerSegmenter::new();
        assert_eq!(s.boundaries(""), vec![0]);
    }

    #[test]
    fn ascii() {
        let s = CfStringTokenizerSegmenter::new();
        let bs = s.boundaries("foo bar");
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
        let bs = s.boundaries("今天天氣很好");
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
        let bs = s.boundaries("hello 世界");
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
        let bs = s.boundaries("a 😀 b");
        assert_eq!(bs.first(), Some(&0));
        assert_eq!(bs.last(), Some(&5));
    }
}
