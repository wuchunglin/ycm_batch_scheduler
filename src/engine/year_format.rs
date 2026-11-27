//! 年度格式：欄位的日期參數要輸出「民國年」或「西元年」。
//!
//! 年度格式由「批次程式新增」的每個欄位各自設定（預設民國年），
//! 執行批次與預覽時，解析器依欄位的年度格式輸出：
//!
//! | 規則 | 民國年（預設） | 西元年 |
//! |------|---------------|--------|
//! | 年月日（E1~E8…） | `1151127` | `20261127` |
//! | 年月（ED~EG…）   | `11511`   | `202611`   |
//! | 年度（PREV_YEAR）| `115`     | `2026`     |

use chrono::{Datelike, NaiveDate};

/// ROC 紀元與西元的差距
const ROC_OFFSET: i32 = 1911;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum YearFormat {
    /// 民國年（預設）：年度 3 位數
    #[default]
    Roc,
    /// 西元年：年度 4 位數
    Ad,
}

impl YearFormat {
    /// 資料庫 / 畫面的 `year_ad` 旗標 → 年度格式（false = 民國年）
    pub fn from_ad(ad: bool) -> Self {
        if ad {
            YearFormat::Ad
        } else {
            YearFormat::Roc
        }
    }

    pub fn is_ad(self) -> bool {
        self == YearFormat::Ad
    }

    pub fn label(self) -> &'static str {
        match self {
            YearFormat::Roc => "民國年",
            YearFormat::Ad => "西元年",
        }
    }

    /// 年度：民國 3 位數（115）、西元 4 位數（2026）。`year` 為西元年。
    pub fn year(self, year: i32) -> String {
        match self {
            YearFormat::Roc => format!("{:03}", year - ROC_OFFSET),
            YearFormat::Ad => format!("{:04}", year),
        }
    }

    /// 年月日，`sep` 為分隔符（可為空字串）。`year` 為西元年。
    pub fn ymd(self, year: i32, month: u32, day: u32, sep: &str) -> String {
        format!("{}{sep}{:02}{sep}{:02}", self.year(year), month, day)
    }

    /// 年月，`sep` 為分隔符（可為空字串）。`year` 為西元年。
    pub fn ym(self, year: i32, month: u32, sep: &str) -> String {
        format!("{}{sep}{:02}", self.year(year), month)
    }

    pub fn date_ymd(self, date: &NaiveDate, sep: &str) -> String {
        self.ymd(date.year(), date.month(), date.day(), sep)
    }

    pub fn date_ym(self, date: &NaiveDate, sep: &str) -> String {
        self.ym(date.year(), date.month(), sep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roc_and_ad_formats() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        assert_eq!(YearFormat::Roc.date_ymd(&d, ""), "1151127");
        assert_eq!(YearFormat::Ad.date_ymd(&d, ""), "20261127");
        assert_eq!(YearFormat::Roc.date_ymd(&d, "/"), "115/11/27");
        assert_eq!(YearFormat::Ad.date_ymd(&d, "/"), "2026/11/27");
        assert_eq!(YearFormat::Roc.date_ym(&d, ""), "11511");
        assert_eq!(YearFormat::Ad.date_ym(&d, ""), "202611");
        assert_eq!(YearFormat::Roc.year(2024), "113");
        assert_eq!(YearFormat::Ad.year(2024), "2024");
        assert_eq!(YearFormat::default(), YearFormat::Roc);
    }
}
