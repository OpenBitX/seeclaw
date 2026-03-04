//! Skill registry — two-layer skill system for the agent engine.
//!
//! **Design**: Skills are split into two parts:
//! - **Manifest** (lightweight summary): consumed by the Planner to decide which
//!   skills are needed, without bloating the LLM context.
//! - **Combo** (deterministic action sequence): consumed by ComboExecNode for
//!   zero-LLM execution of well-known action patterns.
//!
//! The Planner only sees manifest summaries (~50 tokens per skill).
//! ComboExecNode loads the full combo definition only for the skill it needs.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── Manifest ───────────────────────────────────────────────────────────────

/// Lightweight skill summary — injected into the Planner's system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Unique skill identifier, e.g. "os/open_software".
    pub name: String,
    /// One-line description of what the skill does.
    pub description: String,
    /// Named parameters the combo accepts, e.g. ["software_name"].
    pub params: Vec<String>,
    /// Trigger phrases that hint when this skill applies.
    pub triggers: String,
}

// ── Combo ──────────────────────────────────────────────────────────────────

/// A deterministic action sequence loaded from a `.combo.json` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboDefinition {
    /// Skill name (must match the manifest).
    pub name: String,
    /// Parameter names (order matches manifest).
    pub params: Vec<String>,
    /// Ordered action steps to execute.
    pub steps: Vec<ComboStep>,
}

/// A single action inside a combo sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboStep {
    /// Action type name, e.g. "hotkey", "wait", "type_text", "key_press".
    pub action: String,
    /// Arguments for the action (may contain `{param}` placeholders).
    pub args: serde_json::Value,
}

// ── Registry ───────────────────────────────────────────────────────────────

/// Central registry holding all loaded skill manifests and combo definitions.
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    manifests: HashMap<String, SkillManifest>,
    combos: HashMap<String, ComboDefinition>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
            combos: HashMap::new(),
        }
    }

    /// Insert a manifest.
    pub fn add_manifest(&mut self, manifest: SkillManifest) {
        self.manifests.insert(manifest.name.clone(), manifest);
    }

    /// Insert a combo.
    pub fn add_combo(&mut self, combo: ComboDefinition) {
        self.combos.insert(combo.name.clone(), combo);
    }

    /// Get a combo definition by skill name.
    pub fn get_combo(&self, name: &str) -> Option<&ComboDefinition> {
        self.combos.get(name)
    }

    /// Get a manifest by skill name.
    pub fn get_manifest(&self, name: &str) -> Option<&SkillManifest> {
        self.manifests.get(name)
    }

    /// List all registered skill names.
    pub fn skill_names(&self) -> Vec<&str> {
        self.manifests.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a combo exists for a given skill name.
    pub fn has_combo(&self, name: &str) -> bool {
        self.combos.contains_key(name)
    }

    /// Generate a compact manifest summary string for the Planner's system prompt.
    ///
    /// This is the **only** skill information the Planner sees — deliberately
    /// minimal to keep token usage low but explicit about HOW to use skills.
    pub fn manifest_summary_for_planner(&self) -> String {
        if self.manifests.is_empty() {
            return String::new();
        }

        let mut out = String::from("# Available Skills (use mode=\"combo\" in plan_task)\n\n");
        out.push_str("When a task matches a skill below, use `mode: \"combo\"` in your plan_task step with the skill name and params. This is the FASTEST and most RELIABLE execution path — zero LLM calls, pre-tested action sequence.\n\n");
        out.push_str("Example: `{\"description\": \"打开软件\", \"mode\": \"combo\", \"skill\": \"open_software\", \"params\": {\"software_name\": \"Chrome\"}}`\n\n");

        for manifest in self.manifests.values() {
            let has_combo = if self.combos.contains_key(&manifest.name) {
                " ✓ combo"
            } else {
                ""
            };
            out.push_str(&format!(
                "- **{}**: {} | params: [{}] | triggers: {}{}\n",
                manifest.name,
                manifest.description,
                manifest.params.join(", "),
                manifest.triggers,
                has_combo,
            ));
        }

        out
    }

    /// Expand a combo's action steps by substituting `{param}` placeholders
    /// with actual values from the provided params map.
    ///
    /// Returns `None` if the skill has no combo definition.
    pub fn expand_combo(
        &self,
        skill_name: &str,
        params: &serde_json::Value,
    ) -> Option<Vec<ComboStep>> {
        let combo = self.combos.get(skill_name)?;

        let steps = combo
            .steps
            .iter()
            .map(|step| {
                let args_str = serde_json::to_string(&step.args).unwrap_or_default();
                let mut expanded = args_str;

                // Replace {param_name} placeholders with actual values
                for param_name in &combo.params {
                    let placeholder = format!("{{{}}}", param_name);
                    if let Some(val) = params.get(param_name) {
                        let replacement = match val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        expanded = expanded.replace(&placeholder, &replacement);
                    }
                }

                ComboStep {
                    action: step.action.clone(),
                    args: serde_json::from_str(&expanded).unwrap_or(step.args.clone()),
                }
            })
            .collect();

        Some(steps)
    }
}
