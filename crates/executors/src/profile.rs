use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::executors::{BaseCodingAgent, CodingAgent, claude::ClaudeCode};

/// Global cache for executor configs - always contains only ClaudeCode
static EXECUTOR_CONFIGS_CACHE: OnceLock<ExecutorConfigs> = OnceLock::new();

#[derive(Error, Debug)]
pub enum ProfileError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("No available executor profile")]
    NoAvailableExecutorProfile,
}

/// Executor configurations - simplified to only support ClaudeCode
/// Kept for backward compatibility with existing API routes
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutorConfigs {
    /// Map of executor types to their configurations - always contains only ClaudeCode
    pub executors: HashMap<BaseCodingAgent, CodingAgent>,
}

impl Default for ExecutorConfigs {
    fn default() -> Self {
        Self::from_defaults()
    }
}

impl ExecutorConfigs {
    /// Create executor configs from defaults - always ClaudeCode only
    pub fn from_defaults() -> Self {
        let mut executors = HashMap::new();
        executors.insert(
            BaseCodingAgent::ClaudeCode,
            CodingAgent::ClaudeCode(ClaudeCode::default()),
        );
        Self { executors }
    }

    /// Get cached executor configs - always returns ClaudeCode config
    pub fn get_cached() -> Self {
        EXECUTOR_CONFIGS_CACHE
            .get_or_init(Self::from_defaults)
            .clone()
    }

    /// Reload the cached configs - no-op since we only support ClaudeCode
    pub fn reload() {
        // No-op: we always use default ClaudeCode config
        tracing::debug!("ExecutorConfigs::reload() called - using default ClaudeCode config");
    }

    /// Get a coding agent for the given profile - always returns ClaudeCode
    pub fn get_coding_agent(&self, _profile_id: &ExecutorProfileId) -> Option<CodingAgent> {
        Some(CodingAgent::ClaudeCode(ClaudeCode::default()))
    }

    /// Get a coding agent for the given profile, or return default - always returns ClaudeCode
    pub fn get_coding_agent_or_default(&self, _profile_id: &ExecutorProfileId) -> CodingAgent {
        CodingAgent::ClaudeCode(ClaudeCode::default())
    }

    /// Get the recommended executor profile - always returns ClaudeCode
    pub fn get_recommended_executor_profile(&self) -> Option<ExecutorProfileId> {
        Some(ExecutorProfileId::default())
    }

    /// Save profile overrides - no-op since we only support ClaudeCode
    pub fn save_overrides(&self) -> Result<(), ProfileError> {
        // No-op: we don't support custom profiles anymore
        tracing::debug!("ExecutorConfigs::save_overrides() called - ignored, using default ClaudeCode config");
        Ok(())
    }
}

// Executor-centric profile identifier
// Kept for backward compatibility with existing database records
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, Hash, Eq)]
pub struct ExecutorProfileId {
    /// The executor type - always CLAUDE_CODE
    #[serde(alias = "profile", deserialize_with = "de_base_coding_agent_kebab")]
    pub executor: BaseCodingAgent,
    /// Variant name - ignored, always uses default Claude Code config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

// Convert legacy profile/executor names from kebab-case to SCREAMING_SNAKE_CASE
fn de_base_coding_agent_kebab<'de, D>(de: D) -> Result<BaseCodingAgent, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(de)?;
    // kebab-case -> SCREAMING_SNAKE_CASE
    let norm = raw.replace('-', "_").to_ascii_uppercase();
    // Always return CLAUDE_CODE regardless of input for backward compatibility
    BaseCodingAgent::from_str(&norm).or(Ok(BaseCodingAgent::ClaudeCode))
}

impl Default for ExecutorProfileId {
    fn default() -> Self {
        Self {
            executor: BaseCodingAgent::ClaudeCode,
            variant: None,
        }
    }
}

impl ExecutorProfileId {
    /// Create a new executor profile ID - always returns ClaudeCode
    pub fn new(_executor: BaseCodingAgent) -> Self {
        Self {
            executor: BaseCodingAgent::ClaudeCode,
            variant: None,
        }
    }

    /// Create a new executor profile ID with specific variant - variant is ignored
    pub fn with_variant(_executor: BaseCodingAgent, _variant: String) -> Self {
        Self {
            executor: BaseCodingAgent::ClaudeCode,
            variant: None,
        }
    }

    /// Get cache key for this executor profile
    pub fn cache_key(&self) -> String {
        "CLAUDE_CODE".to_string()
    }
}

impl std::fmt::Display for ExecutorProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CLAUDE_CODE")
    }
}

/// Get the default CodingAgent - always returns ClaudeCode with default settings
pub fn get_default_coding_agent() -> CodingAgent {
    CodingAgent::ClaudeCode(ClaudeCode::default())
}

/// Get CodingAgent for profile - always returns ClaudeCode regardless of input
pub fn get_coding_agent(_executor_profile_id: &ExecutorProfileId) -> CodingAgent {
    get_default_coding_agent()
}

/// Get recommended executor profile - always returns ClaudeCode
pub fn get_recommended_executor_profile() -> ExecutorProfileId {
    ExecutorProfileId::default()
}

pub fn to_default_variant(id: &ExecutorProfileId) -> ExecutorProfileId {
    ExecutorProfileId {
        executor: id.executor,
        variant: None,
    }
}
