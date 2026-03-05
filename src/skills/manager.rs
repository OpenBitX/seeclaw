use std::path::Path;

use crate::skills::registry::{SkillDefinition, SkillRegistry};

// ── Registry builder ───────────────────────────────────────────────────────

/// Load a `SkillRegistry` from the skills directory.
///
/// Scans for `*.skill.json` files and populates the registry.
/// Each file is a unified skill definition containing both metadata and combo steps.
pub async fn load_skill_registry(skills_dir: &str) -> SkillRegistry {
    let mut registry = SkillRegistry::new();
    let dir = Path::new(skills_dir);

    if !dir.exists() {
        tracing::warn!("Skills directory does not exist: {}", skills_dir);
        return registry;
    }

    if let Err(e) = scan_skill_dir(dir, &mut registry).await {
        tracing::warn!(error = %e, "Failed to scan skill directory");
    }

    tracing::info!(
        skills = registry.skill_names().len(),
        "Skill registry loaded"
    );
    registry
}

/// Recursively scan a directory for `.skill.json` files.
async fn scan_skill_dir(
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
                    Box::pin(scan_skill_dir(&path, registry)).await?;
                } else if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                    if fname.ends_with(".skill.json") {
                        if let Some(skill) = parse_skill_file(&path).await {
                            tracing::debug!(name = %skill.name, "loaded skill");
                            registry.add_skill(skill);
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

/// Parse a `.skill.json` file into a `SkillDefinition`.
async fn parse_skill_file(path: &Path) -> Option<SkillDefinition> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    match serde_json::from_str::<SkillDefinition>(&content) {
        Ok(skill) => Some(skill),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to parse skill file");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_skill_registry() {
        let registry = load_skill_registry("prompts/skills").await;
        assert!(registry.skill_names().len() > 0);
    }
}
