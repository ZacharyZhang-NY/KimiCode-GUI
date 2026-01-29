use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Direct API mode - requires api_key
    Api,
    /// CLI mode - uses local kimi CLI installation
    Cli,
}

impl Default for AuthMode {
    fn default() -> Self {
        AuthMode::Api
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub cli_path: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::default(),
            api_key: None,
            api_base: None,
            cli_path: None,
        }
    }
}

impl AuthConfig {
    pub fn is_configured(&self) -> bool {
        match self.mode {
            AuthMode::Api => self.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false),
            AuthMode::Cli => true, // Will check CLI availability at runtime
        }
    }
    
    pub fn effective_api_base(&self) -> String {
        self.api_base
            .as_ref()
            .filter(|b| !b.is_empty())
            .cloned()
            .unwrap_or_else(|| "https://api.moonshot.cn/v1".to_string())
    }
}
