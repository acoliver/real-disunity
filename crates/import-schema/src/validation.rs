#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HashValidity {
    Valid,
    Invalid,
    AllZero,
}

pub(crate) fn sha256_validity(value: &str) -> HashValidity {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return HashValidity::Invalid;
    }
    if value.bytes().all(|byte| byte == b'0') {
        return HashValidity::AllZero;
    }
    HashValidity::Valid
}

pub(crate) fn sha256_matches_empty_length(value: &str, bytes: u64) -> bool {
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    bytes != 0 || value.eq_ignore_ascii_case(EMPTY_SHA256)
}

pub(crate) fn is_nonempty_text(value: &str) -> bool {
    value.trim() == value && !value.is_empty() && !value.chars().any(char::is_control)
}

pub(crate) fn is_identifier(value: &str) -> bool {
    is_nonempty_text(value)
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
}

/// Accepts absolute URIs of the form `scheme:rest` with an ASCII scheme and no whitespace.
pub(crate) fn is_uri(value: &str) -> bool {
    if !is_nonempty_text(value) || value.contains(' ') {
        return false;
    }
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    let mut scheme_bytes = scheme.bytes();
    let starts_alpha = scheme_bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic());
    starts_alpha
        && !rest.is_empty()
        && scheme_bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
}

pub(crate) fn is_logical_path(value: &str) -> bool {
    if !is_nonempty_text(value)
        || value.contains('\\')
        || value.contains(':')
        || value.starts_with('/')
        || value.ends_with('/')
    {
        return false;
    }
    value
        .split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Returns the index of the first entry that breaks strict sort order or uniqueness.
pub(crate) fn first_unsorted_unique(values: &[String]) -> Option<usize> {
    values
        .windows(2)
        .position(|pair| pair[0] >= pair[1])
        .map(|index| index + 1)
}

/// Sums counts, returning `None` instead of overflowing.
pub(crate) fn checked_sum(mut values: impl Iterator<Item = u64>) -> Option<u64> {
    values.try_fold(0_u64, u64::checked_add)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_paths_reject_host_and_traversal_forms() {
        assert!(is_logical_path("decoded/environment.json"));
        assert!(!is_logical_path("C:/decoded/environment.json"));
        assert!(!is_logical_path("../environment.json"));
        assert!(!is_logical_path("decoded\\environment.json"));
        assert!(!is_logical_path("decoded//environment.json"));
        assert!(!is_logical_path("decoded/./environment.json"));
    }

    #[test]
    fn hash_validation_distinguishes_zero_from_malformed() {
        assert_eq!(sha256_validity(&"1".repeat(64)), HashValidity::Valid);
        assert_eq!(sha256_validity(&"0".repeat(64)), HashValidity::AllZero);
        assert_eq!(sha256_validity("xyz"), HashValidity::Invalid);
        assert_eq!(sha256_validity(&"f".repeat(63)), HashValidity::Invalid);
    }

    #[test]
    fn uri_validation_requires_scheme_and_payload() {
        assert!(is_uri("urn:real-disunity:grid"));
        assert!(is_uri("https://example.invalid/capability/terrain"));
        assert!(!is_uri("real-disunity.grid"));
        assert!(!is_uri("1urn:grid"));
        assert!(!is_uri("urn:"));
        assert!(!is_uri("urn: grid"));
    }

    #[test]
    fn sorted_unique_reports_first_offender() {
        let sorted = ["a".to_owned(), "b".to_owned()].to_vec();
        assert_eq!(first_unsorted_unique(&sorted), None);
        let duplicate = ["a".to_owned(), "a".to_owned()].to_vec();
        assert_eq!(first_unsorted_unique(&duplicate), Some(1));
        let unsorted = ["b".to_owned(), "a".to_owned()].to_vec();
        assert_eq!(first_unsorted_unique(&unsorted), Some(1));
    }

    #[test]
    fn checked_sum_reports_overflow() {
        assert_eq!(checked_sum([1_u64, 2].into_iter()), Some(3));
        assert_eq!(
            checked_sum([u64::MAX, 1].into_iter()),
            None,
            "overflow must surface as None"
        );
    }
}
