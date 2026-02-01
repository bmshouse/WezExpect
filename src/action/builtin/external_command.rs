use crate::action::{Action, ActionConfig, MatchData};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

/// Configuration fields specific to external_command action
#[derive(Debug, Clone, Deserialize)]
struct ExternalCommandConfig {
    /// Shell command to execute (supports $1, $2, etc. for capture groups)
    external_command: String,
    /// Optional working directory for command execution
    #[serde(default)]
    working_dir: Option<String>,
    /// Optional timeout in seconds (default: 30)
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    /// Optional environment variables
    #[serde(default)]
    env: HashMap<String, String>,
}

fn default_timeout() -> u64 {
    30
}

/// Match data for external_command action
struct ExternalCommandMatch {
    captures: Vec<String>,
}

/// Action that executes external shell commands
///
/// This action runs shell commands when patterns match, with support for
/// capture group substitution ($1, $2, etc.), working directory,
/// timeout, and environment variables.
///
/// **Security Warning**: This executes arbitrary shell commands with captured
/// data. Ensure patterns are carefully designed to avoid command injection.
pub struct ExternalCommandAction;

impl ExternalCommandAction {
    /// Parse action-specific config from the flattened TOML
    fn parse_config(&self, config: &ActionConfig) -> Result<ExternalCommandConfig> {
        let ext_config: ExternalCommandConfig = config
            .extra
            .clone()
            .try_into()
            .context("Failed to parse external_command action config")?;
        Ok(ext_config)
    }

    /// Substitute capture groups in command string ($1, $2, etc.)
    fn substitute_captures(command: &str, captures: &[String]) -> String {
        let mut result = command.to_string();

        // Replace $1, $2, etc. with captured values
        for (idx, capture) in captures.iter().enumerate() {
            let placeholder = format!("${}", idx + 1);
            result = result.replace(&placeholder, capture);
        }

        result
    }

    /// Get the shell command for the current platform
    #[cfg(target_os = "windows")]
    fn shell_command() -> (&'static str, &'static str) {
        ("cmd", "/C")
    }

    #[cfg(not(target_os = "windows"))]
    fn shell_command() -> (&'static str, &'static str) {
        ("sh", "-c")
    }
}

#[async_trait]
impl Action for ExternalCommandAction {
    fn action_type(&self) -> &str {
        "external_command"
    }

    fn validate_config(&self, config: &ActionConfig) -> Result<()> {
        // Parse the external_command-specific config
        let ext_config = self.parse_config(config)?;

        // Validate external_command is not empty
        if ext_config.external_command.trim().is_empty() {
            anyhow::bail!("external_command action requires a non-empty 'external_command' field");
        }

        // Validate timeout is reasonable
        if ext_config.timeout_secs == 0 {
            anyhow::bail!("timeout_secs must be greater than 0");
        }
        if ext_config.timeout_secs > 600 {
            tracing::warn!(
                "external_command timeout is very high ({}s). Consider using a lower value.",
                ext_config.timeout_secs
            );
        }

        Ok(())
    }

    fn check_match(&self, content: &str, pattern: &str) -> Result<Option<MatchData>> {
        let re = Regex::new(pattern)?;

        if let Some(captures) = re.captures(content) {
            // Extract all capture groups (skip index 0 which is the full match)
            let capture_strings: Vec<String> = captures
                .iter()
                .skip(1)
                .map(|m| m.map_or(String::new(), |m| m.as_str().to_string()))
                .collect();

            tracing::debug!(
                "ExternalCommandAction matched, {} capture groups: {:?}",
                capture_strings.len(),
                capture_strings
            );

            Ok(Some(Box::new(ExternalCommandMatch {
                captures: capture_strings,
            })))
        } else {
            Ok(None)
        }
    }

    async fn execute(
        &self,
        _pane_id: u32,
        match_data: MatchData,
        config: &ActionConfig,
        _send_delay_ms: u64,
    ) -> Result<()> {
        tracing::info!("ExternalCommandAction executing");

        // Downcast match data
        let match_data = match_data
            .downcast::<ExternalCommandMatch>()
            .map_err(|_| anyhow::anyhow!("Invalid match data type for ExternalCommandAction"))?;

        // Parse external command config
        let ext_config = self.parse_config(config)?;

        // Substitute capture groups in command
        let command = Self::substitute_captures(&ext_config.external_command, &match_data.captures);
        tracing::info!("Executing external command: '{}'", command);

        // Get shell command for platform
        let (shell, shell_arg) = Self::shell_command();

        // Build command
        let mut cmd = Command::new(shell);
        cmd.arg(shell_arg).arg(&command);

        // Set working directory if specified
        if let Some(ref dir) = ext_config.working_dir {
            cmd.current_dir(dir);
        }

        // Set environment variables
        for (key, value) in &ext_config.env {
            cmd.env(key, value);
        }

        // Execute with timeout
        let output =
            tokio::time::timeout(Duration::from_secs(ext_config.timeout_secs), cmd.output())
                .await
                .context("External command timed out")?
                .context("Failed to execute external command")?;

        // Log output
        if !output.stdout.is_empty() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            tracing::info!("Command stdout: {}", stdout.trim());
        }
        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("Command stderr: {}", stderr.trim());
        }

        // Check exit code
        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            anyhow::bail!(
                "External command failed with exit code {}: '{}'",
                code,
                command
            );
        }

        tracing::info!("ExternalCommandAction completed successfully!");

        Ok(())
    }

    fn required_captures(&self) -> Option<usize> {
        // No specific requirement - can work with 0 or more captures
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config(cmd: &str) -> ActionConfig {
        let mut extra = toml::value::Table::new();
        extra.insert(
            "external_command".to_string(),
            toml::Value::String(cmd.to_string()),
        );

        ActionConfig {
            action_type: "external_command".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        }
    }

    #[test]
    fn test_action_type() {
        let action = ExternalCommandAction;
        assert_eq!(action.action_type(), "external_command");
    }

    #[test]
    fn test_validate_config_valid() {
        let action = ExternalCommandAction;
        let config = create_test_config("echo 'test'");
        assert!(action.validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_empty_command() {
        let action = ExternalCommandAction;
        let config = create_test_config("   ");
        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_substitute_captures() {
        let command = "echo 'Build $1 done in $2 seconds'";
        let captures = vec!["prod".to_string(), "42".to_string()];
        let result = ExternalCommandAction::substitute_captures(command, &captures);
        assert_eq!(result, "echo 'Build prod done in 42 seconds'");
    }

    #[test]
    fn test_substitute_captures_no_captures() {
        let command = "echo 'No captures'";
        let captures = vec![];
        let result = ExternalCommandAction::substitute_captures(command, &captures);
        assert_eq!(result, "echo 'No captures'");
    }

    #[test]
    fn test_check_match_with_captures() {
        let action = ExternalCommandAction;
        let pattern = r"Deployment (\w+) completed in (\d+) seconds";
        let content = "Deployment prod completed in 42 seconds";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_check_match_no_match() {
        let action = ExternalCommandAction;
        let pattern = r"Deployment (\w+)";
        let content = "No deployment here";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let action = ExternalCommandAction;
        let config = create_test_config("echo 'test'");

        let match_data = Box::new(ExternalCommandMatch { captures: vec![] });

        let result = action.execute(0, match_data, &config, 0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_with_captures() {
        let action = ExternalCommandAction;
        let config = create_test_config("echo '$1 $2'");

        let match_data = Box::new(ExternalCommandMatch {
            captures: vec!["hello".to_string(), "world".to_string()],
        });

        let result = action.execute(0, match_data, &config, 0).await;
        assert!(result.is_ok());
    }
}
