//! Test-only helpers for `deepseek-core` unit tests.

/// Assert two strings are byte-identical with a contextual message on failure.
pub(crate) fn assert_byte_identical(context: &str, a: &str, b: &str) {
    assert_eq!(a, b, "{context}");
}
