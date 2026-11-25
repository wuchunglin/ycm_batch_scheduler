use std::collections::BTreeSet;
use chrono::{Datelike, NaiveDate};

use crate::engine::param_resolver::RocDateUtils;
use crate::models::{DateRangeMode, GeneratedScheduleItem, ScheduleMode, ScheduleRule};

/// Schedule Generator expanding Figure 2 schedule definitions into concrete execution dates.
pub struct ScheduleGenerator;

impl ScheduleGenerator {
    pub fn generate(rule: &ScheduleRule) -> Vec<GeneratedScheduleItem> {
        let mut target_dates = BTreeSet::new();

        let (range_start, range_end) = match &rule.range_mode {
            DateRangeMode::AllYear => (None, None),
            DateRangeMode::CustomRange { start, end } => {
                let s = RocDateUtils::parse_roc_date(start);
                let e = RocDateUtils::parse_roc_date(end);
                (s, e)
            }
        };

        match &rule.schedule_mode {
            ScheduleMode::Monthly { days, last_day } => {
                for &roc_year in &rule.years {
                    let greg_year = roc_year + 1911;
                    for month in 1..=12 {
                        let max_day = RocDateUtils::days_in_month(greg_year, month);
                        for &d in days {
                            if d >= 1 && d <= max_day {
                                if let Some(date) = NaiveDate::from_ymd_opt(greg_year, month, d) {
                                    target_dates.insert(date);
                                }
                            }
                        }
                        if *last_day {
                            if let Some(date) = NaiveDate::from_ymd_opt(greg_year, month, max_day) {
                                target_dates.insert(date);
                            }
                        }
                    }
                }
            }

            ScheduleMode::Weekly { weekdays } => {
                for &roc_year in &rule.years {
                    let greg_year = roc_year + 1911;
                    let Some(start_date) = NaiveDate::from_ymd_opt(greg_year, 1, 1) else {
                        continue;
                    };
                    let Some(end_date) = NaiveDate::from_ymd_opt(greg_year, 12, 31) else {
                        continue;
                    };

                    let mut curr = start_date;
                    while curr <= end_date {
                        let weekday_num = curr.weekday().number_from_monday();
                        if weekdays.contains(&weekday_num) {
                            target_dates.insert(curr);
                        }
                        if let Some(next) = curr.succ_opt() {
                            curr = next;
                        } else {
                            break;
                        }
                    }
                }
            }

            ScheduleMode::SpecificDates { dates } => {
                for d_str in dates {
                    if let Some(date) = RocDateUtils::parse_roc_date(d_str) {
                        let roc_year = date.year() - 1911;
                        if rule.years.is_empty() || rule.years.contains(&roc_year) {
                            target_dates.insert(date);
                        }
                    }
                }
            }

            ScheduleMode::SingleRun { date } => {
                if let Some(d_str) = date {
                    if let Some(parsed) = RocDateUtils::parse_roc_date(d_str) {
                        target_dates.insert(parsed);
                    }
                } else {
                    target_dates.insert(chrono::Local::now().date_naive());
                }
            }
        }

        target_dates
            .into_iter()
            .filter(|date| {
                if let Some(start) = range_start {
                    if *date < start {
                        return false;
                    }
                }
                if let Some(end) = range_end {
                    if *date > end {
                        return false;
                    }
                }
                true
            })
            .map(|date| {
                let roc_date = RocDateUtils::to_roc_yyymmdd(&date, "/");
                let roc_date_raw = RocDateUtils::to_roc_yyymmdd(&date, "");
                GeneratedScheduleItem {
                    roc_date,
                    roc_date_raw,
                    exe_time: rule.default_time.clone(),
                }
            })
            .collect()
    }
}
