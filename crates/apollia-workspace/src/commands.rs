//! Loader for custom slash command files.
//!
//! Reads `.md` files from a directory and parses their YAML frontmatter
//! to extract the command description, declared argument names, and prompt
//! template.  Only simple inline YAML is supported - no external YAML crate
//! dependency is required.

use std::path::Path;

/// A command definition loaded from a single Markdown file.
#[derive(Debug, Clone)]
pub struct LoadedCommand {
    /// File stem used as the command name (e.g., `review` from `review.md`).
    pub name: String,
    /// Short description from the `description:` frontmatter key, or empty.
    pub description: String,
    /// Prompt template body with `{{placeholder}}` variables, trimmed.
    pub prompt_template: String,
    /// Placeholder names extracted from the template (`{{name}}` patterns).
    pub args: Vec<String>,
}

/// Loads custom slash command definitions from a directory of Markdown files.
pub struct CommandLoader;

impl CommandLoader {
    /// Loads all `.md` files from `dir` as [`LoadedCommand`]s.
    ///
    /// Returns an empty `Vec` when the directory does not exist or cannot be
    /// read - callers treat absence as an empty command set (fail-silent).
    pub async fn load_dir(dir: &Path) -> Vec<LoadedCommand> {
        let mut read_dir = match tokio::fs::read_dir(dir).await {
            Ok(rd) => rd,
            Err(_) => return Vec::new(),
        };

        let mut commands = Vec::new();
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            commands.push(parse_command_file(name, &content));
        }

        commands
    }
}

/// Parses the content of a command Markdown file into a [`LoadedCommand`].
///
/// Recognises a YAML frontmatter block delimited by `---` on its own line.
/// Extracts `description:` and `args:` keys.  Argument placeholders
/// (`{{name}}`) that appear in the body but not in the frontmatter are
/// appended to the `args` list.
fn parse_command_file(name: String, content: &str) -> LoadedCommand {
    let (description, mut args, body) = extract_frontmatter(content);

    // Supplement with any {{placeholder}} patterns found in the body.
    for ta in extract_template_args(&body) {
        if !args.contains(&ta) {
            args.push(ta);
        }
    }

    LoadedCommand {
        name,
        description,
        prompt_template: body.trim().to_string(),
        args,
    }
}

/// Splits a Markdown file into frontmatter fields and body.
///
/// Returns `(description, args, body)`.
/// Files that do not start with `---` are treated as plain templates with no
/// frontmatter.
fn extract_frontmatter(content: &str) -> (String, Vec<String>, String) {
    let lines: Vec<&str> = content.lines().collect();

    if lines.first().map(|l| l.trim()) != Some("---") {
        return (String::new(), Vec::new(), content.to_string());
    }

    // Find the closing `---` after the opening delimiter.
    let close_offset = lines[1..].iter().position(|l| l.trim() == "---");
    let close_idx = match close_offset {
        Some(i) => i + 1,
        None => return (String::new(), Vec::new(), content.to_string()),
    };

    let frontmatter_lines = &lines[1..close_idx];
    let body = lines[close_idx + 1..].join("\n");

    let mut description = String::new();
    let mut args: Vec<String> = Vec::new();

    for line in frontmatter_lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("description:") {
            description = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("args:") {
            args = parse_yaml_string_array(rest.trim());
        }
    }

    (description, args, body)
}

/// Parses a simple YAML inline string array: `[val1, val2]` → `["val1", "val2"]`.
fn parse_yaml_string_array(s: &str) -> Vec<String> {
    let s = s.trim_start_matches('[').trim_end_matches(']');
    s.split(',')
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Extracts `{{name}}` placeholder names from a template string.
fn extract_template_args(template: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("}}") {
            let name = rest[..end].trim().to_string();
            if !name.is_empty() && !args.contains(&name) {
                args.push(name);
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_md(dir: &TempDir, name: &str, content: &str) {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(path).expect("create temp file");
        f.write_all(content.as_bytes()).expect("write temp file");
    }

    #[tokio::test]
    async fn load_dir_returns_empty_when_directory_missing() {
        // GIVEN a non-existent directory
        let dir = TempDir::new().expect("tmp dir");
        let missing = dir.path().join("does_not_exist");

        // WHEN
        let commands = CommandLoader::load_dir(&missing).await;

        // THEN empty vec, no panic
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn load_dir_ignores_non_md_files() {
        // GIVEN a dir containing a .txt and a .md file
        let dir = TempDir::new().expect("tmp dir");
        write_md(&dir, "cmd.txt", "not a command");
        write_md(&dir, "cmd.md", "no frontmatter body");

        // WHEN
        let commands = CommandLoader::load_dir(dir.path()).await;

        // THEN only the .md file is loaded
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "cmd");
    }

    #[tokio::test]
    async fn load_dir_parses_frontmatter_fields() {
        // GIVEN a .md file with YAML frontmatter
        let dir = TempDir::new().expect("tmp dir");
        write_md(
            &dir,
            "review.md",
            "---\ndescription: Code review\nargs: [focus]\n---\n\nAnalyse {{focus}}.\n",
        );

        // WHEN
        let commands = CommandLoader::load_dir(dir.path()).await;

        // THEN description, template, and args are populated
        assert_eq!(commands.len(), 1);
        let cmd = &commands[0];
        assert_eq!(cmd.name, "review");
        assert_eq!(cmd.description, "Code review");
        assert!(cmd.prompt_template.contains("{{focus}}"));
        assert!(cmd.args.contains(&"focus".to_string()));
    }

    #[test]
    fn extract_template_args_finds_all_placeholders() {
        // GIVEN template with two placeholders
        // WHEN the placeholder names are extracted
        let args = extract_template_args("Hello {{name}}, topic is {{topic}}.");
        // THEN both come back, in the order they appear
        assert_eq!(args, vec!["name".to_string(), "topic".to_string()]);
    }

    #[test]
    fn extract_template_args_deduplicates() {
        // GIVEN template repeating the same placeholder
        // WHEN the placeholder names are extracted
        let args = extract_template_args("{{x}} and {{x}} again");
        // THEN the repeated name is listed once
        assert_eq!(args, vec!["x".to_string()]);
    }

    #[test]
    fn parse_yaml_string_array_handles_comma_separated() {
        // GIVEN an inline YAML array of two bare words
        // WHEN it is parsed as a string array
        // THEN both words come back, split on the comma
        assert_eq!(
            parse_yaml_string_array("[focus, depth]"),
            vec!["focus".to_string(), "depth".to_string()]
        );
    }

    #[test]
    fn parse_yaml_string_array_handles_quoted_values() {
        // GIVEN an inline YAML array holding one quoted value
        // WHEN it is parsed as a string array
        // THEN the quotes are stripped from the value
        assert_eq!(
            parse_yaml_string_array(r#"["focus"]"#),
            vec!["focus".to_string()]
        );
    }

    #[test]
    fn parse_yaml_string_array_empty_returns_empty_vec() {
        // GIVEN an empty inline YAML array
        // WHEN it is parsed as a string array
        let result: Vec<String> = parse_yaml_string_array("[]");
        // THEN nothing comes back, rather than one empty entry
        assert!(result.is_empty());
    }
}
