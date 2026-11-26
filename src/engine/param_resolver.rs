use std::collections::HashMap;
use chrono::{Datelike, Duration, NaiveDate};

/// Helpers for ROC (Republic of China) date calculations and conversions.
pub struct RocDateUtils;

impl RocDateUtils {
    /// Formats a Gregorian date to ROC YYYMMDD string (e.g. 2026-11-24 -> "1151124")
    pub fn to_roc_yyymmdd(date: &NaiveDate, sep: &str) -> String {
        let roc_year = date.year() - 1911;
        format!(
            "{:03}{}{:02}{}{:02}",
            roc_year,
            sep,
            date.month(),
            sep,
            date.day()
        )
    }

    /// Formats a Gregorian date to ROC YYYMM string (e.g. 2026-11-24 -> "11511")
    pub fn to_roc_yyymm(date: &NaiveDate, sep: &str) -> String {
        let roc_year = date.year() - 1911;
        format!("{:03}{}{:02}", roc_year, sep, date.month())
    }

    /// Parses an ROC date string (e.g. "1151124" or "115/11/24") to NaiveDate
    pub fn parse_roc_date(roc_str: &str) -> Option<NaiveDate> {
        let clean = roc_str.replace(['/', '-', '.'], "").trim().to_string();
        if clean.len() < 7 {
            return None;
        }
        let year_str = &clean[..clean.len() - 4];
        let month_str = &clean[clean.len() - 4..clean.len() - 2];
        let day_str = &clean[clean.len() - 2..];

        let roc_year: i32 = year_str.parse().ok()?;
        let month: u32 = month_str.parse().ok()?;
        let day: u32 = day_str.parse().ok()?;

        let greg_year = roc_year + 1911;
        NaiveDate::from_ymd_opt(greg_year, month, day)
    }

    /// Shifts year and month by delta months (positive or negative)
    pub fn shift_months(year: i32, month: u32, delta: i32) -> (i32, u32) {
        let total = (year as i64) * 12 + (month as i64 - 1) + (delta as i64);
        let new_year = total.div_euclid(12) as i32;
        let new_month = (total.rem_euclid(12) + 1) as u32;
        (new_year, new_month)
    }

    /// Returns the number of days in the given month of the given Gregorian year
    pub fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
                if is_leap {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    }
}

/// Generic Dynamic Parameter Resolver implementing Figure 1 (E1..EG), extended date rules,
/// and database-driven custom variable dictionary (`custom_variables`).
pub struct ParamResolver {
    base_date: NaiveDate,
    custom_vars: HashMap<String, String>,
}

impl ParamResolver {
    pub fn new(base_date: NaiveDate) -> Self {
        Self {
            base_date,
            custom_vars: HashMap::new(),
        }
    }

    pub fn with_custom_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.custom_vars = vars
            .into_iter()
            .map(|(k, v)| (k.trim().to_uppercase(), v))
            .collect();
        self
    }

    pub fn base_date(&self) -> NaiveDate {
        self.base_date
    }

    /// Resolve a parameter based on rule code and value/template.
    pub fn resolve(&self, rule_code: &str, rule_value: &str) -> String {
        let code = rule_code.trim().to_uppercase();
        match code.as_str() {
            // Figure 1 Standard Codes (E1 ~ EG)
            "E1" => RocDateUtils::to_roc_yyymmdd(&self.base_date, ""),
            "E2" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(1)), ""),
            "E3" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(2)), ""),
            "E4" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(3)), ""),
            "E5" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(4)), ""),
            "E6" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(5)), ""),
            "E7" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(6)), ""),
            "E8" => RocDateUtils::to_roc_yyymmdd(&(self.base_date - Duration::days(7)), ""),

            "E9" => format!("{:02}", self.base_date.month()),
            "EA" => {
                let (_, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -1);
                format!("{:02}", m)
            }
            "EB" => {
                let (_, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -2);
                format!("{:02}", m)
            }
            "EC" => {
                let (_, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -3);
                format!("{:02}", m)
            }

            "ED" => RocDateUtils::to_roc_yyymm(&self.base_date, ""),
            "EE" => {
                let (y, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -1);
                format!("{:03}{:02}", y - 1911, m)
            }
            "EF" => {
                let (y, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -2);
                format!("{:03}{:02}", y - 1911, m)
            }
            "EG" => {
                let (y, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -3);
                format!("{:03}{:02}", y - 1911, m)
            }

            // Extended Rules
            "DAY_OFFSET" => self.resolve_day_offset(rule_value),
            "MONTH_OFFSET" => self.resolve_month_offset(rule_value),
            "WEEK_DAY" | "WEEK" => self.resolve_week_day(rule_value),
            "MONTH_EDGE" => self.resolve_month_edge(rule_value),
            "DATE_EDGE" => self.resolve_date_edge(rule_value),
            "STATIC" => rule_value.to_string(),
            "TEMPLATE" | "CUSTOM_TEMPLATE" => self.resolve_template(rule_value),

            _ => {
                if rule_value.contains('{') && rule_value.contains('}') {
                    self.resolve_template(rule_value)
                } else if !rule_value.is_empty() {
                    rule_value.to_string()
                } else {
                    self.resolve_token(&code)
                }
            }
        }
    }

    /// Day offset: e.g. "+2", "-10", or "DAY_OFFSET:+2"
    fn resolve_day_offset(&self, value: &str) -> String {
        let val = value
            .trim()
            .strip_prefix("DAY_OFFSET:")
            .unwrap_or(value)
            .trim();
        let offset: i64 = val.trim_start_matches('+').parse().unwrap_or(0);
        let target = self.base_date + Duration::days(offset);
        RocDateUtils::to_roc_yyymmdd(&target, "")
    }

    /// Month offset: e.g. "+1", "-2", or with format "+1:MM", "-1:YYYMM"
    fn resolve_month_offset(&self, value: &str) -> String {
        let val = value
            .trim()
            .strip_prefix("MONTH_OFFSET:")
            .unwrap_or(value)
            .trim();
        let parts: Vec<&str> = val.split(':').collect();
        let offset_str = parts[0].trim_start_matches('+');
        let offset: i32 = offset_str.parse().unwrap_or(0);
        let format_type = parts
            .get(1)
            .map(|s| s.trim().to_uppercase())
            .unwrap_or_else(|| "YYYMM".to_string());

        let (y, m) =
            RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), offset);
        if format_type == "MM" {
            format!("{:02}", m)
        } else {
            format!("{:03}{:02}", y - 1911, m)
        }
    }

    /// Weekday: 1 (Mon) .. 7 (Sun)
    pub fn resolve_week_day(&self, value: &str) -> String {
        let val = value.trim().strip_prefix("WEEK_").unwrap_or(value).trim();
        let parts: Vec<&str> = val.split(':').collect();
        let day_num: u32 = parts[0].parse().unwrap_or(1);
        let sep = parts.get(1).copied().unwrap_or("");

        let days_from_monday = self.base_date.weekday().num_days_from_monday() as i64;
        let monday = self.base_date - Duration::days(days_from_monday);
        let target = monday + Duration::days((day_num.clamp(1, 7) - 1) as i64);

        RocDateUtils::to_roc_yyymmdd(&target, sep)
    }

    /// Month edge: MONTH_FIRST, MONTH_LAST, PREV_MONTH_FIRST, PREV_MONTH_LAST
    fn resolve_month_edge(&self, value: &str) -> String {
        let edge = value.trim().to_uppercase();
        match edge.as_str() {
            "MONTH_FIRST" => {
                let roc_year = self.base_date.year() - 1911;
                format!("{:03}{:02}01", roc_year, self.base_date.month())
            }
            "MONTH_LAST" => {
                let roc_year = self.base_date.year() - 1911;
                let last_day =
                    RocDateUtils::days_in_month(self.base_date.year(), self.base_date.month());
                format!(
                    "{:03}{:02}{:02}",
                    roc_year,
                    self.base_date.month(),
                    last_day
                )
            }
            "PREV_MONTH_FIRST" => {
                let (y, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -1);
                format!("{:03}{:02}01", y - 1911, m)
            }
            "PREV_MONTH_LAST" => {
                let (y, m) =
                    RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), -1);
                let last_day = RocDateUtils::days_in_month(y, m);
                format!("{:03}{:02}{:02}", y - 1911, m, last_day)
            }
            _ => value.to_string(),
        }
    }

    /// Calendar period edges (ROC YYYMMDD). Value format: `KEY` or `KEY:<year offset>`,
    /// KEY = PREV_QUARTER_FIRST | PREV_QUARTER_LAST | PREV_HALF_FIRST | PREV_HALF_LAST
    ///     | YEAR_FIRST | YEAR_LAST.  The year offset (e.g. -4) shifts the base year first.
    fn resolve_date_edge(&self, value: &str) -> String {
        let val = value.trim().to_uppercase();
        let mut parts = val.splitn(2, ':');
        let key = parts.next().unwrap_or("");
        let year_offset: i32 = parts
            .next()
            .map(|s| s.trim().trim_start_matches('+').parse().unwrap_or(0))
            .unwrap_or(0);
        let year = self.base_date.year() + year_offset;
        let month = self.base_date.month();

        // (year, start_month, end_month)
        let (y, start_m, end_m) = match key {
            "PREV_QUARTER_FIRST" | "PREV_QUARTER_LAST" => {
                let q = ((month - 1) / 3) as i32; // 0..=3
                let (y, pq) = if q == 0 { (year - 1, 3) } else { (year, q - 1) };
                let sm = (pq * 3 + 1) as u32;
                (y, sm, sm + 2)
            }
            "PREV_HALF_FIRST" | "PREV_HALF_LAST" => {
                let h = ((month - 1) / 6) as i32; // 0 or 1
                let (y, ph) = if h == 0 { (year - 1, 1) } else { (year, 0) };
                let sm = (ph * 6 + 1) as u32;
                (y, sm, sm + 5)
            }
            "YEAR_FIRST" | "YEAR_LAST" => (year, 1, 12),
            _ => return value.to_string(),
        };

        if key.ends_with("_FIRST") {
            format!("{:03}{:02}01", y - 1911, start_m)
        } else {
            let last = RocDateUtils::days_in_month(y, end_m);
            format!("{:03}{:02}{:02}", y - 1911, end_m, last)
        }
    }

    /// Template interpolation replacing `{TOKEN}` with dynamic value
    pub fn resolve_template(&self, template: &str) -> String {
        let mut result = template.to_string();
        let mut search_from = 0;
        while let Some(rel_start) = result[search_from..].find('{') {
            let start_idx = search_from + rel_start;
            if let Some(end_rel) = result[start_idx..].find('}') {
                let end_idx = start_idx + end_rel;
                let token = &result[start_idx + 1..end_idx];
                let resolved = self.resolve_token(token);
                let resolved_len = resolved.len();
                result.replace_range(start_idx..=end_idx, &resolved);
                search_from = start_idx + resolved_len;
            } else {
                break;
            }
        }
        result
    }

    /// Resolves an individual token inside `{...}`
    fn resolve_token(&self, token: &str) -> String {
        let trimmed = token.trim();
        let upper = trimmed.to_uppercase();

        // Generic monthly dictionary lookup: {MONTH_MAIL} looks up MONTH_MAIL_<MM> in custom_variables
        if upper == "MONTH_MAIL" {
            let month_key = format!("MONTH_MAIL_{:02}", self.base_date.month());
            return self
                .custom_vars
                .get(&month_key)
                .cloned()
                .unwrap_or_default();
        }

        if upper.starts_with("WEEK_") {
            let day_part = &upper[5..];
            if let Ok(d) = day_part.parse::<u32>() {
                return self.resolve_week_day(&format!("{d}"));
            }
            match day_part {
                "MON" | "1" => return self.resolve_week_day("1"),
                "TUE" | "2" => return self.resolve_week_day("2"),
                "WED" | "3" => return self.resolve_week_day("3"),
                "THU" | "4" => return self.resolve_week_day("4"),
                "FRI" | "5" => return self.resolve_week_day("5"),
                "SAT" | "6" => return self.resolve_week_day("6"),
                "SUN" | "7" => return self.resolve_week_day("7"),
                _ => {}
            }
        }

        if upper.starts_with("DAY_OFFSET:") || upper.starts_with("DAY_OFFSET") {
            let rest = upper.strip_prefix("DAY_OFFSET:").unwrap_or(&upper[10..]);
            return self.resolve_day_offset(rest);
        }

        if upper.starts_with("MONTH_OFFSET:") || upper.starts_with("MONTH_OFFSET") {
            let rest = upper.strip_prefix("MONTH_OFFSET:").unwrap_or(&upper[12..]);
            return self.resolve_month_offset(rest);
        }

        match upper.as_str() {
            "E1" | "E2" | "E3" | "E4" | "E5" | "E6" | "E7" | "E8" | "E9" | "EA" | "EB"
            | "EC" | "ED" | "EE" | "EF" | "EG" => self.resolve(&upper, ""),
            "MONTH_FIRST" | "MONTH_LAST" | "PREV_MONTH_FIRST" | "PREV_MONTH_LAST" => {
                self.resolve_month_edge(&upper)
            }
            _ => {
                if trimmed.starts_with('+')
                    || (trimmed.starts_with('-')
                        && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
                {
                    self.resolve_day_offset(trimmed)
                } else if let Some(val) = self.custom_vars.get(&upper) {
                    val.clone()
                } else {
                    format!("{{{token}}}")
                }
            }
        }
    }
}
