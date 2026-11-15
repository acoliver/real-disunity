//! Anchored rewrites of WGSL sources.
//!
//! Shader variants are assembled by replacing anchored text in a base source. A plain
//! `str::replace` whose anchor no longer matches returns the base unchanged, and the
//! unchanged base still validates, so a reworded base would silently drop the variant.
//! Every anchored rewrite therefore states how often its anchor occurs and panics, naming
//! the source and the anchor, when the count differs.

/// Replacement of anchored text in shader sources, with an exact match count.
pub(crate) trait ShaderRewrite {
    /// Replaces each of exactly `count` occurrences of `anchor` with `replacement`.
    ///
    /// # Panics
    ///
    /// When `anchor` is empty or does not occur exactly `count` times in the source; the
    /// message names `source_name` and the anchor.
    fn anchored(&self, source_name: &str, count: usize, anchor: &str, replacement: &str) -> String;

    /// Replaces only the first of exactly `count` occurrences of `anchor`.
    ///
    /// # Panics
    ///
    /// As [`ShaderRewrite::anchored`].
    fn anchored_first(
        &self,
        source_name: &str,
        count: usize,
        anchor: &str,
        replacement: &str,
    ) -> String;

    /// Replaces every occurrence of `anchor`, of which there are at least `minimum`. For
    /// substitutions of every use of an expression in a source whose callers may add uses.
    ///
    /// # Panics
    ///
    /// When `anchor` is empty or occurs fewer than `minimum` times; the message names
    /// `source_name` and the anchor.
    fn anchored_at_least(
        &self,
        source_name: &str,
        minimum: usize,
        anchor: &str,
        replacement: &str,
    ) -> String;
}

impl ShaderRewrite for str {
    #[track_caller]
    fn anchored(&self, source_name: &str, count: usize, anchor: &str, replacement: &str) -> String {
        expect_matches(self, source_name, count, anchor);
        self.replace(anchor, replacement)
    }

    #[track_caller]
    fn anchored_first(
        &self,
        source_name: &str,
        count: usize,
        anchor: &str,
        replacement: &str,
    ) -> String {
        expect_matches(self, source_name, count, anchor);
        self.replacen(anchor, replacement, 1)
    }

    #[track_caller]
    fn anchored_at_least(
        &self,
        source_name: &str,
        minimum: usize,
        anchor: &str,
        replacement: &str,
    ) -> String {
        let found = matches(self, source_name, anchor);
        assert!(
            found >= minimum,
            "shader rewrite of {source_name}: anchor {anchor:?} matched {found} times, expected at least {minimum}"
        );
        self.replace(anchor, replacement)
    }
}

#[track_caller]
fn matches(source: &str, source_name: &str, anchor: &str) -> usize {
    assert!(
        !anchor.is_empty(),
        "shader rewrite of {source_name}: empty anchor"
    );
    source.matches(anchor).count()
}

#[track_caller]
fn expect_matches(source: &str, source_name: &str, count: usize, anchor: &str) {
    let found = matches(source, source_name, anchor);
    assert!(
        found == count,
        "shader rewrite of {source_name}: anchor {anchor:?} matched {found} times, expected {count}"
    );
}

#[cfg(test)]
mod tests {
    use super::ShaderRewrite;

    #[test]
    fn anchored_replaces_every_expected_occurrence() {
        assert_eq!("a b a".anchored("test source", 2, "a", "c"), "c b c");
        assert_eq!("a b a".anchored_first("test source", 2, "a", "c"), "c b a");
    }

    #[test]
    #[should_panic(
        expected = "shader rewrite of test source: anchor \"missing\" matched 0 times, expected 1"
    )]
    fn anchored_panics_when_the_anchor_is_absent() {
        let _ = "fn main() {}".anchored("test source", 1, "missing", "x");
    }

    #[test]
    #[should_panic(
        expected = "shader rewrite of test source: anchor \"a\" matched 3 times, expected 2"
    )]
    fn anchored_panics_on_more_matches_than_expected() {
        let _ = "a a a".anchored("test source", 2, "a", "b");
    }

    #[test]
    #[should_panic(
        expected = "shader rewrite of test source: anchor \"a\" matched 1 times, expected 2"
    )]
    fn anchored_first_panics_on_fewer_matches_than_expected() {
        let _ = "a".anchored_first("test source", 2, "a", "b");
    }

    #[test]
    fn anchored_at_least_replaces_every_occurrence_above_the_minimum() {
        assert_eq!(
            "a b a".anchored_at_least("test source", 1, "a", "c"),
            "c b c"
        );
    }

    #[test]
    #[should_panic(
        expected = "shader rewrite of test source: anchor \"a\" matched 1 times, expected at least 2"
    )]
    fn anchored_at_least_panics_below_the_minimum() {
        let _ = "a".anchored_at_least("test source", 2, "a", "b");
    }

    #[test]
    #[should_panic(expected = "shader rewrite of test source: empty anchor")]
    fn anchored_rejects_an_empty_anchor() {
        let _ = "a".anchored("test source", 2, "", "b");
    }
}
