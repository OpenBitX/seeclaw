//! Skill registry — unified `.skill.json` system for the agent engine.
//!
//! Each skill is a single `.skill.json` file containing both the lightweight
//! summary (name, description, params, triggers) and the deterministic action
//! sequence (steps). The registry loads all of them and provides:
//!
//! - **Summaries for Planner**: compact text with name/description/triggers.
//! - **Combo expansion for ComboExecNode**: zero-LLM execution of action steps.
//! - **Trigger matching for StepRouter**: keyword-based skill detection.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── SkillDefinition — the unified .skill.json format ───────────────────────

/// A complete skill loaded from a single `.skill.json` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    /// Unique skill identifier, e.g. "open_software".
    pub name: String,
    /// One-line description of what the skill does.
    pub description: String,
    /// Named parameters the combo accepts, e.g. ["software_name"].
    pub params: Vec<String>,
    /// Trigger phrases that hint when this skill applies.
    pub triggers: String,
    /// Ordered action steps to execute (the combo sequence).
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

// ── Legacy type aliases (for backward compatibility during migration) ───────

pub type SkillManifest = SkillDefinition;
pub type ComboDefinition = SkillDefinition;

// ── Registry ───────────────────────────────────────────────────────────────

/// Central registry holding all loaded skill definitions.
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Insert a skill definition.
    pub fn add_skill(&mut self, skill: SkillDefinition) {
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Get a skill definition by name.
    pub fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    /// List all registered skill names.
    pub fn skill_names(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a skill exists.
    pub fn has_combo(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Get all skill definitions (for StepRouter trigger matching).
    pub fn all_skills(&self) -> impl Iterator<Item = &SkillDefinition> {
        self.skills.values()
    }

    /// Generate a compact summary string for the Planner's system prompt.
    ///
    /// This is the **only** skill information the Planner sees — deliberately
    /// minimal to keep token usage low. The Planner uses this to recommend
    /// combo mode and specify `required_skills` in its plan output.
    pub fn manifest_summary_for_planner(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut out = String::from("# Available Skills\n\n");
        out.push_str("When a task matches a skill's triggers below, you MUST include it in `required_skills` and recommend `combo` mode.\n\n");

        for skill in self.skills.values() {
            out.push_str(&format!(
                "- **{}**: {} | params: [{}] | triggers: {}\n",
                skill.name,
                skill.description,
                skill.params.join(", "),
                skill.triggers,
            ));
        }

        out
    }

    /// Find skills whose triggers match the given text.
    /// Returns a list of (skill_name, match_score) pairs.
    pub fn match_triggers(&self, text: &str) -> Vec<(String, f32)> {
        let lower = text.to_lowercase();
        let mut matches = Vec::new();

        for skill in self.skills.values() {
            let triggers: Vec<&str> = skill.triggers.split('/').collect();
            let mut score = 0.0f32;

            for trigger in &triggers {
                let t = trigger.trim().to_lowercase();
                if !t.is_empty() && lower.contains(&t) {
                    score += 1.0;
                }
            }

            // Also check if any trigger phrase appears as a substring with + separator
            // e.g. "打开/启动/运行/open/launch/start + 软件名"
            for trigger in &triggers {
                let parts: Vec<&str> = trigger.split('+').collect();
                if parts.len() > 1 {
                    let keyword = parts[0].trim().to_lowercase();
                    if !keyword.is_empty() && lower.contains(&keyword) {
                        score += 0.5;
                    }
                }
            }

            if score > 0.0 {
                matches.push((skill.name.clone(), score));
            }
        }

        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        matches
    }

    /// Expand a skill's action steps by substituting `{param}` placeholders
    /// with actual values from the provided params map.
    ///
    /// Returns `None` if the skill is not found.
    pub fn expand_combo(
        &self,
        skill_name: &str,
        params: &serde_json::Value,
    ) -> Option<Vec<ComboStep>> {
        let skill = self.skills.get(skill_name)?;

        let steps = skill
            .steps
            .iter()
            .map(|step| {
                let args_str = serde_json::to_string(&step.args).unwrap_or_default();
                let mut expanded = args_str;

                // Replace {param_name} placeholders with actual values
                for param_name in &skill.params {
                    let placeholder = format!("{{{}}}", param_name);
                    if let Some(val) = params.get(param_name) {
                        let replacement = match val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        expanded = expanded.replace(&placeholder, &replacement);
                    }
                }

                // Safety check: warn if any {placeholder} remains unexpanded
                if expanded.contains('{') && expanded.contains('}') {
                    tracing::warn!(
                        skill = %skill_name,
                        expanded = %expanded,
                        "expand_combo: unexpanded placeholders remain in combo step"
                    );
                }

                ComboStep {
                    action: step.action.clone(),
                    args: serde_json::from_str(&expanded).unwrap_or(step.args.clone()),
                }
            })
            .collect();

        Some(steps)
    }

    /// Try to extract parameter values from a free-text step description.
    ///
    /// For example, given `open_software` (params: ["software_name"]) and
    /// description "使用系统搜索打开英雄联盟", this extracts the software name
    /// by removing known trigger keywords and taking the remaining text.
    ///
    /// Returns a JSON object with extracted params, or `null`/`{}` if extraction fails.
    pub fn extract_params_from_description(
        &self,
        skill_name: &str,
        description: &str,
    ) -> serde_json::Value {
        let skill = match self.skills.get(skill_name) {
            Some(s) => s,
            None => return serde_json::Value::Null,
        };

        // Only handle simple single-param skills for now
        if skill.params.len() != 1 {
            return serde_json::json!({});
        }

        let param_name = &skill.params[0];

        // Strategy: strip known trigger/action words from the description,
        // whatever remains is likely the parameter value.
        let mut cleaned = description.to_string();

        // Remove trigger keywords
        let trigger_words: Vec<&str> = skill.triggers.split('/').collect();
        for word in &trigger_words {
            let parts: Vec<&str> = word.split('+').collect();
            for part in parts {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    cleaned = cleaned.replace(trimmed, "");
                }
            }
        }

        // Remove common Chinese filler/action words
        let filler = [
            "使用", "通过", "系统搜索", "搜索", "桌面上的", "桌面的", "桌面",
            "帮我", "帮忙", "请", "能不能", "能否", "麻烦",
            "并完成", "并", "完成", "左键", "单击", "右键",
            "上的", "的", "图标", "软件", "应用", "程序",
            "找到", "查找",
        ];
        for f in &filler {
            cleaned = cleaned.replace(f, "");
        }

        let value = cleaned.trim().to_string();

        if value.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ param_name: value })
        }
    }
}