use anyhow::{bail, Context, Result};

use crate::config::Config;

pub fn run(config: &Config, name: &str, prefix: &str) -> Result<()> {
    // Validate prefix format: uppercase start, only uppercase + digits
    if prefix.is_empty()
        || !prefix.starts_with(|c: char| c.is_ascii_uppercase())
        || !prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        bail!(
            "Invalid prefix: {}. Must start with an uppercase letter and contain only uppercase letters and digits.",
            prefix
        );
    }

    // Check for name collision
    if config.prefix_for_project(name).is_some() {
        bail!("Project name {} already exists", name);
    }

    // Check for prefix collision
    if config.project_name_for_prefix(prefix).is_some() {
        bail!("Prefix {} is already used by project {}", prefix, config.project_name_for_prefix(prefix).unwrap());
    }

    // Update config file
    let content = std::fs::read_to_string(&config.config_path)
        .with_context(|| format!("Failed to read config: {}", config.config_path.display()))?;

    let mut doc: toml::Value = content
        .parse()
        .with_context(|| "Failed to parse config as TOML")?;

    if let Some(projects) = doc.get_mut("projects").and_then(|v| v.as_table_mut()) {
        projects.insert(name.to_string(), toml::Value::String(prefix.to_string()));
    }

    let new_content = toml::to_string_pretty(&doc)?;
    std::fs::write(&config.config_path, new_content)?;

    println!("Created project {} ({})", name, prefix);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StatusConfig;
    use std::collections::HashMap;

    fn setup_test(dir: &std::path::Path) -> Config {
        let config_path = dir.join(".tickdown.toml");
        std::fs::write(
            &config_path,
            r#"
default_author = "Tester"
notes_dir = "."

[statuses]
values = ["New", "Done"]
default = "New"

[projects]
Existing = "EX"
"#,
        )
        .unwrap();

        let mut projects = HashMap::new();
        projects.insert("Existing".to_string(), "EX".to_string());
        Config {
            default_author: "Tester".to_string(),
            notes_dir: dir.to_path_buf(),
            statuses: StatusConfig {
                values: vec!["New".into(), "Done".into()],
                default: "New".to_string(),
            },
            projects,
            sync: vec![],
            config_path,
        }
    }

    #[test]
    fn test_create_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        run(&config, "NewProject", "NP").unwrap();

        let cfg = std::fs::read_to_string(dir.path().join(".tickdown.toml")).unwrap();
        assert!(cfg.contains("NewProject"));
        assert!(cfg.contains("NP"));
    }

    #[test]
    fn test_create_project_name_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "Existing", "NEW");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_create_project_prefix_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "Another", "EX");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already used"));
    }

    #[test]
    fn test_create_project_invalid_prefix_lowercase() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "Bad", "bad");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid prefix"));
    }

    #[test]
    fn test_create_project_invalid_prefix_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "Bad", "");
        assert!(result.is_err());
    }
}
