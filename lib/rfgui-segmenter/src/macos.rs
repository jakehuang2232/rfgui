//! macOS — `CFStringTokenizer` with word and line units.
//!
//! Apple's tokenizer is dictionary-backed for Chinese / Japanese / Thai /
//! Khmer / Burmese and UAX#29 elsewhere — same engine that powers the
//! system text input pipeline.
//!
//! `CFStringTokenizer` reports ranges in **UTF-16 code units**. We
//! translate back to char or byte indices depending on the caller.

use std::ffi::c_void;

use core_foundation::base::{CFRange, TCFType};
use core_foundation::string::{CFString, CFStringRef};

use crate::fallback::UnicodeSegmenter;
use crate::{
    GraphemeSegmenter, LineSegmenter, WordSegmenter, build_utf16_to_byte_map,
    build_utf16_to_char_map,
};

type CFStringTokenizerRef = *mut c_void;
type CFLocaleRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFOptionFlags = usize;
type CFStringTokenizerTokenType = CFOptionFlags;

#[allow(non_upper_case_globals)]
const kCFStringTokenizerUnitWordBoundary: CFOptionFlags = 4;
#[allow(non_upper_case_globals)]
const kCFStringTokenizerUnitLineBreak: CFOptionFlags = 3;
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
    fn word_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        cf_token_boundaries_char_indices(text, kCFStringTokenizerUnitWordBoundary)
    }

    fn word_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        cf_token_boundaries_byte_indices(text, kCFStringTokenizerUnitWordBoundary)
    }
}

impl LineSegmenter for CfStringTokenizerSegmenter {
    fn line_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        cf_token_boundaries_char_indices(text, kCFStringTokenizerUnitLineBreak)
    }

    fn line_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        cf_token_boundaries_byte_indices(text, kCFStringTokenizerUnitLineBreak)
    }
}

impl GraphemeSegmenter for CfStringTokenizerSegmenter {
    fn grapheme_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().grapheme_boundaries_char_indices(text)
    }

    fn grapheme_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().grapheme_boundaries_byte_indices(text)
    }
}

fn cf_token_boundaries_char_indices(text: &str, unit: CFOptionFlags) -> Vec<usize> {
    cf_token_boundaries(
        text,
        unit,
        &build_utf16_to_char_map(text),
        text.chars().count(),
    )
}

fn cf_token_boundaries_byte_indices(text: &str, unit: CFOptionFlags) -> Vec<usize> {
    cf_token_boundaries(text, unit, &build_utf16_to_byte_map(text), text.len())
}

fn cf_token_boundaries(
    text: &str,
    unit: CFOptionFlags,
    utf16_to_index: &[usize],
    total: usize,
) -> Vec<usize> {
    let total_chars = text.chars().count();
    if total_chars == 0 {
        return vec![0];
    }

    let cf_string = CFString::new(text);
    let cf_ref = cf_string.as_concrete_TypeRef();
    let utf16_len = unsafe { CFStringGetLength(cf_ref) };

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
            unit,
            std::ptr::null(),
        )
    };
    if tokenizer.is_null() {
        // Couldn't create — degrade to {0, total}.
        out.push(total);
        return out;
    }

    loop {
        let kind = unsafe { CFStringTokenizerAdvanceToNextToken(tokenizer) };
        if kind == kCFStringTokenizerTokenNone {
            break;
        }
        let range = unsafe { CFStringTokenizerGetCurrentTokenRange(tokenizer) };
        let end_utf16 = (range.location + range.length).max(0) as usize;
        let end = utf16_to_index.get(end_utf16).copied().unwrap_or(total);
        if Some(&end) != out.last() {
            out.push(end);
        }
    }

    unsafe { CFRelease(tokenizer as *const c_void) };

    if out.last() != Some(&total) {
        out.push(total);
    }
    out
}

#[cfg(test)]
mod tests;
