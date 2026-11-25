use serde::{Deserialize, Serialize};

use crate::os::memory::Memory;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MemoryInfo {
    pub(crate) total: u64,
    pub(crate) available: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) read_at: Option<String>,
}

impl MemoryInfo {
    pub(crate) fn of(memory: Memory, read_at: Option<String>) -> Self {
        Self { total: memory.total, available: memory.available, read_at }
    }

    pub(crate) fn memory(&self) -> Option<Memory> {
        (self.total > 0).then_some(Memory { total: self.total, available: self.available.min(self.total) })
    }
}

#[cfg(test)]
mod tests;
