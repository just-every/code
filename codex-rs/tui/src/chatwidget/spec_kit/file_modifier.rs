//! File modification engine for quality gate auto-resolutions
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit multi-agent automation framework
//!
//! Provides safe, auditable file modifications with backup and validation

use super::error::{Result, SpecKitError};
use std::fs;
use std::path::{Path, PathBuf};

/// Types of modifications that can be applied to SPEC/plan/tasks files
#[derive(Debug, Clone)]
pub enum SpecModification {
    /// Add a new requirement to spec.md
    AddRequirement {
        section: String,           // "Objectives", "Acceptance Criteria", etc.
        requirement_text: String,
        position: InsertPosition,
    },

    /// Update an existing requirement
    UpdateRequirement {
        search_text: String,       // Text to find
        replacement_text: String,  // New text
    },

    /// Add a new section to the document
    AddSection {
        section_title: String,
        content: String,
        after_section: Option<String>,  // Insert after this section
    },

    /// Replace all occurrences of a term (for terminology fixes)
    ReplaceTerminology {
        old_term: String,
        new_term: String,
        case_sensitive: bool,
    },

    /// Append text to existing section
    AppendToSection {
        section: String,
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InsertPosition {
    /// Add at the end of the section
    End,
    /// Add at the beginning of the section
    Beginning,
    /// Add after a specific line number
    AfterLine(usize),
}

/// Result of applying a modification
#[derive(Debug, Clone)]
pub struct ModificationOutcome {
    pub file_path: PathBuf,
    pub backup_path: PathBuf,
    pub changes: Vec<LineChange>,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct LineChange {
    pub line_number: usize,
    pub change_type: ChangeType,
    pub old_text: Option<String>,
    pub new_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
}

/// Apply a modification to a file with safety checks
pub fn apply_modification(
    file_path: &Path,
    modification: &SpecModification,
) -> Result<ModificationOutcome> {
    // 1. Read original file
    let original_content = fs::read_to_string(file_path).map_err(|e| {
        SpecKitError::FileRead {
            path: file_path.to_path_buf(),
            source: e,
        }
    })?;

    // 2. Create backup
    let backup_path = create_backup(file_path, &original_content)?;

    // 3. Apply modification
    let (modified_content, changes) = apply_modification_to_content(&original_content, modification)?;

    // 4. Write modified content
    fs::write(file_path, &modified_content).map_err(|e| SpecKitError::FileWrite {
        path: file_path.to_path_buf(),
        source: e,
    })?;

    // 5. Validate file structure (basic checks)
    validate_markdown_structure(&modified_content)?;

    Ok(ModificationOutcome {
        file_path: file_path.to_path_buf(),
        backup_path,
        changes,
        success: true,
    })
}

/// Create timestamped backup of file
fn create_backup(file_path: &Path, content: &str) -> Result<PathBuf> {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_path = file_path.with_extension(format!("backup_{}.md", timestamp));

    fs::write(&backup_path, content).map_err(|e| SpecKitError::FileWrite {
        path: backup_path.clone(),
        source: e,
    })?;

    Ok(backup_path)
}

/// Apply modification to content string
fn apply_modification_to_content(
    content: &str,
    modification: &SpecModification,
) -> Result<(String, Vec<LineChange>)> {
    match modification {
        SpecModification::AddRequirement {
            section,
            requirement_text,
            position,
        } => add_requirement_to_content(content, section, requirement_text, *position),

        SpecModification::UpdateRequirement {
            search_text,
            replacement_text,
        } => update_requirement_in_content(content, search_text, replacement_text),

        SpecModification::AddSection {
            section_title,
            content: section_content,
            after_section,
        } => add_section_to_content(content, section_title, section_content, after_section.as_deref()),

        SpecModification::ReplaceTerminology {
            old_term,
            new_term,
            case_sensitive,
        } => replace_terminology_in_content(content, old_term, new_term, *case_sensitive),

        SpecModification::AppendToSection { section, text } => {
            append_to_section_in_content(content, section, text)
        }
    }
}

fn add_requirement_to_content(
    content: &str,
    section: &str,
    requirement: &str,
    position: InsertPosition,
) -> Result<(String, Vec<LineChange>)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut changes = Vec::new();
    let mut in_target_section = false;
    let mut section_end_line = None;

    // Find the section
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("##") && line.to_lowercase().contains(&section.to_lowercase()) {
            in_target_section = true;
        } else if in_target_section && line.starts_with("##") {
            // Found next section, mark end
            section_end_line = Some(idx);
            in_target_section = false;
        }
    }

    if section_end_line.is_none() && in_target_section {
        section_end_line = Some(lines.len());
    }

    // Insert requirement
    let insert_at = match position {
        InsertPosition::End => section_end_line.unwrap_or(lines.len()),
        InsertPosition::Beginning => {
            // Find first line after section header
            lines
                .iter()
                .position(|l| l.starts_with("##") && l.to_lowercase().contains(&section.to_lowercase()))
                .map(|idx| idx + 1)
                .unwrap_or(0)
        }
        InsertPosition::AfterLine(line_num) => line_num + 1,
    };

    for (idx, line) in lines.iter().enumerate() {
        new_lines.push(line.to_string());
        if idx + 1 == insert_at {
            new_lines.push(format!("- {}", requirement));
            changes.push(LineChange {
                line_number: idx + 1,
                change_type: ChangeType::Added,
                old_text: None,
                new_text: requirement.to_string(),
            });
        }
    }

    Ok((new_lines.join("\n"), changes))
}

fn update_requirement_in_content(
    content: &str,
    search: &str,
    replacement: &str,
) -> Result<(String, Vec<LineChange>)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut changes = Vec::new();
    let mut found = false;

    for (idx, line) in lines.iter().enumerate() {
        if line.contains(search) {
            let new_line = line.replace(search, replacement);
            new_lines.push(new_line.clone());
            changes.push(LineChange {
                line_number: idx + 1,
                change_type: ChangeType::Modified,
                old_text: Some(line.to_string()),
                new_text: new_line,
            });
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !found {
        return Err(SpecKitError::from_string(format!(
            "Search text not found: {}",
            search
        )));
    }

    Ok((new_lines.join("\n"), changes))
}

fn add_section_to_content(
    content: &str,
    section_title: &str,
    section_content: &str,
    after_section: Option<&str>,
) -> Result<(String, Vec<LineChange>)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut changes = Vec::new();

    let insert_at = if let Some(after) = after_section {
        // Find the section to insert after
        let mut found_section = false;
        let mut insert_line = None;

        for (idx, line) in lines.iter().enumerate() {
            if line.starts_with("##") && line.to_lowercase().contains(&after.to_lowercase()) {
                found_section = true;
            } else if found_section && line.starts_with("##") {
                insert_line = Some(idx);
                break;
            }
        }

        insert_line.unwrap_or(lines.len())
    } else {
        lines.len()
    };

    for (idx, line) in lines.iter().enumerate() {
        new_lines.push(line.to_string());
        if idx + 1 == insert_at {
            new_lines.push(String::new());  // Blank line
            new_lines.push(format!("## {}", section_title));
            new_lines.push(String::new());
            new_lines.push(section_content.to_string());
            new_lines.push(String::new());

            changes.push(LineChange {
                line_number: idx + 2,
                change_type: ChangeType::Added,
                old_text: None,
                new_text: format!("## {} (section added)", section_title),
            });
        }
    }

    Ok((new_lines.join("\n"), changes))
}

fn replace_terminology_in_content(
    content: &str,
    old_term: &str,
    new_term: &str,
    case_sensitive: bool,
) -> Result<(String, Vec<LineChange>)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut changes = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let contains_term = if case_sensitive {
            line.contains(old_term)
        } else {
            line.to_lowercase().contains(&old_term.to_lowercase())
        };

        if contains_term {
            let new_line = if case_sensitive {
                line.replace(old_term, new_term)
            } else {
                // Case-insensitive replacement (preserve case of first letter if possible)
                replace_case_insensitive(line, old_term, new_term)
            };

            new_lines.push(new_line.clone());
            changes.push(LineChange {
                line_number: idx + 1,
                change_type: ChangeType::Modified,
                old_text: Some(line.to_string()),
                new_text: new_line,
            });
        } else {
            new_lines.push(line.to_string());
        }
    }

    Ok((new_lines.join("\n"), changes))
}

fn replace_case_insensitive(text: &str, old: &str, new: &str) -> String {
    // Simple case-insensitive replacement
    let lower_text = text.to_lowercase();
    let lower_old = old.to_lowercase();

    if let Some(pos) = lower_text.find(&lower_old) {
        let mut result = String::new();
        result.push_str(&text[..pos]);
        result.push_str(new);
        result.push_str(&text[pos + old.len()..]);
        result
    } else {
        text.to_string()
    }
}

fn append_to_section_in_content(
    content: &str,
    section: &str,
    text: &str,
) -> Result<(String, Vec<LineChange>)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut changes = Vec::new();
    let mut in_target_section = false;
    let mut section_end_line = None;

    // Find section boundaries
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("##") && line.to_lowercase().contains(&section.to_lowercase()) {
            in_target_section = true;
        } else if in_target_section && line.starts_with("##") {
            section_end_line = Some(idx);
            in_target_section = false;
        }
    }

    if section_end_line.is_none() && in_target_section {
        section_end_line = Some(lines.len());
    }

    let insert_at = section_end_line.ok_or_else(|| {
        SpecKitError::from_string(format!("Section '{}' not found", section))
    })?;

    for (idx, line) in lines.iter().enumerate() {
        new_lines.push(line.to_string());
        if idx + 1 == insert_at {
            new_lines.push(text.to_string());
            changes.push(LineChange {
                line_number: idx + 1,
                change_type: ChangeType::Added,
                old_text: None,
                new_text: text.to_string(),
            });
        }
    }

    Ok((new_lines.join("\n"), changes))
}

/// Validate markdown structure after modifications
fn validate_markdown_structure(content: &str) -> Result<()> {
    // Basic validation: ensure file isn't empty
    if content.trim().is_empty() {
        return Err(SpecKitError::from_string("Modified file is empty"));
    }

    // Ensure there's at least one heading
    let has_heading = content.lines().any(|line| line.starts_with('#'));
    if !has_heading {
        return Err(SpecKitError::from_string(
            "Modified file has no markdown headings",
        ));
    }

    Ok(())
}

/// Restore file from backup
pub fn restore_from_backup(backup_path: &Path, original_path: &Path) -> Result<()> {
    fs::copy(backup_path, original_path).map_err(|e| SpecKitError::FileWrite {
        path: original_path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SPEC: &str = r#"# Spec: Test Feature

## Objectives
- Implement authentication
- Add user management

## Acceptance Criteria
- Users can log in
- Sessions are secure

## Scope
This is the scope section.
"#;

    #[test]
    fn test_add_requirement_end() {
        let modification = SpecModification::AddRequirement {
            section: "Objectives".to_string(),
            requirement_text: "Support OAuth2".to_string(),
            position: InsertPosition::End,
        };

        let (result, changes) = apply_modification_to_content(TEST_SPEC, &modification).unwrap();

        assert!(result.contains("Support OAuth2"));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Added);
    }

    #[test]
    fn test_update_requirement() {
        let modification = SpecModification::UpdateRequirement {
            search_text: "user management".to_string(),
            replacement_text: "account management".to_string(),
        };

        let (result, changes) = apply_modification_to_content(TEST_SPEC, &modification).unwrap();

        assert!(result.contains("account management"));
        assert!(!result.contains("user management"));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn test_replace_terminology() {
        let modification = SpecModification::ReplaceTerminology {
            old_term: "user".to_string(),
            new_term: "account".to_string(),
            case_sensitive: false,
        };

        let (result, changes) = apply_modification_to_content(TEST_SPEC, &modification).unwrap();

        // Should replace "user" in multiple places
        assert!(result.contains("account management"));
        assert!(result.contains("accounts can log in"));
        assert!(changes.len() >= 2);
    }

    #[test]
    fn test_add_section() {
        let modification = SpecModification::AddSection {
            section_title: "Risks".to_string(),
            content: "- Security vulnerabilities\n- Performance concerns".to_string(),
            after_section: Some("Acceptance Criteria".to_string()),
        };

        let (result, changes) = apply_modification_to_content(TEST_SPEC, &modification).unwrap();

        assert!(result.contains("## Risks"));
        assert!(result.contains("Security vulnerabilities"));
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn test_append_to_section() {
        let modification = SpecModification::AppendToSection {
            section: "Objectives".to_string(),
            text: "- Enable SSO".to_string(),
        };

        let (result, changes) = apply_modification_to_content(TEST_SPEC, &modification).unwrap();

        assert!(result.contains("Enable SSO"));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Added);
    }

    #[test]
    fn test_validate_markdown_structure() {
        let valid = "# Title\n## Section\nContent";
        assert!(validate_markdown_structure(valid).is_ok());

        let empty = "";
        assert!(validate_markdown_structure(empty).is_err());

        let no_headings = "Just some text without headings";
        assert!(validate_markdown_structure(no_headings).is_err());
    }

    #[test]
    fn test_update_requirement_not_found() {
        let modification = SpecModification::UpdateRequirement {
            search_text: "nonexistent text".to_string(),
            replacement_text: "new text".to_string(),
        };

        let result = apply_modification_to_content(TEST_SPEC, &modification);
        assert!(result.is_err());
    }
}
