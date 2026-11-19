use super::super::App;
use super::super::toast::Toast;
use crate::config::Config;
use crate::log;
use crate::machines;
use crate::ui;

pub(in crate::app) fn change_shown(config: &mut Config, name: Option<&str>, shown: bool) -> Option<Vec<String>> {
    let changed = machines::set_shown(&mut config.machines, name, shown)?;
    let what = if shown { "machine shown" } else { "machine hidden" };
    match (name, changed.as_slice()) {
        (_, []) => {}
        (Some(_), [one]) => log::info!("machines", what, name = one),
        (_, every) => log::info!("machines", what, count = every.len()),
    }
    Some(changed)
}

fn notice(name: Option<&str>, shown: bool, outcome: &Result<Option<Vec<String>>, String>) -> String {
    match (outcome, name) {
        (Err(why), _) => why.clone(),
        (Ok(None), name) => format!("{} is no longer on the list", name.unwrap_or_default()),
        (Ok(Some(_)), Some(name)) if shown => format!("{name} shown here"),
        (Ok(Some(_)), Some(name)) => format!("{name} hidden: its server keeps running there"),
        (Ok(Some(_)), None) if shown => "every machine shown here".into(),
        (Ok(Some(_)), None) => "every machine hidden: their servers keep running there".into(),
    }
}

impl App {
    fn flip_shown(&mut self, name: Option<&str>, shown: bool) -> Result<Option<Vec<String>>, String> {
        let mut changed = None;
        self.change_config(|config| changed = change_shown(config, name, shown))?;
        self.follow_machines();
        Ok(changed)
    }

    pub(in crate::app) fn show_machine_row(&mut self, at: usize, shown: bool) {
        let Some(name) = self.config.machines.get(at).map(|machine| machine.name.clone()) else { return };
        let outcome = self.flip_shown(Some(&name), shown);
        self.settings_message(notice(Some(&name), shown, &outcome));
    }

    pub(super) fn show_from_menu(&mut self, name: Option<&str>, shown: bool) {
        let outcome = self.flip_shown(name, shown);
        let icon = if matches!(outcome, Ok(Some(_))) { ui::ToastIcon::Check } else { ui::ToastIcon::Bug };
        self.messages.toast = Some(Toast::new(notice(name, shown, &outcome), icon));
    }
}
