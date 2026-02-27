use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub steps: Vec<SkillStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStep {
    pub action: String,
    pub target: Option<String>,
    pub text: Option<String>,
}
