use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
pub enum SkillScope {
    Repo,
    User,
    System,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub scope: SkillScope,
    pub content: String,
}
