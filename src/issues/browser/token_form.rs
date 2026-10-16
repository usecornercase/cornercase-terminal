use super::{Action, Browser, Step, Tab, TokenForm};
use crate::issues::{Secret, Source, gitlab, jira, plane};

const TOKEN_DOTS: usize = 40;

impl Browser {
    pub(crate) fn token_field(&self, source: Source) -> (&'static str, String) {
        let form = self.forms.get(&source);
        let step = self.form_step(source);
        let typed = form.map_or_else(
            || match step {
                Step::Site => self.jira_site.clone(),
                Step::Workspace => self.plane_workspace.clone(),
                Step::Server => self.gitlab_url.clone(),
                _ => String::new(),
            },
            |f| f.input.clone(),
        );
        match step {
            Step::Site => ("site", typed),
            Step::Email => ("email", typed),
            Step::Workspace => ("workspace", typed),
            Step::Server => ("server", typed),
            Step::Token => (source.token_name(), "•".repeat(typed.chars().count().min(TOKEN_DOTS))),
        }
    }

    pub(crate) fn token_form(&self) -> Option<Source> {
        match self.current() {
            Tab::One(source) if source != Source::Github && !self.connections.contains_key(&source) => Some(source),
            _ => None,
        }
    }

    pub(super) fn form(&mut self, source: Source) -> &mut TokenForm {
        let (site, workspace, url) = (self.jira_site.clone(), self.plane_workspace.clone(), self.gitlab_url.clone());
        self.forms.entry(source).or_insert_with(|| match source {
            Source::Jira => TokenForm { input: site, step: Step::Site, ..TokenForm::default() },
            Source::Plane => TokenForm { input: workspace, step: Step::Workspace, ..TokenForm::default() },
            Source::Gitlab => TokenForm { input: url, step: Step::Server, ..TokenForm::default() },
            _ => TokenForm::default(),
        })
    }

    pub(crate) fn form_step(&self, source: Source) -> Step {
        match (self.forms.get(&source), source) {
            (Some(form), _) => form.step,
            (None, Source::Jira) => Step::Site,
            (None, Source::Plane) => Step::Workspace,
            (None, Source::Gitlab) => Step::Server,
            (None, _) => Step::Token,
        }
    }

    pub(crate) fn token_rejected(&mut self, source: Source, error: String) {
        let form = self.form(source);
        form.checking = false;
        form.error = Some(error);
    }

    pub(super) fn submit_token(&mut self) -> Action {
        let Some(source) = self.token_form() else { return Action::None };
        let email = self.jira_email.clone();
        let form = self.form(source);
        let input = form.input.trim().to_string();
        if form.checking {
            return Action::None;
        }
        match form.step {
            Step::Site => match jira::check_site(&input) {
                Ok(site) => {
                    *form = TokenForm { input: email, step: Step::Email, ..TokenForm::default() };
                    self.jira_site = site;
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Email => match jira::check_email(&input) {
                Ok(email) => {
                    *form = TokenForm::default();
                    self.jira_email.clone_from(&email);
                    return Action::SaveJira { site: self.jira_site.clone(), email };
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Workspace => match plane::check_workspace(&input) {
                Ok(workspace) => {
                    *form = TokenForm::default();
                    self.plane_workspace.clone_from(&workspace);
                    return Action::SavePlane { workspace };
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Server => match gitlab::check_url(&input) {
                Ok(url) => {
                    *form = TokenForm::default();
                    self.gitlab_url.clone_from(&url);
                    return Action::SaveGitlab { url };
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Token if input.is_empty() => {}
            Step::Token => {
                form.checking = true;
                form.error = None;
                return Action::CheckToken(source, Secret(input));
            }
        }
        Action::None
    }

    pub(super) fn step_back(&mut self) {
        let Some(source) = self.token_form() else { return };
        let (site, email, workspace) = (self.jira_site.clone(), self.jira_email.clone(), self.plane_workspace.clone());
        let url = self.gitlab_url.clone();
        let form = self.form(source);
        if form.checking {
            return;
        }
        *form = match (form.step, source) {
            (Step::Token, Source::Gitlab) | (Step::Server, _) => {
                TokenForm { input: url, step: Step::Server, ..TokenForm::default() }
            }
            (Step::Token, Source::Plane) | (Step::Workspace, _) => {
                TokenForm { input: workspace, step: Step::Workspace, ..TokenForm::default() }
            }
            (Step::Token, _) => TokenForm { input: email, step: Step::Email, ..TokenForm::default() },
            (Step::Email | Step::Site, _) => TokenForm { input: site, step: Step::Site, ..TokenForm::default() },
        };
    }
}
