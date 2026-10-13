use std::path::{Path, PathBuf};

use crate::test_util::{Sleeper, TempDir, fixture, write_executable};

pub(crate) struct FakeOpencode {
    pub(crate) dir: TempDir,
    pub(crate) script: PathBuf,
    pub(crate) database: PathBuf,
}

impl FakeOpencode {
    pub(crate) const SESSION: &str = "ses_fake";

    pub(crate) fn new() -> Self {
        let dir = TempDir::new();
        let data = dir.path().join("data/opencode");
        let cache = dir.path().join("cache/opencode");
        for folder in [&data, &cache] {
            std::fs::create_dir_all(folder).expect("create opencode folders");
        }
        let database = data.join("opencode.db");
        rusqlite::Connection::open(&database)
            .and_then(|db| {
                db.execute_batch(fixture!("opencode/1.18.35/schema.sql"))?;
                db.execute(
                    "INSERT INTO project (id, worktree, time_created, time_updated, sandboxes) \
                     VALUES ('global', '/', 0, 0, '[]')",
                    [],
                )
            })
            .expect("create the database");
        std::fs::write(cache.join("models.json"), fixture!("opencode/1.18.35/models.json")).expect("write models.json");
        let script = dir.path().join("bin/opencode");
        std::fs::create_dir_all(script.parent().expect("bin folder")).expect("create bin folder");
        write_executable(
            &script,
            "#!/bin/sh\nexec 3< \"$1\"\nwhile [ -d \"$2\" ] && [ ! -e \"$2/quit\" ]; do sleep 0.02; done\n",
        );
        Self { dir, script, database }
    }

    pub(crate) fn command_line(&self) -> String {
        let quote = |path: &Path| crate::agents::quote(&path.to_string_lossy());
        format!(
            "env XDG_CACHE_HOME={} {} {} {}",
            quote(&self.dir.path().join("cache")),
            quote(&self.script),
            quote(&self.database),
            quote(self.dir.path()),
        )
    }

    pub(crate) fn start(&self, cwd: &Path) -> Sleeper {
        let cache = self.dir.path().join("cache");
        Sleeper::holding(&self.script, cwd, &[("XDG_CACHE_HOME", &cache.to_string_lossy())], &self.database)
    }

    pub(crate) fn write(&self, session: &str, cwd: &Path, messages: &str) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("time").as_millis();
        let now = i64::try_from(now).expect("milliseconds");
        let db = rusqlite::Connection::open(&self.database).expect("open the database");
        db.execute(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated) \
             VALUES (?1, 'global', 'fake', ?2, 'fake', '1.18.35', ?3, ?3) \
             ON CONFLICT(id) DO UPDATE SET directory = excluded.directory, time_updated = excluded.time_updated",
            rusqlite::params![session, cwd.to_string_lossy(), now],
        )
        .expect("write the session");
        for data in messages.lines().filter(|line| !line.trim().is_empty()) {
            let (count, last): (i64, i64) = db
                .query_row("SELECT COUNT(*), COALESCE(MAX(time_created), 0) FROM message", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .expect("count messages");
            db.execute(
                "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?3, ?4)",
                rusqlite::params![format!("msg_{count:06}"), session, now.max(last + 1), data],
            )
            .expect("write a message");
        }
    }

    pub(crate) fn parts(&self, session: &str, message: &str, parts: &str) {
        let db = rusqlite::Connection::open(&self.database).expect("open the database");
        for data in parts.lines().filter(|line| !line.trim().is_empty()) {
            let count: i64 = db.query_row("SELECT COUNT(*) FROM part", [], |r| r.get(0)).expect("count parts");
            db.execute(
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
                 VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                rusqlite::params![format!("prt_{count:06}"), message, session, count, data],
            )
            .expect("write a part");
        }
    }

    pub(crate) fn quit(&self) {
        std::fs::write(self.dir.path().join("quit"), "").expect("signal fake opencode");
    }
}

impl Drop for FakeOpencode {
    fn drop(&mut self) {
        self.quit();
    }
}
