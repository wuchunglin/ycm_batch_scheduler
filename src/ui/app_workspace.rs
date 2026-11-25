//! Application Workspace and Navigation state definitions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    ProgramManager,
    GroupEditor,
    ScheduleRule,
    ExecutionRunner,
    Settings,
}

pub struct WorkspaceState {
    pub active_tab: ActiveTab,
    pub status_message: String,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            active_tab: ActiveTab::ProgramManager,
            status_message: "Ready".to_string(),
        }
    }
}
