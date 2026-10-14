use std::path::Path;

use super::App;
use crate::flows::{builtin, store};
use crate::paths;

impl App {
    #[allow(dead_code)]
    pub(super) fn flow_listing(&self, project: Option<&Path>) -> store::Listing {
        let project = project.map(store::project_dir);
        store::list(builtin::for_config(&self.config), &paths::flows(&self.config_path), project.as_deref())
    }
}
