use thiserror::Error;

#[derive(Debug, Error)]
pub enum JarvisError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("vault error: {0}")]
    Vault(String),
    #[error("playbook error: {0}")]
    Playbook(String),
    #[error("trust denied for step '{step}': {reason}")]
    TrustDenied { step: String, reason: String },
    #[error("execution error: {0}")]
    Execution(String),
    #[error("observer error: {0}")]
    Observer(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, JarvisError>;
