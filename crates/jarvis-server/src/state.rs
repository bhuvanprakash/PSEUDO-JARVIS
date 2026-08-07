use std::sync::Arc;

use jarvis_core::config::Config;
use jarvis_core::paths::VaultPaths;
use jarvis_core::Vault;
use jarvis_llm::LlmProvider;
use jarvis_observer::ObserverService;
use reqwest::Client;

pub struct AppState {
    pub vault: Arc<Vault>,
    pub config: Config,
    pub paths: VaultPaths,
    pub llm: Arc<dyn LlmProvider>,
    pub http_client: Client,
    pub observer: tokio::sync::Mutex<ObserverService>,
}

impl AppState {
    pub fn new(
        vault: Arc<Vault>,
        config: Config,
        paths: VaultPaths,
        llm: Arc<dyn LlmProvider>,
    ) -> Self {
        let observer = ObserverService::new(vault.clone(), config.clone());
        Self {
            vault,
            config,
            paths,
            llm,
            http_client: Client::new(),
            observer: tokio::sync::Mutex::new(observer),
        }
    }
}
