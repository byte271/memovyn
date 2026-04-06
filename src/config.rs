use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifierMode {
    Algorithm,
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgettingPolicy {
    Off,
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub classifier_mode: ClassifierMode,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout: Duration,
    pub forgetting_policy: ForgettingPolicy,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = env::var_os("MEMOVYN_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".memovyn"));
        let database_path = env::var_os("MEMOVYN_DATABASE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("memovyn.sqlite3"));
        let classifier_mode = parse_classifier_mode(env::var("MEMOVYN_CLASSIFIER_MODE").ok());
        let ollama_base_url = env::var("MEMOVYN_OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        let ollama_model =
            env::var("MEMOVYN_OLLAMA_MODEL").unwrap_or_else(|_| "memovyn_0.1b".to_string());
        let ollama_timeout = Duration::from_millis(
            env::var("MEMOVYN_OLLAMA_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(350),
        );
        let forgetting_policy = parse_forgetting_policy(env::var("MEMOVYN_FORGETTING_POLICY").ok());
        Self {
            data_dir,
            database_path,
            classifier_mode,
            ollama_base_url,
            ollama_model,
            ollama_timeout,
            forgetting_policy,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        if let Some(parent) = self.database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}

fn parse_classifier_mode(value: Option<String>) -> ClassifierMode {
    match value
        .as_deref()
        .map(str::trim)
        .unwrap_or("algorithm")
        .to_ascii_lowercase()
        .as_str()
    {
        "ollama" | "hybrid" => ClassifierMode::Ollama,
        _ => ClassifierMode::Algorithm,
    }
}

fn parse_forgetting_policy(value: Option<String>) -> ForgettingPolicy {
    match value
        .as_deref()
        .map(str::trim)
        .unwrap_or("balanced")
        .to_ascii_lowercase()
        .as_str()
    {
        "off" => ForgettingPolicy::Off,
        "conservative" => ForgettingPolicy::Conservative,
        "aggressive" => ForgettingPolicy::Aggressive,
        _ => ForgettingPolicy::Balanced,
    }
}

#[cfg(test)]
mod tests {
    use super::{ClassifierMode, ForgettingPolicy, parse_classifier_mode, parse_forgetting_policy};

    #[test]
    fn parses_classifier_mode() {
        assert_eq!(
            parse_classifier_mode(Some("ollama".to_string())),
            ClassifierMode::Ollama
        );
        assert_eq!(
            parse_classifier_mode(Some("algorithm".to_string())),
            ClassifierMode::Algorithm
        );
    }

    #[test]
    fn parses_forgetting_policy() {
        assert_eq!(
            parse_forgetting_policy(Some("aggressive".to_string())),
            ForgettingPolicy::Aggressive
        );
        assert_eq!(
            parse_forgetting_policy(Some("off".to_string())),
            ForgettingPolicy::Off
        );
    }
}
