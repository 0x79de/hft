use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    pub okx: OkxConfig,
    pub mcp: McpConfig,
    pub rag: RagConfig,
    pub coordinator: CoordinatorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxConfig {
    pub api_key: String,
    pub secret_key: String,
    pub passphrase: String,
    pub sandbox: bool,
    pub base_url: Option<String>,
    pub timeout_ms: u64,
    pub rate_limit_requests_per_second: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub server_url: String,
    pub api_key: Option<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub prediction_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    pub server_url: String,
    pub api_key: Option<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub query_threshold: f32,
    pub top_k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    pub signal_processing_interval_ms: u64,
    pub health_check_interval_ms: u64,
    pub max_concurrent_requests: usize,
    pub decision_timeout_ms: u64,
    pub consensus_threshold: f64,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            signal_processing_interval_ms: 100,
            health_check_interval_ms: 5000,
            max_concurrent_requests: 100,
            decision_timeout_ms: 50,
            consensus_threshold: 0.7,
        }
    }
}

impl IntegrationConfig {
    pub fn from_env() -> Result<Self> {
        let okx = OkxConfig {
            api_key: env::var("OKX_API_KEY")
                .map_err(|_| anyhow!("OKX_API_KEY environment variable not set"))?,
            secret_key: env::var("OKX_SECRET_KEY")
                .map_err(|_| anyhow!("OKX_SECRET_KEY environment variable not set"))?,
            passphrase: env::var("OKX_PASSPHRASE")
                .map_err(|_| anyhow!("OKX_PASSPHRASE environment variable not set"))?,
            sandbox: env::var("OKX_SANDBOX").unwrap_or_default().parse().unwrap_or(true),
            base_url: env::var("OKX_BASE_URL").ok(),
            timeout_ms: env::var("OKX_TIMEOUT_MS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(5000),
            rate_limit_requests_per_second: env::var("OKX_RATE_LIMIT_RPS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(20),
        };

        let mcp = McpConfig {
            server_url: env::var("MCP_SERVER_URL").unwrap_or_else(|_| "http://localhost:8000".to_string()),
            api_key: env::var("MCP_API_KEY").ok(),
            timeout_ms: env::var("MCP_TIMEOUT_MS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(1000),
            max_retries: env::var("MCP_MAX_RETRIES")
                .unwrap_or_default()
                .parse()
                .unwrap_or(3),
            prediction_threshold: env::var("MCP_PREDICTION_THRESHOLD")
                .unwrap_or_default()
                .parse()
                .unwrap_or(0.7),
        };

        let rag = RagConfig {
            server_url: env::var("RAG_SERVER_URL").unwrap_or_else(|_| "http://localhost:8001".to_string()),
            api_key: env::var("RAG_API_KEY").ok(),
            timeout_ms: env::var("RAG_TIMEOUT_MS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(500),
            max_retries: env::var("RAG_MAX_RETRIES")
                .unwrap_or_default()
                .parse()
                .unwrap_or(2),
            query_threshold: env::var("RAG_QUERY_THRESHOLD")
                .unwrap_or_default()
                .parse()
                .unwrap_or(0.6),
            top_k: env::var("RAG_TOP_K")
                .unwrap_or_default()
                .parse()
                .unwrap_or(10),
        };

        let coordinator = CoordinatorConfig::default();

        Ok(Self {
            okx,
            mcp,
            rag,
            coordinator,
        })
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.okx.api_key.is_empty() {
            return Err(anyhow!("OKX API key is required"));
        }
        if self.okx.secret_key.is_empty() {
            return Err(anyhow!("OKX secret key is required"));
        }
        if self.okx.passphrase.is_empty() {
            return Err(anyhow!("OKX passphrase is required"));
        }
        if self.mcp.server_url.is_empty() {
            return Err(anyhow!("MCP server URL is required"));
        }
        if self.rag.server_url.is_empty() {
            return Err(anyhow!("RAG server URL is required"));
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationEnvironment {
    pub variables: HashMap<String, String>,
}

impl IntegrationEnvironment {
    pub fn new() -> Self {
        let mut variables = HashMap::new();
        
        // Load all environment variables with known prefixes
        for (key, value) in env::vars() {
            if key.starts_with("OKX_") || key.starts_with("MCP_") || key.starts_with("RAG_") {
                variables.insert(key, value);
            }
        }
        
        Self { variables }
    }
    
    pub fn get(&self, key: &str) -> Option<&String> {
        self.variables.get(key)
    }
    
    pub fn set(&mut self, key: String, value: String) {
        self.variables.insert(key, value);
    }
}

impl Default for IntegrationEnvironment {
    fn default() -> Self {
        Self::new()
    }
}