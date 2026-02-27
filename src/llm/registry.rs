use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::errors::{SeeClawError, SeeClawResult};
use crate::llm::provider::LlmProvider;
use crate::llm::providers::openai_compatible::OpenAiCompatibleProvider;
use crate::llm::types::CallConfig;
use crate::config::LlmConfig;

/// Registry of all available LLM providers, keyed by their config.toml identifier.
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    active: String,
    /// Kept for role-to-model lookups (does not need to be mutable after init).
    llm_config: LlmConfig,
}

impl ProviderRegistry {
    pub fn new(active: String) -> Self {
        Self {
            providers: HashMap::new(),
            active,
            llm_config: LlmConfig::default(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn LlmProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub fn get_active(&self) -> SeeClawResult<Arc<dyn LlmProvider>> {
        self.providers
            .get(&self.active)
            .cloned()
            .ok_or_else(|| SeeClawError::Config(format!("Active provider '{}' not found in registry", self.active)))
    }

    pub fn set_active(&mut self, name: String) -> SeeClawResult<()> {
        if self.providers.contains_key(&name) {
            self.active = name;
            Ok(())
        } else {
            Err(SeeClawError::Config(format!("Provider '{name}' not registered")))
        }
    }

    pub fn list_names(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Return the provider and call configuration for a named agent role.
    ///
    /// Role resolution order:
    /// 1. `[llm.roles.<role>]` in config.toml
    /// 2. Fallback: active provider with its default model / temperature and `stream = true`
    pub fn call_config_for_role(&self, role: &str) -> SeeClawResult<(Arc<dyn LlmProvider>, CallConfig)> {
        let role_entry = match role {
            "routing" => self.llm_config.roles.routing.as_ref(),
            "chat"    => self.llm_config.roles.chat.as_ref(),
            "tools"   => self.llm_config.roles.tools.as_ref(),
            "vision"  => self.llm_config.roles.vision.as_ref(),
            other => {
                tracing::warn!(role = other, "unknown role, falling back to active provider");
                None
            }
        };

        if let Some(entry) = role_entry {
            let provider = self.providers.get(&entry.provider).cloned().ok_or_else(|| {
                SeeClawError::Config(format!(
                    "Role '{}' references unknown provider '{}'",
                    role, entry.provider
                ))
            })?;
            let temperature = entry.temperature.unwrap_or_else(|| {
                self.llm_config
                    .providers
                    .get(&entry.provider)
                    .map(|p| p.temperature)
                    .unwrap_or(0.1)
            });
            tracing::debug!(
                role = role,
                provider = %entry.provider,
                model = %entry.model,
                stream = entry.stream,
                temperature = temperature,
                "resolved role config"
            );
            return Ok((provider, CallConfig {
                model: entry.model.clone(),
                stream: entry.stream,
                temperature,
            }));
        }

        // Fallback: active provider, provider-level defaults
        let provider = self.get_active()?;
        let entry = self.llm_config.providers.get(&self.active);
        let (model, temperature) = entry
            .map(|p| (p.model.clone(), p.temperature))
            .unwrap_or_else(|| (String::new(), 0.1));
        tracing::debug!(
            role = role,
            provider = %self.active,
            model = %model,
            "role not configured, using active provider fallback"
        );
        Ok((provider, CallConfig { model, stream: true, temperature }))
    }

    /// Build a registry from the loaded app config.
    /// API keys are read from environment variables named `SEECLAW_<ID>_API_KEY`.
    pub fn from_config(config: &AppConfig) -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            active: config.llm.active_provider.clone(),
            llm_config: config.llm.clone(),
        };
        for (id, entry) in &config.llm.providers {
            let api_key = std::env::var(format!("SEECLAW_{}_API_KEY", id.to_uppercase()))
                .unwrap_or_else(|_| entry.api_key.clone().unwrap_or_default());
            let provider = OpenAiCompatibleProvider::new(
                id.clone(),
                entry.api_base.clone(),
                api_key,
            );
            registry.register(Arc::new(provider));
        }
        registry
    }
}
