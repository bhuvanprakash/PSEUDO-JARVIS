use std::sync::Arc;

use jarvis_scheduler::PlaybookScheduler;
use tracing::info;

use crate::orchestrator::{Orchestrator, RunOptions};
use crate::state::AppState;

pub async fn start_scheduler(state: Arc<AppState>) -> jarvis_core::Result<()> {
    if !state.config.scheduler.enabled {
        return Ok(());
    }
    let playbooks = state.vault.sync_playbooks()?;
    let mut scheduler = PlaybookScheduler::new().await?;
    for pb in playbooks {
        if let Some(cron) = &pb.doc.spec.triggers.schedule {
            let playbook_id = pb.id().to_string();
            let state_clone = state.clone();
            scheduler
                .add(cron, move || {
                    let state = state_clone.clone();
                    let playbook_id = playbook_id.clone();
                    async move {
                        info!("scheduled run for {playbook_id}");
                        let _ = Orchestrator::run(
                            &state,
                            RunOptions {
                                playbook_id: Some(playbook_id),
                                phrase: None,
                                trigger: "schedule".into(),
                                schedule_triggered: true,
                                auto_approve: false,
                            },
                            None,
                        )
                        .await;
                    }
                })
                .await?;
        }
    }
    scheduler.start().await?;
    Ok(())
}
