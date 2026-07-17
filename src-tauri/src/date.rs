//! Shared date type + parser for EU4's `Y.M.D` dates (Sprint 12).
//!
//! Before Sprint 12 four modules each carried their own `parse_date` and a
//! hardwired `1444.11.11` start constant. The whole app is now parameterized on
//! a *selected date*; this module is the single source of truth for the type,
//! the parser, and the historical default start.

/// A `(year, month, day)` date. Ordered field-wise, which is chronological for
/// well-formed dates.
pub type Date = (u32, u32, u32);

/// The vanilla grand-campaign start. Used as the fallback when a session has no
/// bookmarks/defines to derive an effective start from, and by the compatibility
/// wrappers that preserve pre-Sprint-12 (`= 1444.11.11`) behavior.
pub const DEFAULT_START: Date = (1444, 11, 11);

/// Parses `"1444.11.11"` into `(1444, 11, 11)`. Rejects anything that isn't
/// exactly three dot-separated unsigned integers.
pub fn parse_date(s: &str) -> Option<Date> {
    let mut it = s.split('.');
    let date = (
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    );
    it.next().is_none().then_some(date)
}

/// Renders a [`Date`] back to canonical `Y.M.D` text.
pub fn format_date(d: Date) -> String {
    format!("{}.{}.{}", d.0, d.1, d.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        assert_eq!(parse_date("1444.11.11"), Some((1444, 11, 11)));
        assert_eq!(parse_date("1300.1.1"), Some((1300, 1, 1)));
        assert_eq!(parse_date(" 142.7.10 "), Some((142, 7, 10)));
        assert_eq!(format_date((1821, 1, 2)), "1821.1.2");
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(parse_date("1444.11"), None);
        assert_eq!(parse_date("1444.11.11.1"), None);
        assert_eq!(parse_date("not.a.date"), None);
        assert_eq!(parse_date(""), None);
    }

    #[test]
    fn orders_chronologically() {
        assert!(parse_date("1444.11.11").unwrap() < parse_date("1453.5.29").unwrap());
        assert!(parse_date("1444.11.11").unwrap() < parse_date("1444.11.12").unwrap());
        assert!(parse_date("1300.1.1").unwrap() < DEFAULT_START);
    }
}
