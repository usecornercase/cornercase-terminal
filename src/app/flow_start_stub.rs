use ratatui::layout::Rect;

use super::App;
use crate::flows::Flow;

impl App {
    #[expect(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        reason = "a stand-in until the backend's start_run is merged"
    )]
    pub(super) fn start_run(
        &mut self,
        project_id: u64,
        flow: Flow,
        task: String,
        branch: String,
        focus: bool,
        area: Rect,
    ) -> Result<(), String> {
        let _ = (project_id, flow, task, branch, focus, area);
        Ok(())
    }
}
