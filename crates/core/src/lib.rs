//! AI Assistant engine. Nothing here depends on egui, eframe, winit or rfd, and every module builds for
//! `wasm32-unknown-unknown` (`cargo xtask wasm`). All input is untrusted: return `Result`, never panic.

use serde::{Deserialize, Serialize};

/// Errors of the engine.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("input is empty")]
    Empty,
    #[error("input is {len} bytes, more than the {max}-byte limit")]
    TooLarge { len: usize, max: usize },
}

/// Inputs larger than this are refused instead of allocated for.
pub const MAX_INPUT: usize = 64 * 1024 * 1024;

/// A summary of a document, shown by the UI.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub bytes: usize,
    pub lines: usize,
}

/// Summarises `data`. Never panics, whatever the input.
pub fn summarize(data: &[u8]) -> Result<Summary, Error> {
    if data.is_empty() {
        return Err(Error::Empty);
    }
    if data.len() > MAX_INPUT {
        return Err(Error::TooLarge { len: data.len(), max: MAX_INPUT });
    }
    // A final line without a trailing newline still counts; a trailing newline adds no line.
    let newlines = data.iter().filter(|&&b| b == b'\n').count();
    let lines = if data.last() == Some(&b'\n') { newlines } else { newlines.saturating_add(1) };
    Ok(Summary { bytes: data.len(), lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn counts_lines() {
        assert_eq!(summarize(b"a\nb\nc").unwrap(), Summary { bytes: 5, lines: 3 });
    }

    #[test]
    fn a_trailing_newline_adds_no_line() {
        assert_eq!(summarize(b"one\ntwo\n").unwrap(), Summary { bytes: 8, lines: 2 });
    }

    #[test]
    fn refuses_empty_input() {
        assert_eq!(summarize(b""), Err(Error::Empty));
    }

    proptest! {
        #[test]
        fn never_panics(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let _ = summarize(&data);
        }
    }
}
