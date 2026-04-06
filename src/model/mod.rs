use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::config::{ClassifierMode, Config};
use crate::domain::MemoryMetadata;
use crate::taxonomy::TaxonomyEvolutionSnapshot;

#[derive(Debug, Clone, Default)]
pub struct ModelGuidance {
    pub main_category: Option<String>,
    pub boosted_labels: Vec<String>,
    pub language_hint: Option<String>,
    pub confidence: f32,
    pub avoid_patterns: Vec<String>,
    pub reinforce_patterns: Vec<String>,
    pub notes: Vec<String>,
    pub backend: String,
}

#[derive(Debug, Clone)]
pub enum ModelHook {
    Algorithm,
    Ollama(OllamaHook),
}

impl ModelHook {
    pub fn from_config(config: &Config) -> Self {
        match config.classifier_mode {
            ClassifierMode::Algorithm => Self::Algorithm,
            ClassifierMode::Ollama => Self::Ollama(OllamaHook {
                endpoint: HttpEndpoint::parse(&config.ollama_base_url)
                    .unwrap_or_else(|| HttpEndpoint::local_default()),
                model: config.ollama_model.clone(),
                timeout: config.ollama_timeout,
            }),
        }
    }

    pub fn classify(
        &self,
        content: &str,
        metadata: &MemoryMetadata,
        evolution: &TaxonomyEvolutionSnapshot,
    ) -> Option<ModelGuidance> {
        match self {
            Self::Algorithm => None,
            Self::Ollama(hook) => hook.classify(content, metadata, evolution),
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Algorithm => "algorithm",
            Self::Ollama(_) => "ollama",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OllamaHook {
    endpoint: HttpEndpoint,
    model: String,
    timeout: Duration,
}

impl OllamaHook {
    fn classify(
        &self,
        content: &str,
        metadata: &MemoryMetadata,
        evolution: &TaxonomyEvolutionSnapshot,
    ) -> Option<ModelGuidance> {
        let request_body = json!({
            "model": self.model,
            "stream": false,
            "format": "json",
            "prompt": build_prompt(content, metadata, evolution),
        });
        let response = self
            .endpoint
            .post_json("/api/generate", &request_body.to_string(), self.timeout)
            .ok()?;
        parse_guidance_response(&response)
    }
}

#[derive(Debug, Clone)]
struct HttpEndpoint {
    host: String,
    port: u16,
    base_path: String,
}

impl HttpEndpoint {
    fn local_default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11434,
            base_path: String::new(),
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        let stripped = raw.strip_prefix("http://")?;
        let mut split = stripped.splitn(2, '/');
        let host_port = split.next()?;
        let base_path = split
            .next()
            .map(|path| format!("/{}", path.trim_matches('/')))
            .unwrap_or_default();
        let mut host_split = host_port.splitn(2, ':');
        let host = host_split.next()?.to_string();
        let port = host_split
            .next()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(80);
        Some(Self {
            host,
            port,
            base_path,
        })
    }

    fn post_json(&self, path: &str, body: &str, timeout: Duration) -> std::io::Result<String> {
        let mut addrs = (self.host.as_str(), self.port).to_socket_addrs()?;
        let addr = addrs.next().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no Ollama address resolved")
        })?;
        let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let request_path = format!("{}{}", self.base_path, path);
        let request = format!(
            "POST {request_path} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            self.host,
            self.port,
            body.len(),
            body
        );
        stream.write_all(request.as_bytes())?;
        stream.flush()?;

        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        let body = response
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or_default()
            .to_string();
        Ok(body)
    }
}

#[derive(Debug, Deserialize)]
struct OllamaEnvelope {
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaGuidance {
    main_category: Option<String>,
    #[serde(default)]
    multi_labels: Vec<String>,
    language_hint: Option<String>,
    confidence: Option<f32>,
    #[serde(default)]
    avoid_patterns: Vec<String>,
    #[serde(default)]
    reinforce_patterns: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
}

fn parse_guidance_response(raw: &str) -> Option<ModelGuidance> {
    let envelope = serde_json::from_str::<OllamaEnvelope>(raw).ok()?;
    let payload = envelope.response.as_deref().unwrap_or(raw);
    let guidance = serde_json::from_str::<OllamaGuidance>(payload).ok()?;
    Some(ModelGuidance {
        main_category: guidance.main_category,
        boosted_labels: guidance.multi_labels,
        language_hint: guidance.language_hint,
        confidence: guidance.confidence.unwrap_or(0.0).clamp(0.0, 1.0),
        avoid_patterns: guidance.avoid_patterns,
        reinforce_patterns: guidance.reinforce_patterns,
        notes: guidance.notes,
        backend: "ollama".to_string(),
    })
}

fn build_prompt(
    content: &str,
    metadata: &MemoryMetadata,
    evolution: &TaxonomyEvolutionSnapshot,
) -> String {
    format!(
        "You are Memovyn_0.1B, a tiny classifier for a coding-agent memory system.\n\
Return strict JSON with keys: main_category, multi_labels, language_hint, confidence, avoid_patterns, reinforce_patterns, notes.\n\
Do not explain. Use concise taxonomy labels when possible.\n\
Known project priors: {}.\n\
Reinforced priors: {}.\n\
Solidified priors: {}.\n\
Avoid patterns: {}.\n\
Metadata language: {}.\n\
Paths: {}.\n\
Content:\n{}",
        evolution.prior_labels.join(", "),
        evolution.reinforced_labels.join(", "),
        evolution.solidified_priors.join(", "),
        evolution.avoid_patterns.join(", "),
        metadata
            .language
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        metadata.paths.join(", "),
        content
    )
}

#[cfg(test)]
mod tests {
    use super::{HttpEndpoint, parse_guidance_response};

    #[test]
    fn parses_http_endpoint() {
        let endpoint = HttpEndpoint::parse("http://127.0.0.1:11434").unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 11434);
    }

    #[test]
    fn parses_ollama_response() {
        let raw = r#"{"response":"{\"main_category\":\"api\",\"multi_labels\":[\"api\",\"security\"],\"language_hint\":\"rust\",\"confidence\":0.82,\"avoid_patterns\":[\"avoid:secret\"],\"reinforce_patterns\":[\"stable\"],\"notes\":[\"model-note\"]}"}"#;
        let guidance = parse_guidance_response(raw).expect("guidance");
        assert_eq!(guidance.main_category.as_deref(), Some("api"));
        assert!(
            guidance
                .boosted_labels
                .iter()
                .any(|label| label == "security")
        );
    }
}
