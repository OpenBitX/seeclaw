use std::collections::HashMap;
use std::sync::Arc;

use crate::errors::{SeeClawError, SeeClawResult};
use crate::llm::provider::LlmProvider;

/// Registry of all available LLM providers, keyed by their config.toml identifier.
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    active: String,
}

impl ProviderRegistry {
    pub fn new(active: String) -> Self {
        Self {
            providers: HashMap::new(),
            active,
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
}
