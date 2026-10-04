//! Pinned native checkerpool.go selection for this opt-in probe only.

/// Use the complete Program array, including skipped files and bundled libs.
/// Native does not cap checker counts by available CPUs or memory estimates.
pub(super) fn checker_count(requested: Option<i32>, files: usize, single_threaded: bool) -> usize {
    let requested = if single_threaded {
        1
    } else {
        usize::try_from(requested.unwrap_or(4).max(1)).expect("positive i32 count fits usize")
    };
    requested.min(files.max(1)).min(256)
}

/// Positional counts are an opt-in probe override, not the compiler CLI parser.
pub(super) fn parse_override(value: Option<&str>) -> Result<Option<i32>, &'static str> {
    let Some(value) = value else { return Ok(None) };
    match value.parse::<i32>() {
        Ok(count) if count > 0 => Ok(Some(count)),
        _ => Err("worker count must be a positive 32-bit integer"),
    }
}

#[cfg(test)]
mod tests {
    use super::{checker_count, parse_override};

    include!("native-selection-cases.rs");

    #[test]
    fn selection_matches_two_hundred_pinned_native_pool_observations() {
        for &(requested, files, single, expected) in NATIVE_CASES {
            assert_eq!(
                checker_count(requested, files, single),
                expected,
                "requested={requested:?}, files={files}, single={single}"
            );
        }
    }

    #[test]
    fn defaults_and_explicit_counts_use_full_program_length() {
        assert_eq!(checker_count(None, 1000, false), 4);
        assert_eq!(checker_count(Some(8), 1000, false), 8);
        assert_eq!(checker_count(Some(i32::MAX), 1000, false), 256);
        assert_eq!(checker_count(Some(8), 3, false), 3);
        assert_eq!(checker_count(None, 0, false), 1);
        assert_eq!(checker_count(Some(0), 1000, false), 1);
        assert_eq!(checker_count(Some(-3), 1000, false), 1);
    }

    #[test]
    fn single_threaded_wins_over_all_override_values() {
        for requested in [None, Some(-3), Some(0), Some(1), Some(8), Some(i32::MAX)] {
            for files in [0, 1, 3, 1000] {
                assert_eq!(checker_count(requested, files, true), 1);
            }
        }
    }

    #[test]
    fn standalone_override_is_optional_and_validated_before_loading() {
        assert_eq!(parse_override(None), Ok(None));
        assert_eq!(parse_override(Some("8")), Ok(Some(8)));
        for invalid in ["0", "-1", "1.5", "many", "2147483648", ""] {
            assert!(parse_override(Some(invalid)).is_err());
        }
    }
}
