use crate::list::Combo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentAccounts {
    pub(crate) kind: String,
    names: Vec<String>,
    pub(crate) combo: Combo,
    missing: Option<String>,
}

impl AgentAccounts {
    pub(crate) fn new(kind: String, accounts: &[String], labels: Vec<String>, active: &str) -> Self {
        let names: Vec<String> = std::iter::once(String::new()).chain(accounts.iter().cloned()).collect();
        let chosen = names.iter().position(|name| name == active).unwrap_or(0);
        Self { kind, names, combo: Combo::new(labels, Some(chosen)), missing: None }
    }

    pub(crate) fn keeping(mut self, saved: Option<&str>) -> Self {
        match saved {
            Some(name) if !self.names.iter().any(|n| n == name) => {
                self.combo = Combo::new(self.combo.options().to_vec(), None);
                self.missing = Some(name.to_string());
            }
            Some(name) => self.choose_named(name),
            None => {}
        }
        self
    }

    pub(crate) fn chosen(&self) -> &str {
        match self.combo.chosen() {
            Some(i) => self.names.get(i).map_or("", String::as_str),
            None => self.missing.as_deref().unwrap_or_default(),
        }
    }

    pub(crate) fn missing(&self) -> Option<&str> {
        self.missing.as_deref().filter(|_| self.combo.chosen().is_none())
    }

    pub(crate) fn choose_named(&mut self, name: &str) {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            self.combo.choose(i);
        }
    }
}

#[cfg(test)]
mod tests;
