use std::collections::HashMap;
use chrono::{Datelike, Duration, NaiveDate};

use super::year_format::YearFormat;

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
///
/// 日期類規則的年度格式由呼叫端以 [`YearFormat`] 指定（來自欄位設定）：
/// `resolve` / `resolve_template` 一律輸出民國年；`resolve_with` / `resolve_template_with`
/// 依指定的年度格式輸出民國年（如 `1151127`）或西元年（如 `20261127`）。
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

    /// 以民國年輸出（沒有欄位年度格式可參照時使用，例如試算、步驟日期）。
    pub fn resolve(&self, rule_code: &str, rule_value: &str) -> String {
        self.resolve_with(rule_code, rule_value, YearFormat::Roc)
    }

    /// Resolve a parameter based on rule code and value/template, using the field's year format.
    pub fn resolve_with(&self, rule_code: &str, rule_value: &str, fmt: YearFormat) -> String {
        let code = rule_code.trim().to_uppercase();
        let base = self.base_date;
        match code.as_str() {
            // Figure 1 Standard Codes (E1 ~ EG)
            "E1" => fmt.date_ymd(&base, ""),
            "E2" => fmt.date_ymd(&(base - Duration::days(1)), ""),
            "E3" => fmt.date_ymd(&(base - Duration::days(2)), ""),
            "E4" => fmt.date_ymd(&(base - Duration::days(3)), ""),
            "E5" => fmt.date_ymd(&(base - Duration::days(4)), ""),
            "E6" => fmt.date_ymd(&(base - Duration::days(5)), ""),
            "E7" => fmt.date_ymd(&(base - Duration::days(6)), ""),
            "E8" => fmt.date_ymd(&(base - Duration::days(7)), ""),

            // 只有月份：不含年度，與年度格式無關
            "E9" => format!("{:02}", base.month()),
            "EA" => format!("{:02}", RocDateUtils::shift_months(base.year(), base.month(), -1).1),
            "EB" => format!("{:02}", RocDateUtils::shift_months(base.year(), base.month(), -2).1),
            "EC" => format!("{:02}", RocDateUtils::shift_months(base.year(), base.month(), -3).1),

            // 年月
            "ED" => fmt.date_ym(&base, ""),
            "EE" => self.year_month_shifted(-1, fmt),
            "EF" => self.year_month_shifted(-2, fmt),
            "EG" => self.year_month_shifted(-3, fmt),

            // Extended Rules
            "DAY_OFFSET" => self.resolve_day_offset(rule_value, fmt),
            "MONTH_OFFSET" => self.resolve_month_offset(rule_value, fmt),
            "WEEK_DAY" | "WEEK" => self.resolve_week_day_with(rule_value, fmt),
            "MONTH_EDGE" => self.resolve_month_edge(rule_value, fmt),
            "DATE_EDGE" => self.resolve_date_edge(rule_value, fmt),
            "LAST_YEAR_MONTH" => self.resolve_last_year_month(rule_value, fmt),
            "PREV_YEAR" => self.resolve_prev_year(rule_value, fmt),
            "STATIC" => rule_value.to_string(),
            "TEMPLATE" | "CUSTOM_TEMPLATE" => self.resolve_template_with(rule_value, fmt),

            _ => {
                if rule_value.contains('{') && rule_value.contains('}') {
                    self.resolve_template_with(rule_value, fmt)
                } else if !rule_value.is_empty() {
                    rule_value.to_string()
                } else {
                    self.resolve_token(&code, fmt)
                }
            }
        }
    }

    /// 執行日往前（或往後）`delta` 個月的年月
    fn year_month_shifted(&self, delta: i32, fmt: YearFormat) -> String {
        let (y, m) = RocDateUtils::shift_months(self.base_date.year(), self.base_date.month(), delta);
        fmt.ym(y, m, "")
    }

    /// 年度類規則的數字部分。舊版本曾把年度格式存成 `<數字>:AD` 後綴，
    /// 年度格式現在改由欄位設定，所以這裡只取冒號前的數字，後綴一律忽略。
    fn year_rule_number(value: &str) -> &str {
        value.split(':').next().unwrap_or("").trim()
    }

    /// 執行日去年月份：值 `1`~`12`。例：執行日 116 年、值 3 → 11503（西元 202503）
    fn resolve_last_year_month(&self, value: &str, fmt: YearFormat) -> String {
        match Self::year_rule_number(value).parse::<u32>() {
            Ok(m) if (1..=12).contains(&m) => {
                format!("{}{:02}", fmt.year(self.base_date.year() - 1), m)
            }
            _ => value.to_string(),
        }
    }

    /// 執行日以前年度：值 N，N=1 去年度、2 前年度…；N=0 為本年度。
    /// 例：執行日 115 年、值 2 → 113（西元 2024）
    fn resolve_prev_year(&self, value: &str, fmt: YearFormat) -> String {
        match Self::year_rule_number(value).parse::<i32>() {
            Ok(back) if back >= 0 => fmt.year(self.base_date.year() - back),
            _ => value.to_string(),
        }
    }

    /// Day offset: e.g. "+2", "-10", or "DAY_OFFSET:+2"
    fn resolve_day_offset(&self, value: &str, fmt: YearFormat) -> String {
        let val = value
            .trim()
            .strip_prefix("DAY_OFFSET:")
            .unwrap_or(value)
            .trim();
        let offset: i64 = val.trim_start_matches('+').parse().unwrap_or(0);
        let target = self.base_date + Duration::days(offset);
        fmt.date_ymd(&target, "")
    }

    /// Month offset: e.g. "+1", "-2", or with format "+1:MM", "-1:YYYMM"
    fn resolve_month_offset(&self, value: &str, fmt: YearFormat) -> String {
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
            fmt.ym(y, m, "")
        }
    }

    /// Weekday: 1 (Mon) .. 7 (Sun)，以民國年輸出
    pub fn resolve_week_day(&self, value: &str) -> String {
        self.resolve_week_day_with(value, YearFormat::Roc)
    }

    /// Weekday: 1 (Mon) .. 7 (Sun)，依年度格式輸出
    pub fn resolve_week_day_with(&self, value: &str, fmt: YearFormat) -> String {
        let val = value.trim().strip_prefix("WEEK_").unwrap_or(value).trim();
        let parts: Vec<&str> = val.split(':').collect();
        let day_num: u32 = parts[0].parse().unwrap_or(1);
        let sep = parts.get(1).copied().unwrap_or("");

        let days_from_monday = self.base_date.weekday().num_days_from_monday() as i64;
        let monday = self.base_date - Duration::days(days_from_monday);
        let target = monday + Duration::days((day_num.clamp(1, 7) - 1) as i64);

        fmt.date_ymd(&target, sep)
    }

    /// Month edge: MONTH_FIRST, MONTH_LAST, PREV_MONTH_FIRST, PREV_MONTH_LAST
    fn resolve_month_edge(&self, value: &str, fmt: YearFormat) -> String {
        let edge = value.trim().to_uppercase();
        let (y0, m0) = (self.base_date.year(), self.base_date.month());
        match edge.as_str() {
            "MONTH_FIRST" => fmt.ymd(y0, m0, 1, ""),
            "MONTH_LAST" => fmt.ymd(y0, m0, RocDateUtils::days_in_month(y0, m0), ""),
            "PREV_MONTH_FIRST" => {
                let (y, m) = RocDateUtils::shift_months(y0, m0, -1);
                fmt.ymd(y, m, 1, "")
            }
            "PREV_MONTH_LAST" => {
                let (y, m) = RocDateUtils::shift_months(y0, m0, -1);
                fmt.ymd(y, m, RocDateUtils::days_in_month(y, m), "")
            }
            _ => value.to_string(),
        }
    }

    /// Calendar period edges. Value format: `KEY` or `KEY:<year offset>`,
    /// KEY = PREV_QUARTER_FIRST | PREV_QUARTER_LAST | PREV_HALF_FIRST | PREV_HALF_LAST
    ///     | YEAR_FIRST | YEAR_LAST.  The year offset (e.g. -4) shifts the base year first.
    fn resolve_date_edge(&self, value: &str, fmt: YearFormat) -> String {
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
            fmt.ymd(y, start_m, 1, "")
        } else {
            fmt.ymd(y, end_m, RocDateUtils::days_in_month(y, end_m), "")
        }
    }

    /// Template interpolation，以民國年輸出（沒有欄位年度格式可參照時使用，例如收件者 mail）
    pub fn resolve_template(&self, template: &str) -> String {
        self.resolve_template_with(template, YearFormat::Roc)
    }

    /// Template interpolation replacing `{TOKEN}` with dynamic value, using the field's year format
    pub fn resolve_template_with(&self, template: &str, fmt: YearFormat) -> String {
        let mut result = template.to_string();
        let mut search_from = 0;
        while let Some(rel_start) = result[search_from..].find('{') {
            let start_idx = search_from + rel_start;
            if let Some(end_rel) = result[start_idx..].find('}') {
                let end_idx = start_idx + end_rel;
                let token = &result[start_idx + 1..end_idx];
                let resolved = self.resolve_token(token, fmt);
                let resolved_len = resolved.len();
                result.replace_range(start_idx..=end_idx, &resolved);
                search_from = start_idx + resolved_len;
            } else {
                break;
            }
        }
        result
    }

    /// 中文別名：{執行日當天}、{執行日前1天}、{執行日後3天}、{執行日前月}、{執行日前月第1日} ...
    /// 名稱與「預設輸入值」下拉選單一致（可省略括號內的範例）。
    fn resolve_alias(&self, token: &str, fmt: YearFormat) -> Option<String> {
        let t: String = token.chars().filter(|c| !c.is_whitespace()).collect();
        let key = t.strip_prefix("執行日")?;
        let v = match key {
            "" | "當天" => self.resolve_with("E1", "", fmt),
            "當月" => self.resolve_with("ED", "", fmt),
            "前月" => self.resolve_with("EE", "", fmt),
            "前月第1日" => self.resolve_month_edge("PREV_MONTH_FIRST", fmt),
            "前月最後1日" => self.resolve_month_edge("PREV_MONTH_LAST", fmt),
            "當月第1日" | "本月第1日" => self.resolve_month_edge("MONTH_FIRST", fmt),
            "當月最後1日" | "本月最後1日" => self.resolve_month_edge("MONTH_LAST", fmt),
            "前1季第1日" => self.resolve_date_edge("PREV_QUARTER_FIRST", fmt),
            "前1季最後1日" => self.resolve_date_edge("PREV_QUARTER_LAST", fmt),
            "前半年第1日" => self.resolve_date_edge("PREV_HALF_FIRST", fmt),
            "前半年最後1日" => self.resolve_date_edge("PREV_HALF_LAST", fmt),
            "本年第1日" => self.resolve_date_edge("YEAR_FIRST", fmt),
            "本年最後1日" => self.resolve_date_edge("YEAR_LAST", fmt),
            _ => {
                let (sign, rest) = if let Some(r) = key.strip_prefix("前") {
                    ("-", r)
                } else if let Some(r) = key.strip_prefix("後") {
                    ("+", r)
                } else {
                    return None;
                };
                let n: i64 = rest.strip_suffix("天")?.parse().ok()?;
                self.resolve_day_offset(&format!("{sign}{n}"), fmt)
            }
        };
        Some(v)
    }

    /// Resolves an individual token inside `{...}`
    fn resolve_token(&self, token: &str, fmt: YearFormat) -> String {
        let trimmed = token.trim();
        let upper = trimmed.to_uppercase();

        if let Some(v) = self.resolve_alias(trimmed, fmt) {
            return v;
        }

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
                return self.resolve_week_day_with(&format!("{d}"), fmt);
            }
            match day_part {
                "MON" | "1" => return self.resolve_week_day_with("1", fmt),
                "TUE" | "2" => return self.resolve_week_day_with("2", fmt),
                "WED" | "3" => return self.resolve_week_day_with("3", fmt),
                "THU" | "4" => return self.resolve_week_day_with("4", fmt),
                "FRI" | "5" => return self.resolve_week_day_with("5", fmt),
                "SAT" | "6" => return self.resolve_week_day_with("6", fmt),
                "SUN" | "7" => return self.resolve_week_day_with("7", fmt),
                _ => {}
            }
        }

        if upper.starts_with("DAY_OFFSET:") || upper.starts_with("DAY_OFFSET") {
            let rest = upper.strip_prefix("DAY_OFFSET:").unwrap_or(&upper[10..]);
            return self.resolve_day_offset(rest, fmt);
        }

        if upper.starts_with("MONTH_OFFSET:") || upper.starts_with("MONTH_OFFSET") {
            let rest = upper.strip_prefix("MONTH_OFFSET:").unwrap_or(&upper[12..]);
            return self.resolve_month_offset(rest, fmt);
        }

        match upper.as_str() {
            "E1" | "E2" | "E3" | "E4" | "E5" | "E6" | "E7" | "E8" | "E9" | "EA" | "EB"
            | "EC" | "ED" | "EE" | "EF" | "EG" => self.resolve_with(&upper, "", fmt),
            "MONTH_FIRST" | "MONTH_LAST" | "PREV_MONTH_FIRST" | "PREV_MONTH_LAST" => {
                self.resolve_month_edge(&upper, fmt)
            }
            _ => {
                if trimmed.starts_with('+')
                    || (trimmed.starts_with('-')
                        && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
                {
                    self.resolve_day_offset(trimmed, fmt)
                } else if let Some(val) = self.custom_vars.get(&upper) {
                    val.clone()
                } else {
                    format!("{{{token}}}")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> ParamResolver {
        // 執行日 2026-11-27 = 民國 115 年 11 月 27 日（星期五）
        ParamResolver::new(NaiveDate::from_ymd_opt(2026, 11, 27).unwrap())
    }

    const ROC: YearFormat = YearFormat::Roc;
    const AD: YearFormat = YearFormat::Ad;

    #[test]
    fn e_codes_follow_year_format() {
        let r = r();
        assert_eq!(r.resolve_with("E1", "", ROC), "1151127");
        assert_eq!(r.resolve_with("E1", "", AD), "20261127");
        assert_eq!(r.resolve_with("E2", "", ROC), "1151126");
        assert_eq!(r.resolve_with("E8", "", AD), "20261120");
        assert_eq!(r.resolve_with("ED", "", ROC), "11511");
        assert_eq!(r.resolve_with("ED", "", AD), "202611");
        assert_eq!(r.resolve_with("EE", "", ROC), "11510");
        assert_eq!(r.resolve_with("EG", "", AD), "202608");
        // 只有月份：不受年度格式影響
        assert_eq!(r.resolve_with("E9", "", ROC), "11");
        assert_eq!(r.resolve_with("E9", "", AD), "11");
        assert_eq!(r.resolve_with("EA", "", AD), "10");
        // 預設（沒指定年度格式）= 民國年
        assert_eq!(r.resolve("E1", ""), "1151127");
    }

    #[test]
    fn e_codes_cross_year_boundary() {
        let r = ParamResolver::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        assert_eq!(r.resolve_with("EE", "", ROC), "11412");
        assert_eq!(r.resolve_with("EE", "", AD), "202512");
        assert_eq!(r.resolve_with("E8", "", AD), "20251229");
    }

    #[test]
    fn extended_rules_follow_year_format() {
        let r = r();
        assert_eq!(r.resolve_with("DAY_OFFSET", "+8", ROC), "1151205");
        assert_eq!(r.resolve_with("DAY_OFFSET", "+8", AD), "20261205");
        assert_eq!(r.resolve_with("DAY_OFFSET", "-3", AD), "20261124");
        assert_eq!(r.resolve_with("MONTH_OFFSET", "-1", ROC), "11510");
        assert_eq!(r.resolve_with("MONTH_OFFSET", "-1", AD), "202610");
        assert_eq!(r.resolve_with("MONTH_OFFSET", "-1:MM", AD), "10");
        assert_eq!(r.resolve_with("MONTH_EDGE", "MONTH_FIRST", ROC), "1151101");
        assert_eq!(r.resolve_with("MONTH_EDGE", "MONTH_LAST", AD), "20261130");
        assert_eq!(r.resolve_with("MONTH_EDGE", "PREV_MONTH_LAST", AD), "20261031");
        assert_eq!(r.resolve_with("DATE_EDGE", "PREV_QUARTER_FIRST", ROC), "1150701");
        assert_eq!(r.resolve_with("DATE_EDGE", "PREV_QUARTER_LAST", AD), "20260930");
        assert_eq!(r.resolve_with("DATE_EDGE", "YEAR_LAST", AD), "20261231");
        assert_eq!(r.resolve_with("DATE_EDGE", "PREV_QUARTER_FIRST:-4", ROC), "1110701");
        assert_eq!(r.resolve_with("DATE_EDGE", "PREV_QUARTER_FIRST:-4", AD), "20220701");
        // 本週星期五 = 執行日當天；分隔符保留
        assert_eq!(r.resolve_with("WEEK_DAY", "5", AD), "20261127");
        assert_eq!(r.resolve_with("WEEK_DAY", "5:/", ROC), "115/11/27");
        assert_eq!(r.resolve_with("WEEK_DAY", "5:/", AD), "2026/11/27");
    }

    #[test]
    fn year_rules_follow_year_format_and_ignore_legacy_ad_suffix() {
        let r = r();
        assert_eq!(r.resolve_with("PREV_YEAR", "2", ROC), "113");
        assert_eq!(r.resolve_with("PREV_YEAR", "2", AD), "2024");
        assert_eq!(r.resolve_with("PREV_YEAR", "0", AD), "2026");
        assert_eq!(r.resolve_with("LAST_YEAR_MONTH", "3", ROC), "11403");
        assert_eq!(r.resolve_with("LAST_YEAR_MONTH", "3", AD), "202503");
        // 舊版 `:AD` 後綴不再影響結果（年度格式只看欄位設定）
        assert_eq!(r.resolve_with("PREV_YEAR", "2:AD", ROC), "113");
        assert_eq!(r.resolve_with("LAST_YEAR_MONTH", "3:AD", ROC), "11403");
    }

    #[test]
    fn templates_and_aliases_follow_year_format() {
        let r = r();
        assert_eq!(r.resolve_template_with("DAVDF222.{E1}", ROC), "DAVDF222.1151127");
        assert_eq!(r.resolve_template_with("DAVDF222.{E1}", AD), "DAVDF222.20261127");
        assert_eq!(r.resolve_template_with("{執行日當天}", AD), "20261127");
        assert_eq!(r.resolve_template_with("{執行日前1天}", AD), "20261126");
        assert_eq!(r.resolve_template_with("{執行日後3天}", ROC), "1151130");
        assert_eq!(r.resolve_template_with("{執行日前月}", AD), "202610");
        assert_eq!(r.resolve_template_with("{執行日前月第1日}", AD), "20261001");
        assert_eq!(r.resolve_template_with("{執行日本年最後1日}", AD), "20261231");
        assert_eq!(r.resolve_template_with("{DAY_OFFSET:+1}", AD), "20261128");
        assert_eq!(r.resolve_template_with("{MONTH_FIRST}-{MONTH_LAST}", AD), "20261101-20261130");
        assert_eq!(r.resolve_template_with("{WEEK_5}", AD), "20261127");
        assert_eq!(r.resolve_template_with("{+2}", AD), "20261129");
        // 未知變數原樣保留；STATIC 不受影響
        assert_eq!(r.resolve_template_with("{UNKNOWN}", AD), "{UNKNOWN}");
        assert_eq!(r.resolve_with("STATIC", "ABC", AD), "ABC");
        // 舊介面 = 民國年
        assert_eq!(r.resolve_template("{E1}"), "1151127");
        assert_eq!(r.resolve_week_day("5"), "1151127");
    }

    #[test]
    fn custom_template_rule_code() {
        let r = r();
        assert_eq!(r.resolve_with("CUSTOM_TEMPLATE", "X{E1}Y{ED}", AD), "X20261127Y202611");
        assert_eq!(r.resolve_with("CUSTOM_TEMPLATE", "X{E1}Y{ED}", ROC), "X1151127Y11511");
    }
}
