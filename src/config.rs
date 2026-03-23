use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub default_author: String,
    pub notes_dir: PathBuf,
    pub statuses: StatusConfig,
    pub projects: HashMap<String, String>,
    #[serde(skip)]
    pub config_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct StatusConfig {
    #[allow(dead_code)]
    pub values: Vec<String>,
    pub default: String,
}

impl Config {
    pub fn prefix_for_project(&self, project: &str) -> Option<&str> {
        self.projects.get(project).map(|s| s.as_str())
    }

    pub fn project_name_for_prefix(&self, prefix: &str) -> Option<&str> {
        self.projects
            .iter()
            .find(|(_, v)| v.as_str() == prefix)
            .map(|(k, _)| k.as_str())
    }

    /// Resolve a project name or prefix to a prefix.
    /// Accepts either "TickDown" (project name) or "LAW" (prefix directly).
    pub fn resolve_prefix(&self, input: &str) -> Option<String> {
        // Try as project name first
        if let Some(prefix) = self.prefix_for_project(input) {
            return Some(prefix.to_string());
        }
        // Try as a prefix directly
        if self.projects.values().any(|v| v == input) {
            return Some(input.to_string());
        }
        None
    }
}

#[cfg(test)]
fn test_config() -> Config {
    let mut projects = HashMap::new();
    projects.insert("TickDown".to_string(), "LAW".to_string());
    projects.insert("Garten".to_string(), "GTN".to_string());
    Config {
        default_author: "Test".to_string(),
        notes_dir: PathBuf::from("."),
        statuses: StatusConfig {
            values: vec!["New".into(), "Done".into()],
            default: "New".to_string(),
        },
        projects,
        config_path: PathBuf::from(".tickdown.toml"),
    }
}

pub fn load_config(config_path: Option<&Path>) -> Result<Config> {
    let path = if let Some(p) = config_path {
        p.to_path_buf()
    } else if let Ok(env_path) = std::env::var("TICKDOWN_CONFIG") {
        PathBuf::from(env_path)
    } else {
        find_config_in_ancestors()?
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let mut config: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config: {}", path.display()))?;
    config.config_path = path;
    Ok(config)
}

fn find_config_in_ancestors() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join(".tickdown.toml");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !dir.pop() {
            anyhow::bail!("No .tickdown.toml found in current directory or ancestors");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_for_project_found() {
        let config = test_config();
        assert_eq!(config.prefix_for_project("TickDown"), Some("LAW"));
        assert_eq!(config.prefix_for_project("Garten"), Some("GTN"));
    }

    #[test]
    fn test_prefix_for_project_not_found() {
        let config = test_config();
        assert_eq!(config.prefix_for_project("Unknown"), None);
    }

    #[test]
    fn test_project_name_for_prefix_found() {
        let config = test_config();
        assert_eq!(config.project_name_for_prefix("LAW"), Some("TickDown"));
        assert_eq!(config.project_name_for_prefix("GTN"), Some("Garten"));
    }

    #[test]
    fn test_project_name_for_prefix_not_found() {
        let config = test_config();
        assert_eq!(config.project_name_for_prefix("XYZ"), None);
    }

    #[test]
    fn test_resolve_prefix_by_project_name() {
        let config = test_config();
        assert_eq!(config.resolve_prefix("TickDown"), Some("LAW".to_string()));
    }

    #[test]
    fn test_resolve_prefix_by_prefix_directly() {
        let config = test_config();
        assert_eq!(config.resolve_prefix("LAW"), Some("LAW".to_string()));
        assert_eq!(config.resolve_prefix("GTN"), Some("GTN".to_string()));
    }

    #[test]
    fn test_resolve_prefix_unknown() {
        let config = test_config();
        assert_eq!(config.resolve_prefix("NOPE"), None);
    }

    #[test]
    fn test_resolve_prefix_prefers_project_name_over_prefix() {
        // If a project name happens to equal another project's prefix,
        // project name lookup takes priority
        let mut projects = HashMap::new();
        projects.insert("GTN".to_string(), "SPECIAL".to_string());
        projects.insert("Garten".to_string(), "GTN".to_string());
        let config = Config {
            default_author: "Test".to_string(),
            notes_dir: PathBuf::from("."),
            statuses: StatusConfig {
                values: vec![],
                default: "New".to_string(),
            },
            projects,
            config_path: PathBuf::from("."),
        };
        // "GTN" as input matches the project name "GTN" -> prefix "SPECIAL"
        assert_eq!(config.resolve_prefix("GTN"), Some("SPECIAL".to_string()));
    }

    #[test]
    fn test_load_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".tickdown.toml");
        std::fs::write(
            &config_path,
            r#"
default_author = "File Author"
notes_dir = "."

[statuses]
values = ["New", "Done"]
default = "New"

[projects]
Alpha = "ALP"
"#,
        )
        .unwrap();

        let config = load_config(Some(&config_path)).unwrap();
        assert_eq!(config.default_author, "File Author");
        assert_eq!(config.prefix_for_project("Alpha"), Some("ALP"));
        assert_eq!(config.config_path, config_path);
    }

    #[test]
    fn test_load_config_missing_file() {
        let result = load_config(Some(std::path::Path::new("/nonexistent/.tickdown.toml")));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".tickdown.toml");
        std::fs::write(&config_path, "this is not valid toml {{{{").unwrap();

        let result = load_config(Some(&config_path));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_from_env_var() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".tickdown.toml");
        std::fs::write(
            &config_path,
            r#"
default_author = "Env Author"
notes_dir = "."

[statuses]
values = ["New"]
default = "New"

[projects]
Bravo = "BRV"
"#,
        )
        .unwrap();

        std::env::set_var("TICKDOWN_CONFIG", &config_path);
        let config = load_config(None).unwrap();
        std::env::remove_var("TICKDOWN_CONFIG");

        assert_eq!(config.default_author, "Env Author");
        assert_eq!(config.prefix_for_project("Bravo"), Some("BRV"));
    }

    #[test]
    fn test_find_config_in_ancestors() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&child).unwrap();
        let config_path = dir.path().join(".tickdown.toml");
        std::fs::write(
            &config_path,
            r#"
default_author = "Ancestor"
notes_dir = "."

[statuses]
values = ["New"]
default = "New"

[projects]
Foo = "FOO"
"#,
        )
        .unwrap();

        // Temporarily change cwd to the deeply nested child
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&child).unwrap();

        // Should find the config in an ancestor
        let found = find_config_in_ancestors().unwrap();
        assert_eq!(found, config_path);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_find_config_in_ancestors_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("isolated");
        std::fs::create_dir_all(&child).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&child).unwrap();

        let result = find_config_in_ancestors();
        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No .tickdown.toml found"));
    }

    #[test]
    fn test_load_config_from_toml_string() {
        let toml_str = r#"
default_author = "Test Author"
notes_dir = "C:\\test"

[statuses]
values = ["New", "Done"]
default = "New"

[projects]
MyProject = "MP"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_author, "Test Author");
        assert_eq!(config.notes_dir, PathBuf::from("C:\\test"));
        assert_eq!(config.statuses.default, "New");
        assert_eq!(config.prefix_for_project("MyProject"), Some("MP"));
    }
}
