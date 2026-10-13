use std::thread;

use crate::app::App;
use crate::app::event::AppEvent;
use crate::error::Result;
use crate::log::Job;
use crate::panics;

impl App {
    pub(super) fn spawn_job<T: Send + 'static>(
        &self,
        job: Job,
        work: impl FnOnce() -> Result<T> + Send + 'static,
        answer: impl FnOnce(Result<T>, &mut Job) -> Option<AppEvent> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let mut job = job.begin();
        thread::spawn(move || {
            let result = panics::job(work);
            let outcome = result.as_ref().map(|_| ()).map_err(ToString::to_string);
            let event = answer(result, &mut job);
            job.finish(&outcome);
            if let Some(event) = event {
                let _ = tx.send(event);
            }
        });
    }
}
