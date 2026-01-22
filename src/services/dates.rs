use chrono::{Datelike, Local, NaiveDate, Weekday};

/// Represents a date range for filtering notes
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// Represents an ISO week reference
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IsoWeek {
    pub year: i32,
    pub week: u32,
}

impl IsoWeek {
    /// Returns the Monday of this ISO week
    pub fn monday(&self) -> Option<NaiveDate> {
        date_for_iso_week(self.year, self.week)
    }

    /// Returns the date range (Monday-Sunday) for this ISO week
    pub fn date_range(&self) -> Option<DateRange> {
        let monday = self.monday()?;
        Some(DateRange {
            start: monday,
            end: monday + chrono::Duration::days(6),
        })
    }
}

/// Represents all common date/time references in English
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DateReference {
    /// Specific date
    Date(NaiveDate),
    /// Date range
    Range(DateRange),
    /// ISO week
    Week(IsoWeek),
}

impl DateReference {
    #[allow(dead_code)]
    pub fn as_date(&self) -> Option<NaiveDate> {
        match self {
            DateReference::Date(date) => Some(*date),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_range(&self) -> Option<DateRange> {
        match self {
            DateReference::Range(range) => Some(*range),
            DateReference::Week(week) => week.date_range(),
            DateReference::Date(date) => Some(DateRange {
                start: *date,
                end: *date,
            }),
        }
    }

    #[allow(dead_code)]
    pub fn as_week(&self) -> Option<IsoWeek> {
        match self {
            DateReference::Week(week) => Some(*week),
            _ => None,
        }
    }
}

/// Parses any common English date/time reference
/// Examples: "today", "last week", "next Monday", "in 3 days", "2026-W4"
pub fn parse_date_reference(query: &str) -> Option<DateReference> {
    let lowered = query.to_lowercase();
    let today = Local::now().date_naive();

    // Explicit ISO week (2026-W4)
    if let Some(week) = parse_explicit_week(&lowered) {
        return Some(DateReference::Week(week));
    }

    // Single day references
    if contains_word(&lowered, "today") {
        return Some(DateReference::Date(today));
    }
    if contains_word(&lowered, "tomorrow") {
        return Some(DateReference::Date(today + chrono::Duration::days(1)));
    }
    if contains_word(&lowered, "yesterday") {
        return Some(DateReference::Date(today - chrono::Duration::days(1)));
    }

    // Week references
    if lowered.contains("this week") {
        return Some(DateReference::Week(current_week()));
    }
    if lowered.contains("last week")
        || lowered.contains("past week")
        || lowered.contains("previous week")
    {
        return Some(DateReference::Week(last_week()));
    }
    if lowered.contains("next week") {
        return Some(DateReference::Week(next_week()));
    }

    // Month references
    if lowered.contains("this month") {
        return Some(DateReference::Range(this_month_range(today)));
    }
    if lowered.contains("last month")
        || lowered.contains("past month")
        || lowered.contains("previous month")
    {
        return Some(DateReference::Range(last_month_range(today)));
    }
    if lowered.contains("next month") {
        return Some(DateReference::Range(next_month_range(today)));
    }

    // Year references
    if lowered.contains("this year") {
        return Some(DateReference::Range(this_year_range(today)));
    }
    if lowered.contains("last year")
        || lowered.contains("past year")
        || lowered.contains("previous year")
    {
        return Some(DateReference::Range(last_year_range(today)));
    }
    if lowered.contains("next year") {
        return Some(DateReference::Range(next_year_range(today)));
    }

    // Relative day offsets ("in 3 days", "5 days ago")
    if let Some(offset) = parse_day_offset(&lowered) {
        let date = today + chrono::Duration::days(offset);
        return Some(DateReference::Date(date));
    }

    // Weekday references ("next Monday", "last Friday", "this Thursday")
    if let Some(date) = parse_weekday_reference(&lowered, today) {
        return Some(DateReference::Date(date));
    }

    // N days/weeks/months range ("last 7 days", "past 2 weeks", "last 3 months")
    if let Some(range) = parse_relative_range(&lowered, today) {
        return Some(DateReference::Range(range));
    }

    None
}

/// Returns the current ISO week
pub fn current_week() -> IsoWeek {
    let today = Local::now().date_naive();
    let iso = today.iso_week();
    IsoWeek {
        year: iso.year(),
        week: iso.week(),
    }
}

/// Returns the previous ISO week (handles year boundaries)
pub fn last_week() -> IsoWeek {
    let current = current_week();
    if current.week == 1 {
        let prev_year = current.year - 1;
        let last_week_of_prev_year = weeks_in_year(prev_year);
        IsoWeek {
            year: prev_year,
            week: last_week_of_prev_year,
        }
    } else {
        IsoWeek {
            year: current.year,
            week: current.week - 1,
        }
    }
}

/// Returns the next ISO week (handles year boundaries)
pub fn next_week() -> IsoWeek {
    let current = current_week();
    let max_week = weeks_in_year(current.year);
    if current.week >= max_week {
        IsoWeek {
            year: current.year + 1,
            week: 1,
        }
    } else {
        IsoWeek {
            year: current.year,
            week: current.week + 1,
        }
    }
}

/// Parses explicit week references like "2026-W4", "2026-W04", "2026-w4"
pub fn parse_explicit_week(query: &str) -> Option<IsoWeek> {
    let lowered = query.to_lowercase();
    for token in lowered.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if let Some(week) = parse_week_token(cleaned) {
            return Some(week);
        }
    }
    None
}

/// Resolves which week to use based on query keywords
pub fn resolve_query_week(query: &str) -> IsoWeek {
    let lowered = query.to_lowercase();
    
    // Check for explicit week reference first (e.g., "2026-W4")
    if let Some(week) = parse_explicit_week(&lowered) {
        return week;
    }
    
    // Check for "last week"
    if lowered.contains("last week") {
        return last_week();
    }
    
    // Default: current week
    current_week()
}

/// Computes the Monday of a specific ISO week
pub fn date_for_iso_week(year: i32, week: u32) -> Option<NaiveDate> {
    if week < 1 || week > 53 {
        return None;
    }
    // ISO week 1 is the week containing the first Thursday of the year
    let jan4 = NaiveDate::from_ymd_opt(year, 1, 4)?;
    let days_from_monday = jan4.weekday().num_days_from_monday() as i64;
    let week1_monday = jan4 - chrono::Duration::days(days_from_monday);
    let target_monday = week1_monday + chrono::Duration::weeks(i64::from(week) - 1);
    Some(target_monday)
}

// Month range helpers

fn this_month_range(today: NaiveDate) -> DateRange {
    let year = today.year();
    let month = today.month();
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let end = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap() - chrono::Duration::days(1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap() - chrono::Duration::days(1)
    };
    DateRange { start, end }
}

fn last_month_range(today: NaiveDate) -> DateRange {
    let year = today.year();
    let month = today.month();
    let (prev_year, prev_month) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    let start = NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(year, month, 1).unwrap() - chrono::Duration::days(1);
    DateRange { start, end }
}

fn next_month_range(today: NaiveDate) -> DateRange {
    let year = today.year();
    let month = today.month();
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let start = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    let end = if next_month == 12 {
        NaiveDate::from_ymd_opt(next_year + 1, 1, 1).unwrap() - chrono::Duration::days(1)
    } else {
        NaiveDate::from_ymd_opt(next_year, next_month + 1, 1).unwrap() - chrono::Duration::days(1)
    };
    DateRange { start, end }
}

// Year range helpers

fn this_year_range(today: NaiveDate) -> DateRange {
    let year = today.year();
    let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    DateRange { start, end }
}

fn last_year_range(today: NaiveDate) -> DateRange {
    let year = today.year() - 1;
    let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    DateRange { start, end }
}

fn next_year_range(today: NaiveDate) -> DateRange {
    let year = today.year() + 1;
    let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    DateRange { start, end }
}

// Relative range parsing ("last 7 days", "past 2 weeks", "next 3 days")

fn parse_relative_range(lowered: &str, today: NaiveDate) -> Option<DateRange> {
    let tokens: Vec<&str> = lowered.split_whitespace().collect();
    
    // "last N days" or "past N days"
    for i in 0..tokens.len().saturating_sub(2) {
        if tokens[i] == "last" || tokens[i] == "past" {
            if let Ok(count) = tokens[i + 1].parse::<i64>() {
                if tokens[i + 2] == "days" || tokens[i + 2] == "day" {
                    let start = today - chrono::Duration::days(count);
                    return Some(DateRange { start, end: today });
                }
                if tokens[i + 2] == "weeks" || tokens[i + 2] == "week" {
                    let start = today - chrono::Duration::weeks(count);
                    return Some(DateRange { start, end: today });
                }
                if tokens[i + 2] == "months" || tokens[i + 2] == "month" {
                    let start = today - chrono::Duration::days(count * 30);
                    return Some(DateRange { start, end: today });
                }
            }
        }
    }

    // "next N days/weeks/months"
    for i in 0..tokens.len().saturating_sub(2) {
        if tokens[i] == "next" {
            if let Ok(count) = tokens[i + 1].parse::<i64>() {
                if tokens[i + 2] == "days" || tokens[i + 2] == "day" {
                    let end = today + chrono::Duration::days(count);
                    return Some(DateRange { start: today, end });
                }
                if tokens[i + 2] == "weeks" || tokens[i + 2] == "week" {
                    let end = today + chrono::Duration::weeks(count);
                    return Some(DateRange { start: today, end });
                }
                if tokens[i + 2] == "months" || tokens[i + 2] == "month" {
                    let end = today + chrono::Duration::days(count * 30);
                    return Some(DateRange { start: today, end });
                }
            }
        }
    }
    
    None
}

// Weekday parsing

fn parse_weekday_reference(lowered: &str, today: NaiveDate) -> Option<NaiveDate> {
    let weekday = parse_weekday(lowered)?;
    let today_weekday = today.weekday();
    let mut delta = weekday.num_days_from_monday() as i64
        - today_weekday.num_days_from_monday() as i64;

    if lowered.contains("next ") {
        if delta <= 0 {
            delta += 7;
        }
    } else if lowered.contains("last ") {
        if delta >= 0 {
            delta -= 7;
        }
    } else if lowered.contains("this ") {
        if delta < 0 {
            delta += 7;
        }
    } else if delta <= 0 {
        // Default: assume future reference
        delta += 7;
    }

    Some(today + chrono::Duration::days(delta))
}

fn parse_weekday(text: &str) -> Option<Weekday> {
    if text.contains("monday") || text.contains("mon") {
        return Some(Weekday::Mon);
    }
    if text.contains("tuesday") || text.contains("tue") {
        return Some(Weekday::Tue);
    }
    if text.contains("wednesday") || text.contains("wed") {
        return Some(Weekday::Wed);
    }
    if text.contains("thursday") || text.contains("thu") {
        return Some(Weekday::Thu);
    }
    if text.contains("friday") || text.contains("fri") {
        return Some(Weekday::Fri);
    }
    if text.contains("saturday") || text.contains("sat") {
        return Some(Weekday::Sat);
    }
    if text.contains("sunday") || text.contains("sun") {
        return Some(Weekday::Sun);
    }
    None
}

// Day offset parsing ("in 3 days", "5 days ago")

fn parse_day_offset(text: &str) -> Option<i64> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    
    // "N days ago" or "N day ago"
    for window in tokens.windows(3) {
        if let [number, "days" | "day", "ago"] = window {
            if let Ok(value) = number.parse::<i64>() {
                return Some(-value);
            }
        }
    }
    
    // "in N days" or "in N day"
    for i in 0..tokens.len().saturating_sub(2) {
        if tokens[i] == "in" {
            if let Ok(value) = tokens[i + 1].parse::<i64>() {
                if tokens[i + 2] == "days" || tokens[i + 2] == "day" {
                    return Some(value);
                }
            }
        }
    }
    
    None
}

// Word boundary checking

fn contains_word(text: &str, word: &str) -> bool {
    text.split_whitespace().any(|w| w == word)
}

/// Checks if a date falls within a range (inclusive)
pub fn date_in_range(date: NaiveDate, range: DateRange) -> bool {
    date >= range.start && date <= range.end
}

// Private helpers

fn parse_week_token(token: &str) -> Option<IsoWeek> {
    let parts: Vec<&str> = token.split("-w").collect();
    if parts.len() != 2 {
        return None;
    }
    if parts[0].len() != 4 {
        return None;
    }
    let year = parts[0].parse::<i32>().ok()?;
    let week_str = parts[1].trim_start_matches('0');
    let week = if week_str.is_empty() {
        0
    } else {
        week_str.parse::<u32>().ok()?
    };
    if week < 1 || week > 53 {
        return None;
    }
    Some(IsoWeek { year, week })
}

fn weeks_in_year(year: i32) -> u32 {
    // ISO 8601: A year has 53 weeks if Dec 31 (or Dec 30 for leap years) is a Thursday
    let dec31 = NaiveDate::from_ymd_opt(year, 12, 31).unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(year, 12, 30).unwrap()
    });
    let iso = dec31.iso_week();
    if iso.year() == year {
        iso.week()
    } else {
        52
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_explicit_week() {
        assert_eq!(
            parse_explicit_week("show me 2026-W4 note").map(|w| (w.year, w.week)),
            Some((2026, 4))
        );
        assert_eq!(
            parse_explicit_week("2026-W04").map(|w| (w.year, w.week)),
            Some((2026, 4))
        );
        assert_eq!(
            parse_explicit_week("2026-w4").map(|w| (w.year, w.week)),
            Some((2026, 4))
        );
        assert_eq!(parse_explicit_week("no week here"), None);
    }

    #[test]
    fn test_date_for_iso_week() {
        let date = date_for_iso_week(2026, 4).unwrap();
        let iso = date.iso_week();
        assert_eq!(iso.year(), 2026);
        assert_eq!(iso.week(), 4);
        assert_eq!(date.weekday(), Weekday::Mon);
    }

    #[test]
    fn test_parse_date_reference_simple() {
        let today = Local::now().date_naive();
        
        // Today
        if let Some(DateReference::Date(date)) = parse_date_reference("what happened today?") {
            assert_eq!(date, today);
        } else {
            panic!("Failed to parse 'today'");
        }
        
        // Tomorrow
        if let Some(DateReference::Date(date)) = parse_date_reference("what about tomorrow") {
            assert_eq!(date, today + chrono::Duration::days(1));
        } else {
            panic!("Failed to parse 'tomorrow'");
        }
        
        // This week
        if let Some(DateReference::Week(week)) = parse_date_reference("this week") {
            assert_eq!(week, current_week());
        } else {
            panic!("Failed to parse 'this week'");
        }
    }

    #[test]
    fn test_parse_date_reference_offset() {
        let today = Local::now().date_naive();
        
        // "in 3 days"
        if let Some(DateReference::Date(date)) = parse_date_reference("in 3 days") {
            assert_eq!(date, today + chrono::Duration::days(3));
        } else {
            panic!("Failed to parse 'in 3 days'");
        }
        
        // "5 days ago"
        if let Some(DateReference::Date(date)) = parse_date_reference("5 days ago") {
            assert_eq!(date, today - chrono::Duration::days(5));
        } else {
            panic!("Failed to parse '5 days ago'");
        }
    }

    #[test]
    fn test_week_boundaries() {
        let week = last_week();
        assert!(week.week >= 1 && week.week <= 53);
        
        let week = next_week();
        assert!(week.week >= 1 && week.week <= 53);
    }
}
