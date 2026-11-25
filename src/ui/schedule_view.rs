//! Schedule Rule View state definitions

use crate::models::{GeneratedScheduleItem, ScheduleRule};

pub struct ScheduleViewState {
    pub current_rule: ScheduleRule,
    pub preview_items: Vec<GeneratedScheduleItem>,
}

impl Default for ScheduleViewState {
    fn default() -> Self {
        Self {
            current_rule: ScheduleRule::default(),
            preview_items: Vec::new(),
        }
    }
}
