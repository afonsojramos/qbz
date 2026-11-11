//! Release-date label formatting for album cards (#469).
//!
//! Renders a Qobuz release date as a fixed "MMM D, YYYY" structure (e.g.
//! "Nov 6, 2025") with a *localized* abbreviated month. We deliberately keep
//! the month / day / year order fixed (not the locale-reordered output of a
//! `toLocaleDateString`-style call) but localize the month token, because the
//! app ships five UI languages (en / es / de / fr / pt). The Slint UI is
//! English-only during the migration, so [`current_locale`] returns English
//! today — it is the single place to wire the real UI language once Slint
//! gets translation support, and no caller needs to change when it does.

use chrono::{Datelike, Locale, NaiveDate};

/// UI language used to render date labels. Maps the active runtime language
/// (`qbz_i18n::current_language()`, set at startup and on live switch) to the
/// matching `chrono` locale so month tokens localize with the rest of the UI.
/// The app supports en / es / de / fr / pt; unknown values fall back to English.
pub fn current_locale() -> Locale {
    match qbz_i18n::current_language() {
        "es" => Locale::es_ES,
        "de" => Locale::de_DE,
        "fr" => Locale::fr_FR,
        "pt" => Locale::pt_PT,
        _ => Locale::en_US,
    }
}

/// Format a Qobuz release date string ("YYYY-MM-DD", possibly with a trailing
/// time component) as "MMM D, YYYY" with a localized month. Falls back to the
/// bare 4-digit year when only a year is available or the value cannot be
/// parsed as a full date, and to an empty string when there is no date.
pub fn release_label(date: Option<&str>) -> String {
    let Some(raw) = date else {
        return String::new();
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    // Only the leading YYYY-MM-DD matters; ignore any trailing time.
    let head = raw.get(0..10).unwrap_or(raw);
    if let Ok(parsed) = NaiveDate::parse_from_str(head, "%Y-%m-%d") {
        // %b = localized abbreviated month, %-d = day without leading zero.
        return parsed
            .format_localized("%b %-d, %Y", current_locale())
            .to_string();
    }
    // Year-only fallback (e.g. the source only had "2025").
    raw.get(0..4).unwrap_or_default().to_string()
}

/// Format a Qobuz release date as "MMMM D, YYYY" (FULL localized month) —
/// the Album Info modal's "Released by … on <date>" line (Slint
/// `info_modals.rs::full_release_date`, Tauri `formatReleaseDate`). Empty
/// when the value is missing or unparseable (no year fallback here: the
/// header already shows the short form elsewhere).
pub fn full_release_label(date: Option<&str>) -> String {
    let Some(raw) = date else {
        return String::new();
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let head = raw.get(0..10).unwrap_or(raw);
    if let Ok(parsed) = NaiveDate::parse_from_str(head, "%Y-%m-%d") {
        // %B = localized full month, %-d = day without leading zero.
        return parsed
            .format_localized("%B %-d, %Y", current_locale())
            .to_string();
    }
    String::new()
}

/// Locale-independent chronological key. Zero means unknown; display labels
/// must never be used as sorting data. A year-only value uses January 1.
pub fn release_sort_key(date: Option<&str>) -> u32 {
    let Some(raw) = date.map(str::trim) else {
        return 0;
    };
    if raw.len() == 4 && raw.bytes().all(|b| b.is_ascii_digit()) {
        return raw
            .parse::<u32>()
            .ok()
            .filter(|y| *y > 0)
            .map(|y| y * 10_000 + 101)
            .unwrap_or(0);
    }
    let Some(head) = raw.get(..10) else {
        return 0;
    };
    match NaiveDate::parse_from_str(head, "%Y-%m-%d") {
        Ok(date) if date.year() > 0 => {
            date.year() as u32 * 10_000 + date.month() * 100 + date.day()
        }
        _ => 0,
    }
}

/// Unknown dates stay last in either direction; stable sort preserves ties.
pub fn compare_release_dates(a: u32, b: u32, newest: bool) -> std::cmp::Ordering {
    match (a == 0, b == 0) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ if newest => b.cmp(&a),
        _ => a.cmp(&b),
    }
}

#[cfg(test)]
mod release_sort_tests {
    use super::*;

    #[test]
    fn original_dates_sort_chronologically_not_by_month_label() {
        let dates = [
            "2015-09-04",
            "1980-04-01",
            "2021-09-03",
            "1985-08-01",
            "1981-02-01",
        ];
        let mut keys: Vec<_> = dates.iter().map(|d| release_sort_key(Some(d))).collect();
        keys.push(0);
        keys.sort_by(|a, b| compare_release_dates(*a, *b, false));
        assert_eq!(keys, [19800401, 19810201, 19850801, 20150904, 20210903, 0]);
        keys.sort_by(|a, b| compare_release_dates(*a, *b, true));
        assert_eq!(keys, [20210903, 20150904, 19850801, 19810201, 19800401, 0]);
        assert_eq!(release_sort_key(Some("2021-09-03T12:00:00Z")), 20210903);
        assert_eq!(release_sort_key(Some("1985")), 19850101);
        for bad in [None, Some(""), Some("Sep 4, 2015"), Some("2021-02-30")] {
            assert_eq!(release_sort_key(bad), 0);
        }
    }
}
