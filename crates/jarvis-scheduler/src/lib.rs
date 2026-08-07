use std::future::Future;

use tokio_cron_scheduler::{Job, JobScheduler};

pub struct PlaybookScheduler {
    scheduler: JobScheduler,
}

impl PlaybookScheduler {
    pub async fn new() -> jarvis_core::Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Other(e.to_string()))?;
        Ok(Self { scheduler })
    }

    pub async fn add<F, Fut>(&mut self, cron: &str, f: F) -> jarvis_core::Result<()>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let job = Job::new_async(cron, move |_uuid, _l| {
            let fut = f();
            Box::pin(async move {
                fut.await;
            })
        })
        .map_err(|e| jarvis_core::error::JarvisError::Other(e.to_string()))?;
        self.scheduler
            .add(job)
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Other(e.to_string()))?;
        Ok(())
    }

    pub async fn start(self) -> jarvis_core::Result<()> {
        self.scheduler
            .start()
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Other(e.to_string()))?;
        Ok(())
    }
}
