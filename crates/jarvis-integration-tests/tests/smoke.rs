use std::sync::Arc;

use jarvis_core::paths::VaultPaths;
use jarvis_core::Vault;
use jarvis_llm::ollama::create_provider;
use jarvis_server::orchestrator::{Orchestrator, RunOptions};
use jarvis_server::AppState;

#[tokio::test]
async fn runs_hello_world_playbook() {
    let temp = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(temp.path().join(".jarvis"));
    paths.ensure_layout().unwrap();

    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../playbooks/community/hello-world.yaml");
    let dest = paths.playbooks().join("hello-world.yaml");
    std::fs::copy(src, dest).unwrap();

    let config = jarvis_core::Config::default();
    config.save(&paths.config()).unwrap();

    let vault = Arc::new(Vault::open(paths.clone()).unwrap());
    vault.sync_playbooks().unwrap();
    let llm: Arc<dyn jarvis_llm::LlmProvider> =
        Arc::from(create_provider(false, "http://127.0.0.1:11434"));
    let state = AppState::new(vault, config, paths.clone(), llm);

    let run_id = Orchestrator::run(
        &state,
        RunOptions {
            playbook_id: Some("hello-world".into()),
            phrase: None,
            trigger: "test".into(),
            schedule_triggered: false,
            auto_approve: true,
        },
        None,
    )
    .await
    .expect("playbook should run");

    let run = state.vault.get_run(&run_id).unwrap();
    assert_eq!(run.status, "completed");
    assert!(paths.run_replay_dir(&run_id).join("manifest.json").exists());
}
