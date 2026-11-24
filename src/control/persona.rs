use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::flows::persona::Persona;

pub(crate) const CHECK_SHORT: &str =
    "personas are prompt text that a flow's agents read and follow without asking for permission";
pub(crate) const CHECK: &str =
    "personas are prompt text that a flow's agents read and follow without asking for permission: read them first";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaList {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaImport {
    pub(crate) personas: Vec<PersonaText>,
    pub(crate) replace: bool,
    pub(crate) check: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) catalogue: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaText {
    pub(crate) file: String,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaRemove {
    pub(crate) id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_field_names, reason = "the answer's JSON key is `personas`")]
pub(crate) struct Personas {
    pub(crate) dir: PathBuf,
    pub(crate) personas: Vec<PersonaInfo>,
    pub(crate) broken: Vec<BrokenPersona>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) emoji: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    pub(crate) bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<PathBuf>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) replaced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) problem: Option<String>,
}

impl From<&Persona> for PersonaInfo {
    fn from(persona: &Persona) -> Self {
        Self {
            id: persona.id.clone(),
            name: persona.name.clone(),
            description: persona.description.clone(),
            emoji: persona.emoji.clone(),
            color: persona.color.clone(),
            source: persona.source.clone(),
            bytes: persona.text.len(),
            path: None,
            replaced: false,
            file: None,
            problem: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BrokenPersona {
    pub(crate) path: PathBuf,
    pub(crate) error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonasImported {
    pub(crate) personas: Vec<PersonaInfo>,
    pub(crate) check: String,
    pub(crate) saved: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaRemoved {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaSave {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) replace: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaShow {
    pub(crate) id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaShown {
    pub(crate) persona: PersonaInfo,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonaSaved {
    pub(crate) persona: PersonaInfo,
}

impl PersonaImport {
    pub(super) fn fields(&self, fields: &mut Fields) {
        let bytes: usize = self.personas.iter().map(|persona| persona.text.len()).sum();
        fields.add("count", self.personas.len()).add("text_bytes", bytes);
        fields.add("replace", self.replace).add("check", self.check).add("catalogue", self.catalogue);
    }
}

impl PersonaSave {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("persona", &self.id).add("text_bytes", self.text.len()).add("replace", self.replace);
    }
}

impl PersonaShow {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("persona", &self.id);
    }
}

impl PersonaRemove {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("persona", &self.id);
    }
}

#[cfg(test)]
mod tests;
