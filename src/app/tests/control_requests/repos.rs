use std::sync::Arc;

use parking_lot::Mutex;

use super::*;
use crate::test_util::git_repo;

const SHOP: &str = "[remote \"origin\"]\n\turl = gh-work:acme/web-shop.git\n";

struct Resolving {
    app: App,
    rx: Receiver<AppEvent>,
    asked: Arc<Mutex<Vec<String>>>,
    repos: Vec<TempDir>,
}

impl Resolving {
    fn new(configs: &[&str]) -> Self {
        let (mut app, rx) = empty_app();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        app.machines.repos.set_resolver(Arc::new(move |host: &str| {
            seen.lock().push(host.to_string());
            (host == "gh-work").then(|| "github.com".to_string())
        }));
        let repos: Vec<TempDir> = configs.iter().map(|config| repo_with(config)).collect();
        for repo in &repos {
            app.open_project(repo.path().to_path_buf(), AREA).expect("open the project");
        }
        Self { app, rx, asked, repos }
    }

    fn identities(&self) -> Vec<Option<String>> {
        self.app.projects.iter().map(|p| p.repo.identity.clone()).collect()
    }

    fn wait_for(&mut self, wanted: &[Option<&str>], at: Instant) {
        let wanted: Vec<Option<String>> = wanted.iter().map(|w| w.map(str::to_string)).collect();
        wait_until("the identities", || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(at);
            self.identities() == wanted
        });
    }
}

fn repo_with(config: &str) -> TempDir {
    let repo = git_repo(&[]);
    std::fs::write(repo.path().join(".git/config"), config).expect("write config");
    repo
}

#[test]
fn each_project_reads_its_identity_resolving_a_host_once() {
    let mut rig = Resolving::new(&[SHOP, "[remote \"origin\"]\n\turl = gh-work:acme/blog\n", "[core]\n"]);

    rig.wait_for(&[Some("github.com/acme/web-shop"), Some("github.com/acme/blog"), None], Instant::now());

    assert_eq!(*rig.asked.lock(), ["gh-work"]);
    let remotes = &rig.app.projects[0].repo.remotes;
    assert_eq!(*remotes, [("origin".to_string(), "gh-work:acme/web-shop.git".to_string())]);
}

#[test]
fn status_names_each_project_s_remotes_without_credentials_and_its_identity() {
    let token = "[remote \"origin\"]\n\turl = https://ana:ghp_s3cret@github.com/me/web-shop.git\n\
                 [remote \"upstream\"]\n\turl = gh-work:acme/web-shop.git\n";
    let mut rig = Resolving::new(&[token, "[core]\n"]);
    rig.wait_for(&[Some("github.com/acme/web-shop"), None], Instant::now());

    let status = value(&mut rig.app, None, Command::Status(wire::Status {}));

    let remotes = json!([
        {"name": "origin", "url": "https://github.com/me/web-shop.git"},
        {"name": "upstream", "url": "gh-work:acme/web-shop.git"},
    ]);
    assert_eq!(status["projects"][0]["remotes"], remotes);
    assert_eq!(status["projects"][0]["repo"], json!("github.com/acme/web-shop"));
    let bare = status["projects"][1].as_object().expect("a project");
    assert!(!bare.contains_key("remotes") && !bare.contains_key("repo"), "{bare:?}");
    assert!(!status.to_string().contains("s3cret"), "{status}");
}

#[test]
fn a_status_request_never_resolves_a_host() {
    let mut rig = Resolving::new(&[SHOP]);
    rig.wait_for(&[Some("github.com/acme/web-shop")], Instant::now());
    rig.asked.lock().clear();

    value(&mut rig.app, None, Command::Status(wire::Status {}));

    assert!(rig.asked.lock().is_empty());
}

#[test]
fn the_identity_follows_a_changed_remote_on_a_later_refresh_without_resolving_again() {
    let mut rig = Resolving::new(&[SHOP]);
    let start = Instant::now();
    rig.wait_for(&[Some("github.com/acme/web-shop")], start);
    let moved = "[remote \"origin\"]\n\turl = gh-work:acme/shop-next.git\n";
    std::fs::write(rig.repos[0].path().join(".git/config"), moved).expect("change the remote");

    rig.wait_for(&[Some("github.com/acme/shop-next")], start + Duration::from_mins(1));

    assert_eq!(*rig.asked.lock(), ["gh-work"]);
}
