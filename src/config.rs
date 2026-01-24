use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::Path;

fn default_enabled() -> bool {
    true
}

/// A rule that defines a pattern to watch for and an action to take
#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    /// Optional descriptive name for logging
    #[serde(default)]
    pub name: Option<String>,

    /// Regex pattern to match in terminal output
    pub pattern: String,

    /// Action configuration (flattened into the rule)
    #[serde(flatten)]
    pub action: crate::action::ActionConfig,

    /// Whether this rule is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Main configuration structure loaded from config.toml
///
/// This configuration controls how Wez Expect monitors WezTerm panes,
/// what patterns to detect, and what commands to send.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub monitor: MonitorConfig,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub pane: PaneConfig,
}

/// Configuration for terminal monitoring behavior
///
/// Controls how often to poll the terminal.
#[derive(Debug, Deserialize, Clone)]
pub struct MonitorConfig {
    /// How often to check terminal content for timeout messages (in seconds)
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

/// Configuration for which WezTerm pane to monitor
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PaneConfig {
    /// Optional specific pane ID to monitor
    /// If None, the first available pane will be auto-selected
    pub pane_id: Option<u32>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: default_poll_interval(),
        }
    }
}

fn default_poll_interval() -> u64 {
    3
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;

        let config: Config =
            toml::from_str(&contents).with_context(|| "Failed to parse config file as TOML")?;

        config.validate()?;
        Ok(config)
    }

    /// Load configuration from default location or create with defaults
    pub fn load_or_default() -> Result<Self> {
        let config_path = Path::new("config.toml");

        if config_path.exists() {
            Self::load(config_path)
        } else {
            tracing::warn!(
                "No config.toml found, using defaults. Create config.toml to customize."
            );
            let config = Self::default_config();
            config.validate()?;
            Ok(config)
        }
    }

    /// Create a default configuration
    fn default_config() -> Self {
        use crate::action::ActionConfig;

        Config {
            monitor: MonitorConfig::default(),
            rules: vec![Rule {
                name: Some(String::from("default_timeout")),
                pattern: String::from(
                    r#"Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\."#,
                ),
                action: ActionConfig {
                    action_type: String::from("wait_for_time"),
                    command: Some(String::from("continue")),
                    extra: toml::Value::Table(Default::default()),
                },
                enabled: true,
            }],
            pane: PaneConfig::default(),
        }
    }

    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        use crate::action::{factory::BuiltinActionFactory, ActionFactory};

        // Validate poll interval is reasonable
        if self.monitor.poll_interval_secs == 0 {
            anyhow::bail!("Poll interval must be greater than 0");
        }

        if self.monitor.poll_interval_secs > 300 {
            tracing::warn!(
                "Poll interval is very high ({}s). Consider using a lower value.",
                self.monitor.poll_interval_secs
            );
        }

        // Must have at least one rule
        if self.rules.is_empty() {
            anyhow::bail!("Config must have at least one rule defined in [[rules]]");
        }

        // Must have at least one enabled rule
        if !self.rules.iter().any(|r| r.enabled) {
            anyhow::bail!("At least one rule must be enabled");
        }

        // Create factory for action validation
        let factory = BuiltinActionFactory::new();

        // Validate each rule
        for (idx, rule) in self.rules.iter().enumerate() {
            let default_name = format!("rule #{}", idx + 1);
            let rule_name = rule.name.as_deref().unwrap_or(&default_name);

            // Validate regex compiles
            let re = Regex::new(&rule.pattern)
                .with_context(|| format!("Invalid regex in {}: {}", rule_name, rule.pattern))?;

            // Get the action for this rule
            let action = factory.create(&rule.action.action_type).ok_or_else(|| {
                anyhow::anyhow!(
                    "{} has unknown action type: '{}'",
                    rule_name,
                    rule.action.action_type
                )
            })?;

            // Validate action-specific config
            action.validate_config(&rule.action).with_context(|| {
                format!(
                    "Invalid configuration for {} ({})",
                    rule_name, rule.action.action_type
                )
            })?;

            // Validate capture group requirements
            if let Some(required_captures) = action.required_captures() {
                let num_captures = re.captures_len() - 1; // -1 for full match
                if num_captures != required_captures {
                    anyhow::bail!(
                        "{} has action '{}' but pattern has {} capture groups (need exactly {})",
                        rule_name,
                        rule.action.action_type,
                        num_captures,
                        required_captures
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionConfig;

    #[test]
    fn test_default_config_valid() {
        let config = Config::default_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_regex() {
        let mut config = Config::default_config();
        config.rules[0].pattern = String::from("(invalid[regex");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_empty_command() {
        let mut config = Config::default_config();
        config.rules[0].action.command = Some(String::from("   "));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_empty_rules() {
        let mut config = Config::default_config();
        config.rules = vec![];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_all_rules_disabled() {
        let mut config = Config::default_config();
        config.rules[0].enabled = false;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_wait_for_time_missing_captures() {
        let mut config = Config::default_config();
        config.rules[0].pattern = String::from("no captures");
        config.rules[0].action = ActionConfig {
            action_type: String::from("wait_for_time"),
            command: Some(String::from("continue")),
            extra: toml::Value::Table(Default::default()),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_immediate_action_valid() {
        let mut config = Config::default_config();
        config.rules[0].pattern = String::from("Press Enter");
        config.rules[0].action = ActionConfig {
            action_type: String::from("immediate"),
            command: Some(String::from("\n")),
            extra: toml::Value::Table(Default::default()),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_unknown_action_type() {
        let mut config = Config::default_config();
        config.rules[0].action = ActionConfig {
            action_type: String::from("unknown_action"),
            command: Some(String::from("test")),
            extra: toml::Value::Table(Default::default()),
        };
        assert!(config.validate().is_err());
    }
}
