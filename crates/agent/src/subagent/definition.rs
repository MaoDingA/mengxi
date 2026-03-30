// subagent/definition.rs — Parse subagent definitions from Markdown with YAML frontmatter
//
// No serde_yaml dependency. Manual line-by-line parsing of a constrained YAML subset:
//   key: value
//   key: [a, b, c]
//   key:
//     - item
//   key: |
//     multiline text

use std::path::Path;

/// A parsed subagent definition.
#[derive(Debug, Clone)]
pub struct SubagentDefinition {
    /// Unique agent name (used as tool name: subagent_{name}).
    pub name: String,
    /// Model override (None = use provider default).
    pub model: Option<String>,
    /// Tools this agent has access to (subset of parent's registry).
    pub tools: Vec<String>,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Maximum turns for the agent loop.
    pub max_turns: usize,
}

/// Errors when parsing subagent definitions.
#[derive(Debug, thiserror::Error)]
pub enum SubagentDefinitionError {
    #[error("DEFINITION_MISSING_FRONTMATTER -- {0}")]
    MissingFrontmatter(String),
    #[error("DEFINITION_YAML_ERROR -- {0}")]
    YamlParseError(String),
    #[error("DEFINITION_MISSING_FIELD -- {0}")]
    MissingField(String),
    #[error("DEFINITION_IO_ERROR -- {0}")]
    IoError(String),
}

impl SubagentDefinition {
    /// Parse a subagent definition from Markdown with YAML frontmatter.
    ///
    /// Expected format:
    /// ```markdown
    /// ---
    /// name: explore
    /// model: claude-sonnet-4-20250514
    /// tools:
    ///   - search_by_tag
    ///   - analyze_project
    /// system_prompt: |
    ///   You are an exploration agent...
    /// max_turns: 10
    /// ---
    /// # Body text (ignored)
    /// ```
    pub fn from_markdown(content: &str) -> Result<Self, SubagentDefinitionError> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(SubagentDefinitionError::MissingFrontmatter(
                "Definition must start with YAML frontmatter (---)".into(),
            ));
        }

        // Find closing ---
        let after_first = &trimmed[3..];
        let end_idx = after_first
            .find("\n---")
            .ok_or_else(|| {
                SubagentDefinitionError::MissingFrontmatter(
                    "YAML frontmatter must be closed with ---".into(),
                )
            })?;

        let yaml_str = &after_first[..end_idx];

        // Parse the constrained YAML subset
        let mut name = None;
        let mut model = None;
        let mut tools: Vec<String> = Vec::new();
        let mut system_prompt = None;
        let mut max_turns: Option<usize> = None;

        let mut lines = yaml_str.lines().peekable();
        while let Some(line) = lines.next() {
            let trimmed_line = line.trim();

            if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
                continue;
            }

            // Multiline literal block (system_prompt: |)
            if let Some(rest) = trimmed_line.strip_prefix("system_prompt:") {
                let rest = rest.trim();
                if rest == "|" {
                    // Collect indented lines until dedent
                    let mut prompt_lines = Vec::new();
                    while let Some(&next) = lines.peek() {
                        if next.is_empty() && prompt_lines.is_empty() {
                            lines.next();
                            continue;
                        }
                        // Check if line is indented (part of the block)
                        if next.starts_with("  ") || next.starts_with("\t") {
                            prompt_lines.push(lines.next().unwrap());
                        } else if next.trim().is_empty() {
                            // Empty line within block
                            prompt_lines.push(lines.next().unwrap());
                        } else {
                            // Dedent — end of block
                            break;
                        }
                    }
                    // Remove trailing empty lines
                    while prompt_lines.last().map(|l| l.trim().is_empty()) == Some(true) {
                        prompt_lines.pop();
                    }
                    // Calculate minimum indentation and dedent
                    let min_indent = prompt_lines
                        .iter()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.len() - l.trim_start().len())
                        .min()
                        .unwrap_or(0);
                    let prompt: String = prompt_lines
                        .iter()
                        .map(|l| {
                            if l.trim().is_empty() {
                                ""
                            } else if l.len() > min_indent {
                                &l[min_indent..]
                            } else {
                                l
                            }
                        })
                        .collect::<Vec<&str>>()
                        .join("\n");
                    let prompt = prompt.trim_end().to_string();
                    system_prompt = Some(prompt);
                } else {
                    // Single-line system_prompt
                    system_prompt = Some(rest.to_string());
                }
                continue;
            }

            if let Some(rest) = trimmed_line.strip_prefix("name:") {
                name = Some(rest.trim().to_string());
                continue;
            }

            if let Some(rest) = trimmed_line.strip_prefix("model:") {
                let val = rest.trim();
                if !val.is_empty() {
                    model = Some(val.to_string());
                }
                continue;
            }

            if let Some(rest) = trimmed_line.strip_prefix("max_turns:") {
                let val = rest.trim();
                max_turns = Some(val.parse::<usize>().map_err(|_| {
                    SubagentDefinitionError::YamlParseError(format!(
                        "Invalid max_turns value: '{}'",
                        val
                    ))
                })?);
                continue;
            }

            if let Some(rest) = trimmed_line.strip_prefix("tools:") {
                let val = rest.trim();
                if val.starts_with('[') && val.ends_with(']') {
                    // Inline array: [a, b, c]
                    tools = val[1..val.len() - 1]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                // If empty after "tools:", items follow as YAML list items (- item)
                continue;
            }

            // YAML list item: "  - item"
            if trimmed_line.starts_with("- ") {
                let item = trimmed_line[2..].trim().to_string();
                if !item.is_empty() {
                    tools.push(item);
                }
                continue;
            }
        }

        let name = name.ok_or_else(|| {
            SubagentDefinitionError::MissingField("name".into())
        })?;
        let system_prompt = system_prompt.ok_or_else(|| {
            SubagentDefinitionError::MissingField("system_prompt".into())
        })?;
        let max_turns = max_turns.unwrap_or(10);

        Ok(Self {
            name,
            model,
            tools,
            system_prompt,
            max_turns,
        })
    }

    /// Load all subagent definitions from a directory.
    ///
    /// Scans for `.md` files, parses each one. Invalid files are skipped
    /// with a warning logged.
    pub fn load_from_dir(dir: &Path) -> Vec<SubagentDefinition> {
        let mut definitions = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return definitions,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match Self::from_markdown(&content) {
                        Ok(defn) => definitions.push(defn),
                        Err(e) => {
                            log::warn!(
                                "Skipping subagent definition {}: {}",
                                path.display(),
                                e
                            );
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read {}: {}", path.display(), e);
                    }
                }
            }
        }
        definitions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_full_definition() {
        let content = "\
---
name: explore
model: claude-sonnet-4-20250514
tools:
  - search_by_tag
  - search_by_image
  - analyze_project
system_prompt: |
  You are an exploration agent.
  Analyze color distributions.
max_turns: 10
---
# Explore Agent
Body text.";
        let defn = SubagentDefinition::from_markdown(content).unwrap();
        assert_eq!(defn.name, "explore");
        assert_eq!(defn.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(
            defn.tools,
            vec!["search_by_tag", "search_by_image", "analyze_project"]
        );
        assert!(defn.system_prompt.contains("exploration agent"));
        assert_eq!(defn.max_turns, 10);
    }

    #[test]
    fn test_parse_inline_tools() {
        let content = "\
---
name: test
tools: [search_by_tag, analyze_project]
system_prompt: Hello
max_turns: 5
---";
        let defn = SubagentDefinition::from_markdown(content).unwrap();
        assert_eq!(defn.tools, vec!["search_by_tag", "analyze_project"]);
    }

    #[test]
    fn test_parse_no_model() {
        let content = "\
---
name: minimal
tools: [search_by_tag]
system_prompt: Minimal agent
max_turns: 3
---";
        let defn = SubagentDefinition::from_markdown(content).unwrap();
        assert!(defn.model.is_none());
    }

    #[test]
    fn test_parse_default_max_turns() {
        let content = "\
---
name: no_turns
tools: []
system_prompt: No turns specified
---";
        let defn = SubagentDefinition::from_markdown(content).unwrap();
        assert_eq!(defn.max_turns, 10); // default
    }

    #[test]
    fn test_parse_missing_name() {
        let content = "\
---
tools: [search_by_tag]
system_prompt: No name
max_turns: 3
---";
        let result = SubagentDefinition::from_markdown(content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("name"));
    }

    #[test]
    fn test_parse_missing_system_prompt() {
        let content = "\
---
name: no_prompt
tools: []
max_turns: 3
---";
        let result = SubagentDefinition::from_markdown(content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("system_prompt"));
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Just markdown\nNo frontmatter.";
        let result = SubagentDefinition::from_markdown(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unclosed_frontmatter() {
        let content = "---\nname: test\nNo closing delimiter.";
        let result = SubagentDefinition::from_markdown(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_max_turns() {
        let content = "\
---
name: bad_turns
tools: []
system_prompt: test
max_turns: not_a_number
---";
        let result = SubagentDefinition::from_markdown(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("explore.md");
        let mut f = std::fs::File::create(&file_path).unwrap();
        write!(
            f,
            "---\nname: explore\ntools: [search_by_tag]\nsystem_prompt: Explore agent\nmax_turns: 10\n---"
        )
        .unwrap();

        let definitions = SubagentDefinition::load_from_dir(dir.path());
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, "explore");
    }

    #[test]
    fn test_load_from_dir_skips_invalid() {
        let dir = tempfile::tempdir().unwrap();

        let good = dir.path().join("good.md");
        let mut f = std::fs::File::create(&good).unwrap();
        write!(f, "---\nname: good\ntools: []\nsystem_prompt: ok\nmax_turns: 1\n---").unwrap();

        let bad = dir.path().join("bad.md");
        let mut f2 = std::fs::File::create(&bad).unwrap();
        write!(f2, "not valid frontmatter").unwrap();

        let definitions = SubagentDefinition::load_from_dir(dir.path());
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, "good");
    }

    #[test]
    fn test_load_from_nonexistent_dir() {
        let definitions = SubagentDefinition::load_from_dir(Path::new("/nonexistent"));
        assert!(definitions.is_empty());
    }
}
