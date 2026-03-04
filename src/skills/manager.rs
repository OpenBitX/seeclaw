use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::skills::registry::{ComboDefinition, SkillManifest, SkillRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub example: String,
    pub rules: Vec<String>,
    pub role: String,
    pub content: String,
    pub enabled: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub category: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    pub enabled_skills: Vec<String>,
    pub skill_settings: HashMap<String, SkillSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSettings {
    pub enabled: bool,
    pub priority: u32,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled_skills: vec![
                "os/open_software".to_string(),
                "os/file_operations".to_string(),
                "web/browser_actions".to_string(),
            ],
            skill_settings: HashMap::new(),
        }
    }
}

pub struct SkillsManager {
    skills: HashMap<String, Skill>,
    config: SkillsConfig,
    skills_dir: String,
}

impl SkillsManager {
    pub fn new(skills_dir: String) -> Self {
        Self {
            skills: HashMap::new(),
            config: SkillsConfig::default(),
            skills_dir,
        }
    }

    pub fn with_config(skills_dir: String, config: SkillsConfig) -> Self {
        Self {
            skills: HashMap::new(),
            config,
            skills_dir,
        }
    }

    pub async fn load_all_skills(&mut self) -> Result<(), String> {
        let skills_dir = self.skills_dir.clone();
        let skills_path = Path::new(&skills_dir);
        
        if !skills_path.exists() {
            tracing::warn!("Skills directory does not exist: {}", skills_dir);
            return Ok(());
        }

        self.load_skills_from_dir(&skills_path).await
    }

    async fn load_skills_from_dir(&mut self, dir: &Path) -> Result<(), String> {
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| format!("Failed to read skills directory: {}", e))?;

        loop {
            match entries.next_entry().await {
                Ok(Some(entry)) => {
                    let path = entry.path();

                    if path.is_dir() {
                        Box::pin(self.load_skills_from_dir(&path)).await?;
                    } else if path.extension().map_or(false, |ext| ext == "md") {
                        if let Err(e) = self.load_skill_file(&path).await {
                            tracing::warn!("Failed to load skill file {:?}: {}", path, e);
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            }
        }

        Ok(())
    }

    async fn load_skill_file(&mut self, path: &Path) -> Result<(), String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read skill file: {}", e))?;

        let skill = self.parse_skill_file(&content, path)?;

        let relative_path = path
            .strip_prefix(&self.skills_dir)
            .map_err(|e| format!("Failed to get relative path: {}", e))?
            .to_string_lossy();

        let skill_name = relative_path
            .trim_start_matches('\\')
            .trim_start_matches('/')
            .replace('\\', "/")
            .trim_end_matches(".md")
            .to_string();

        let skill = Skill {
            name: skill_name.clone(),
            description: skill.description,
            example: skill.example,
            rules: skill.rules,
            role: skill.role,
            content,
            enabled: self.config.enabled_skills.contains(&skill_name),
            category: self.extract_category(&skill_name),
        };

        self.skills.insert(skill_name, skill);
        tracing::info!("Loaded skill: {}", relative_path);
        Ok(())
    }

    fn parse_skill_file(&self, content: &str, path: &Path) -> Result<ParsedSkill, String> {
        let mut name = String::new();
        let mut description = String::new();
        let mut example = String::new();
        let mut rules = Vec::new();
        let mut role = String::new();

        let mut current_section = String::new();
        let mut current_text = String::new();

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("# ") {
                if !current_section.is_empty() {
                    self.process_section(&current_section, &current_text, &mut name, &mut description, &mut example, &mut rules, &mut role);
                }
                current_section = line[2..].to_lowercase();
                current_text.clear();
            } else if line.starts_with("**") && line.ends_with("**") {
                let parts: Vec<&str> = line[2..line.len()-2].splitn(2, ":").collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_lowercase();
                    let value = parts[1].to_string();

                    match key.as_str() {
                        "name" => name = value,
                        "description" => description = value,
                        "example" => example = value,
                        "role" => role = value,
                        _ => {}
                    }
                }
            } else {
                if !current_section.is_empty() {
                    current_text.push_str(line);
                    current_text.push('\n');
                }
            }
        }

        if !current_section.is_empty() {
            self.process_section(&current_section, &current_text, &mut name, &mut description, &mut example, &mut rules, &mut role);
        }

        if name.is_empty() {
            return Err(format!("Skill file {:?} is missing name field", path));
        }

        Ok(ParsedSkill {
            name,
            description,
            example,
            rules,
            role,
        })
    }

    fn process_section(
        &self,
        section: &str,
        text: &str,
        name: &mut String,
        description: &mut String,
        example: &mut String,
        rules: &mut Vec<String>,
        role: &mut String,
    ) {
        match section {
            "metadata" => {
                for line in text.lines() {
                    let line = line.trim();
                    if line.starts_with("**") && line.ends_with("**") {
                        let parts: Vec<&str> = line[2..line.len()-2].splitn(2, ":").collect();
                        if parts.len() == 2 {
                            let key = parts[0].trim().to_lowercase();
                            let value = parts[1].to_string();

                            match key.as_str() {
                                "name" => *name = value,
                                "description" => *description = value,
                                "example" => *example = value,
                                "role" => *role = value,
                                _ => {}
                            }
                        }
                    }
                }
            }
            "rules" => {
                *rules = text
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect();
            }
            _ => {}
        }
    }

    fn extract_category(&self, skill_name: &str) -> String {
        skill_name
            .split('/')
            .next()
            .unwrap_or("general")
            .to_string()
    }

    pub fn get_skill(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn get_enabled_skills(&self) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| s.enabled)
            .collect()
    }

    pub fn get_skills_by_category(&self, category: &str) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| s.category == category)
            .collect()
    }

    pub fn get_all_metadata(&self) -> Vec<SkillMetadata> {
        self.skills
            .values()
            .map(|s| SkillMetadata {
                name: s.name.clone(),
                description: s.description.clone(),
                category: s.category.clone(),
                enabled: s.enabled,
            })
            .collect()
    }

    pub fn enable_skill(&mut self, name: &str) {
        if let Some(skill) = self.skills.get_mut(name) {
            skill.enabled = true;
            if !self.config.enabled_skills.contains(&name.to_string()) {
                self.config.enabled_skills.push(name.to_string());
            }
        }
    }

    pub fn disable_skill(&mut self, name: &str) {
        if let Some(skill) = self.skills.get_mut(name) {
            skill.enabled = false;
            self.config.enabled_skills.retain(|s| s != name);
        }
    }

    pub fn get_skills_context_for_planner(&self, _goal: &str) -> String {
        let enabled_skills = self.get_enabled_skills();
        
        if enabled_skills.is_empty() {
            return String::new();
        }

        let mut context = String::from("# Available Skills\n\n");
        context.push_str("The following skills are available to help accomplish the task:\n\n");

        for skill in enabled_skills {
            context.push_str(&format!("## {}\n", skill.name));
            context.push_str(&format!("**Description**: {}\n", skill.description));
            context.push_str(&format!("**When to use**: {}\n", skill.role));
            
            if !skill.rules.is_empty() {
                context.push_str("**Rules**:\n");
                for rule in &skill.rules {
                    context.push_str(&format!("- {}\n", rule));
                }
            }
            
            if !skill.example.is_empty() {
                context.push_str(&format!("**Example**: {}\n", skill.example));
            }
            
            context.push('\n');
        }

        context
    }

    pub fn get_config(&self) -> &SkillsConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: SkillsConfig) {
        let enabled_skills = config.enabled_skills.clone();
        self.config = config;
        for skill_name in &enabled_skills {
            if let Some(skill) = self.skills.get_mut(skill_name) {
                skill.enabled = true;
            }
        }
    }
}

// ── Registry builder ───────────────────────────────────────────────────────

/// Load a `SkillRegistry` from the skills directory.
///
/// Scans for `*.manifest.md` and `*.combo.json` files and populates the
/// registry. This is the new entry point that replaces the old full-content
/// approach.
pub async fn load_skill_registry(skills_dir: &str) -> SkillRegistry {
    let mut registry = SkillRegistry::new();
    let dir = Path::new(skills_dir);

    if !dir.exists() {
        tracing::warn!("Skills directory does not exist: {}", skills_dir);
        return registry;
    }

    if let Err(e) = scan_skill_dir(dir, dir, &mut registry).await {
        tracing::warn!(error = %e, "Failed to scan skill directory");
    }

    tracing::info!(
        manifests = registry.skill_names().len(),
        "Skill registry loaded"
    );
    registry
}

/// Recursively scan a directory for manifest and combo files.
async fn scan_skill_dir(
    root: &Path,
    dir: &Path,
    registry: &mut SkillRegistry,
) -> Result<(), String> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("read_dir failed: {e}"))?;

    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let path = entry.path();
                if path.is_dir() {
                    Box::pin(scan_skill_dir(root, &path, registry)).await?;
                } else if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                    if fname.ends_with(".manifest.md") {
                        if let Some(m) = parse_manifest_file(&path, root).await {
                            tracing::debug!(name = %m.name, "loaded manifest");
                            registry.add_manifest(m);
                        }
                    } else if fname.ends_with(".combo.json") {
                        if let Some(c) = parse_combo_file(&path).await {
                            tracing::debug!(name = %c.name, "loaded combo");
                            registry.add_combo(c);
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read dir entry");
                continue;
            }
        }
    }
    Ok(())
}

/// Parse a `.manifest.md` file into a `SkillManifest`.
///
/// Expected format (key: value per line):
/// ```text
/// name: open_software
/// description: ...
/// params: [software_name]
/// triggers: ...
/// ```
async fn parse_manifest_file(path: &Path, root: &Path) -> Option<SkillManifest> {
    let content = tokio::fs::read_to_string(path).await.ok()?;

    let mut name = String::new();
    let mut description = String::new();
    let mut params = Vec::new();
    let mut triggers = String::new();

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("description:") {
            description = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("params:") {
            // Parse [a, b, c] format
            let val = val.trim().trim_start_matches('[').trim_end_matches(']');
            params = val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        } else if let Some(val) = line.strip_prefix("triggers:") {
            triggers = val.trim().to_string();
        }
    }

    // Derive skill name from file path if not specified
    if name.is_empty() {
        let rel = path.strip_prefix(root).ok()?;
        name = rel
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches(".manifest.md")
            .to_string();
    }

    Some(SkillManifest {
        name,
        description,
        params,
        triggers,
    })
}

/// Parse a `.combo.json` file into a `ComboDefinition`.
async fn parse_combo_file(path: &Path) -> Option<ComboDefinition> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    match serde_json::from_str::<ComboDefinition>(&content) {
        Ok(combo) => Some(combo),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to parse combo file");
            None
        }
    }
}

struct ParsedSkill {
    name: String,
    description: String,
    example: String,
    rules: Vec<String>,
    role: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_skills() {
        let mut manager = SkillsManager::new("prompts/skills".to_string());
        let result = manager.load_all_skills().await;
        assert!(result.is_ok());
    }
}
