//! macOS — `CFStringTokenizer` with word and line units.
//!
//! Apple's tokenizer is dictionary-backed for Chinese / Japanese / Thai /
//! Khmer / Burmese and UAX#29 elsewhere — same engine that powers the
//! system text input pipeline.
//!
//! `CFStringTokenizer` reports ranges in **UTF-16 code units**. We
//! translate back to char or byte indices depending on the caller.

use std::cell::RefCell;
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

    fn CFStringTokenizerSetString(
        tokenizer: CFStringTokenizerRef,
        string: CFStringRef,
        range: CFRange,
    );

    fn CFStringTokenizerAdvanceToNextToken(
        tokenizer: CFStringTokenizerRef,
    ) -> CFStringTokenizerTokenType;

    fn CFStringTokenizerGetCurrentTokenRange(tokenizer: CFStringTokenizerRef) -> CFRange;

    fn CFRelease(cf: *const c_void);

    fn CFStringGetLength(s: CFStringRef) -> isize;
}

/// An owned `CFStringTokenizer`.
struct Tokenizer(CFStringTokenizerRef);

impl Drop for Tokenizer {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0 as *const c_void) };
    }
}

thread_local! {
    /// One tokenizer per unit, reused across calls: creating one loads the
    /// system's language tokenizer and break iterator, which costs far more
    /// than tokenizing a typical label.
    static TOKENIZERS: RefCell<Vec<(CFOptionFlags, Tokenizer)>> = const { RefCell::new(Vec::new()) };
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
    cf_string_token_boundaries(cf_string.as_concrete_TypeRef(), unit, utf16_to_index, total)
}

/// Boundaries of a non-empty `cf_ref`, read with this thread's cached
/// tokenizer for `unit`. The tokenizer keeps `cf_ref` until the next call or
/// [`release_cached_text`].
fn cf_string_token_boundaries(
    cf_ref: CFStringRef,
    unit: CFOptionFlags,
    utf16_to_index: &[usize],
    total: usize,
) -> Vec<usize> {
    let utf16_len = unsafe { CFStringGetLength(cf_ref) };

    let mut out = Vec::new();
    out.push(0usize);

    let range = CFRange {
        location: 0,
        length: utf16_len,
    };
    let tokenized = TOKENIZERS.with(|tokenizers| {
        let mut tokenizers = tokenizers.borrow_mut();
        let tokenizer = match tokenizers.iter().find(|(cached, _)| *cached == unit) {
            Some((_, tokenizer)) => {
                unsafe { CFStringTokenizerSetString(tokenizer.0, cf_ref, range) };
                tokenizer.0
            }
            None => {
                let tokenizer = unsafe {
                    CFStringTokenizerCreate(std::ptr::null(), cf_ref, range, unit, std::ptr::null())
                };
                if tokenizer.is_null() {
                    return false;
                }
                tokenizers.push((unit, Tokenizer(tokenizer)));
                tokenizer
            }
        };
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
        true
    });
    if !tokenized {
        // Couldn't create — degrade to {0, total}.
        out.push(total);
        return out;
    }

    if out.last() != Some(&total) {
        out.push(total);
    }
    out
}

/// Points this thread's cached tokenizers at a one-space string, so they stop
/// retaining the last tokenized text. The tokenizers themselves stay cached.
/// An empty string would not do: setting a zero-length range keeps the
/// previous string.
pub fn release_cached_text() {
    TOKENIZERS.with(|tokenizers| {
        let tokenizers = tokenizers.borrow();
        if tokenizers.is_empty() {
            return;
        }
        let placeholder = CFString::new(" ");
        let range = CFRange {
            location: 0,
            length: 1,
        };
        for (_, tokenizer) in tokenizers.iter() {
            unsafe {
                CFStringTokenizerSetString(tokenizer.0, placeholder.as_concrete_TypeRef(), range)
            };
        }
    });
}

#[cfg(test)]
mod tests;
