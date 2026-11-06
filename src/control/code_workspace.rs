use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ImportedWorkspace {
    pub(crate) group: Option<u64>,
    pub(crate) name: String,
    pub(crate) projects: Vec<u64>,
    pub(crate) missing: usize,
    pub(crate) failed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) projection: Option<u64>,
}

impl ImportedWorkspace {
    pub(crate) fn whole(&self) -> bool {
        !self.projects.is_empty() && self.missing == 0 && self.failed == 0
    }

    pub(crate) fn message(&self) -> String {
        let imported = match self.projects.len() {
            0 => format!("nothing imported from {}", self.name),
            1 => format!("1 project imported into {}", self.name),
            n => format!("{n} projects imported into {}", self.name),
        };
        let missing = (self.missing > 0).then(|| format!("{} not found", self.missing));
        let failed = (self.failed > 0).then(|| format!("{} could not open, see server.log", self.failed));
        [Some(imported), missing, failed].into_iter().flatten().collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests;
