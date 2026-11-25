//! Program Manager View state definitions

pub struct ProgramViewState {
    pub selected_program_id: Option<i64>,
    pub html_import_status: String,
}

impl Default for ProgramViewState {
    fn default() -> Self {
        Self {
            selected_program_id: None,
            html_import_status: String::new(),
        }
    }
}
