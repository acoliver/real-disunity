//! The one-line summaries of the run report that rank counts by name.

use std::collections::{BTreeMap, BTreeSet};

/// `title` and the `top` names with the largest counts, largest first and equal counts by name,
/// as `title: name count, name count`. A count of names past `top` ends the line as `and N more`.
/// There is no line for no counts.
pub(crate) fn top_counts_line(
    title: &str,
    counts: &BTreeMap<String, usize>,
    top: usize,
) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let mut ranked: Vec<_> = counts.iter().collect();
    // A stable sort keeps the names of equal counts in the map's order.
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    let named: Vec<String> = ranked
        .iter()
        .take(top)
        .map(|(name, count)| format!("{name} {count}"))
        .collect();
    let line = format!("{title}: {}", named.join(", "));
    Some(match ranked.len().saturating_sub(top) {
        0 => line,
        more => format!("{line}, and {more} more"),
    })
}

/// `title: _A, _B`: the first `top` of `names` in name order, and `and N more` for those past
/// them. There is no line for no names.
pub(crate) fn names_line(title: &str, names: &BTreeSet<String>, top: usize) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let shown: Vec<&str> = names.iter().take(top).map(String::as_str).collect();
    let line = format!("{title}: {}", shown.join(", "));
    Some(match names.len().saturating_sub(top) {
        0 => line,
        more => format!("{line}, and {more} more"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
        entries
            .iter()
            .map(|(name, count)| ((*name).to_owned(), *count))
            .collect()
    }

    #[test]
    fn the_missing_texture_line_names_the_textures_with_the_most_draws_first() {
        let counts = counted(&[("_A", 60), ("_B", 120), ("_C", 60)]);
        assert_eq!(
            top_counts_line("draw:MissingTexture by texture", &counts, 5).as_deref(),
            Some("draw:MissingTexture by texture: _B 120, _A 60, _C 60")
        );
    }

    #[test]
    fn the_missing_texture_line_counts_the_textures_past_the_top_ones() {
        let counts = counted(&[("_A", 4), ("_B", 3), ("_C", 2), ("_D", 1)]);
        assert_eq!(
            top_counts_line("t", &counts, 2).as_deref(),
            Some("t: _A 4, _B 3, and 2 more")
        );
    }

    #[test]
    fn there_is_no_missing_texture_line_without_missing_textures() {
        assert_eq!(top_counts_line("t", &BTreeMap::new(), 5), None);
    }

    fn names(entries: &[&str]) -> BTreeSet<String> {
        entries.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn the_names_line_names_the_first_names_in_order_and_counts_the_rest() {
        assert_eq!(
            names_line("t", &names(&["_B", "_C", "_A"]), 8).as_deref(),
            Some("t: _A, _B, _C")
        );
        assert_eq!(
            names_line("t", &names(&["_D", "_B", "_E", "_A", "_C"]), 2).as_deref(),
            Some("t: _A, _B, and 3 more")
        );
        assert_eq!(names_line("t", &BTreeSet::new(), 8), None);
    }
}
