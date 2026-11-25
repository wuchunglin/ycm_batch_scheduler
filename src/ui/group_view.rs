//! Group Editor View state definitions

pub struct GroupViewState {
    pub selected_group_id: Option<i64>,
    pub selected_item_id: Option<i64>,
}

impl Default for GroupViewState {
    fn default() -> Self {
        Self {
            selected_group_id: None,
            selected_item_id: None,
        }
    }
}
