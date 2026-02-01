use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::Path;

fn default_enabled() -> bool {
    true
}

/// A rule that defines a pattern to watch for and an action to take.
///
/// Rules are processed in order - the first enabled rule that matches wins.
#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    /// Optional descriptive name for this rule (used in log messages for identification)
    #[serde(default)]
    pub name: Option<String>,

    /// Regex pattern to match against terminal output
    pub pattern: String,

    /// Action configuration (flattened into the rule TOML)
    ///
    /// This includes the `action_type` field and any action-specific fields
    /// like `command`, `condition_pattern`, etc.
    #[serde(flatten)]
    pub action: crate::action::ActionConfig,

    /// Whether this rule is enabled (defaults to true)
    ///
    /// Disabled rules are skipped during pattern matching
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

/// Configuration for terminal monitoring behavior.
///
/// Controls polling frequency, timing delays, and duplicate prevention.
#[derive(Debug, Deserialize, Clone)]
pub struct MonitorConfig {
    /// How often to check terminal content for pattern matches (in seconds, default: 3)
    ///
    /// Lower values = faster response but higher CPU usage
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,

    /// Delay before sending commands to terminal (in milliseconds, default: 100)
    ///
    /// Ensures the terminal application is ready to receive input and prevents
    /// dropped keystrokes, especially important for interactive applications like Python REPLs
    #[serde(default = "default_send_delay_ms")]
    pub send_delay_ms: u64,

    /// Minimum time between pattern matches (in seconds, default: 60)
    ///
    /// Provides time-based duplicate prevention. After a pattern matches and action executes,
    /// this cooldown period prevents re-matching even if content changes. Works independently
    /// of content-hash duplicate prevention.
    #[serde(default = "default_match_cooldown_secs")]
    pub match_cooldown_secs: u64,

    /// Number of lines to retrieve from terminal history when checking patterns (default: 50)
    ///
    /// Prevents processing the entire terminal scrollback on each poll. Increase if patterns
    /// appear further back in history, decrease to avoid re-matching old patterns.
    #[serde(default = "default_lookback_lines")]
    pub lookback_lines: u32,
}

/// Configuration for which WezTerm pane to monitor.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PaneConfig {
    /// Specific pane ID to monitor (optional)
    ///
    /// If not specified, the first available pane will be auto-selected.
    /// Pane IDs can be found using `wezterm cli list`.
    pub pane_id: Option<u32>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: default_poll_interval(),
            send_delay_ms: default_send_delay_ms(),
            match_cooldown_secs: default_match_cooldown_secs(),
            lookback_lines: default_lookback_lines(),
        }
    }
}

fn default_poll_interval() -> u64 {
    3
}

fn default_send_delay_ms() -> u64 {
    100
}

fn default_match_cooldown_secs() -> u64 {
    60
}

fn default_lookback_lines() -> u32 {
    50
}

impl Config {
    /// Load configuration from a TOML file
    ///
    /// # Examples
    /// ```no_run
    /// use wez_expect::Config;
    ///
    /// // Load from default config file
    /// let config = Config::load("config.toml")?;
    ///
    /// // Load from custom path
    /// let config = Config::load("/path/to/my-config.toml")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;

        let config: Config =
            toml::from_str(&contents).map_err(|e| Self::enrich_toml_error(&e, &contents))?;

        config.validate()?;
        Ok(config)
    }

    /// Enriches TOML parsing errors with helpful context
    fn enrich_toml_error(error: &toml::de::Error, contents: &str) -> anyhow::Error {
        let error_msg = error.to_string();

        // Identify which rule is problematic based on line number
        if let Some(line_col) = error.span() {
            // Convert byte offset to 1-based line number
            let line_num = contents[..line_col.start].lines().count() + 1;

            // Find all [[rules]] sections and their line numbers
            let mut rule_sections = Vec::new();
            for (idx, line) in contents.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed == "[[rules]]" {
                    rule_sections.push(idx + 1); // Convert to 1-based line number
                }
            }

            // Determine which rule the error line belongs to
            let mut current_rule = 0;
            let mut rule_start_line = 0;

            for (rule_idx, &rules_line) in rule_sections.iter().enumerate() {
                if line_num >= rules_line {
                    current_rule = rule_idx + 1; // 1-based rule number
                    rule_start_line = rules_line;
                } else {
                    break;
                }
            }

            // Extract rule name if available
            let rule_name = if current_rule > 0 {
                Self::extract_rule_name(contents, rule_start_line)
            } else {
                None
            };

            let rule_identifier = if let Some(name) = rule_name {
                format!("rule #{} (\"{}\")", current_rule, name)
            } else if current_rule > 0 {
                format!("rule #{}", current_rule)
            } else {
                "configuration".to_string()
            };

            // Provide specific guidance based on error type
            if error_msg.contains("missing field `action_type`") {
                return anyhow::anyhow!(
                    "Failed to parse config file\n\n\
                    Error in {}: missing required field 'action_type'\n\n\
                    Valid action types are:\n\
                    - \"wait_for_time\" - Extract time from captures and wait until that time\n\
                    - \"immediate\" - Send command instantly when pattern matches\n\
                    - \"conditional\" - Test captures against conditions and send different commands\n\
                    - \"external_command\" - Execute shell commands with capture substitution\n\n\
                    Common mistake: Using 'action' instead of 'action_type'\n\n\
                    Example:\n\
                    [[rules]]\n\
                    name = \"my_rule\"\n\
                    pattern = \"some pattern\"\n\
                    action_type = \"immediate\"  # Must be 'action_type', not 'action'\n\
                    command = \"\\n\"\n\n\
                    See line {} in your config file",
                    rule_identifier, line_num
                );
            } else if error_msg.contains("unknown field") {
                return anyhow::anyhow!(
                    "Failed to parse config file\n\n\
                    Error in {}: {}\n\n\
                    This might be a typo or an unsupported field.\n\
                    Check the documentation for valid fields for each action type.\n\n\
                    See line {} in your config file",
                    rule_identifier,
                    error_msg,
                    line_num
                );
            } else if error_msg.contains("invalid type") {
                return anyhow::anyhow!(
                    "Failed to parse config file\n\n\
                    Error in {}: {}\n\n\
                    Check that values match expected types:\n\
                    - Strings should be in quotes: \"value\"\n\
                    - Numbers should not be in quotes: 123\n\
                    - Booleans should be: true or false\n\n\
                    See line {} in your config file",
                    rule_identifier,
                    error_msg,
                    line_num
                );
            } else {
                return anyhow::anyhow!(
                    "Failed to parse config file\n\n\
                    Error in {}: {}\n\n\
                    See line {} in your config file",
                    rule_identifier,
                    error_msg,
                    line_num
                );
            }
        }

        anyhow::anyhow!("Failed to parse config file as TOML\n\n{}", error_msg)
    }

    /// Extracts the rule name from config contents starting at a given line
    fn extract_rule_name(contents: &str, start_line: usize) -> Option<String> {
        let lines: Vec<&str> = contents.lines().collect();

        // Look for name = "..." in the next few lines after [[rules]]
        for line in lines.iter().skip(start_line).take(10).map(|s| s.trim()) {
            // Stop if we hit another [[rules]] section
            if line == "[[rules]]" {
                break;
            }

            // Look for name = "value"
            if line.starts_with("name") {
                if let Some(eq_pos) = line.find('=') {
                    let value_part = line[eq_pos + 1..].trim();
                    // Extract value between quotes
                    if let Some(stripped) = value_part.strip_prefix('"') {
                        if let Some(end_quote) = stripped.find('"') {
                            return Some(stripped[..end_quote].to_string());
                        }
                    } else if let Some(stripped) = value_part.strip_prefix('\'') {
                        if let Some(end_quote) = stripped.find('\'') {
                            return Some(stripped[..end_quote].to_string());
                        }
                    }
                }
            }
        }

        None
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

        // Validate send delay is reasonable
        if self.monitor.send_delay_ms > 5000 {
            tracing::warn!(
                "Send delay is very high ({}ms). This may cause noticeable lag when sending commands.",
                self.monitor.send_delay_ms
            );
        }

        // Validate match cooldown is reasonable
        if self.monitor.match_cooldown_secs > 3600 {
            tracing::warn!(
                "Match cooldown is very high ({}s). Patterns won't re-match for over an hour.",
                self.monitor.match_cooldown_secs
            );
        }

        // Validate lookback lines is reasonable
        if self.monitor.lookback_lines == 0 {
            anyhow::bail!("Lookback lines must be greater than 0");
        }

        if self.monitor.lookback_lines > 10000 {
            tracing::warn!(
                "Lookback lines is very high ({}). This may impact performance and memory usage.",
                self.monitor.lookback_lines
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
            let re = Regex::new(&rule.pattern).with_context(|| {
                format!(
                    "Invalid regex pattern in {}\n\
                    Pattern: {}\n\n\
                    Check for:\n\
                    - Unmatched parentheses or brackets\n\
                    - Invalid escape sequences\n\
                    - Unclosed character classes [...]",
                    rule_name, rule.pattern
                )
            })?;

            // Get the action for this rule
            let action = factory.create(&rule.action.action_type).ok_or_else(|| {
                let supported = factory.supported_types();
                anyhow::anyhow!(
                    "{} has unknown action_type: '{}'\n\n\
                    Valid action types are:\n{}\n\n\
                    Did you mean one of these?",
                    rule_name,
                    rule.action.action_type,
                    supported
                        .iter()
                        .map(|t| format!("  - \"{}\"", t))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            })?;

            // Validate action-specific config
            action.validate_config(&rule.action).with_context(|| {
                format!(
                    "Invalid configuration for {} (action_type: '{}')\n\
                    Check the documentation for required fields for this action type",
                    rule_name, rule.action.action_type
                )
            })?;

            // Validate capture group requirements
            if let Some(required_captures) = action.required_captures() {
                let num_captures = re.captures_len() - 1; // -1 for full match
                if num_captures != required_captures {
                    anyhow::bail!(
                        "{} has action_type '{}' which requires exactly {} capture group(s),\n\
                        but the pattern has {} capture group(s)\n\n\
                        Pattern: {}\n\n\
                        Tip: Capture groups are defined with parentheses: (pattern)\n\
                        Count the number of '(' in your pattern (excluding non-capturing groups '(?:...')",
                        rule_name,
                        rule.action.action_type,
                        required_captures,
                        num_captures,
                        rule.pattern
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

    #[test]
    fn test_default_lookback_lines() {
        let config = Config::default_config();
        assert_eq!(config.monitor.lookback_lines, 50);
    }

    #[test]
    fn test_lookback_lines_zero_invalid() {
        let mut config = Config::default_config();
        config.monitor.lookback_lines = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Lookback lines must be greater than 0"));
    }

    #[test]
    fn test_lookback_lines_custom_value() {
        let mut config = Config::default_config();
        config.monitor.lookback_lines = 100;
        assert!(config.validate().is_ok());
        assert_eq!(config.monitor.lookback_lines, 100);
    }

    #[test]
    fn test_lookback_lines_from_toml() {
        let toml_str = r#"
            [monitor]
            poll_interval_secs = 5
            lookback_lines = 200

            [[rules]]
            pattern = "test pattern (.*)"
            action_type = "immediate"
            command = "test"
        "#;

        let config: Config = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.monitor.lookback_lines, 200);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_lookback_lines_default_when_omitted() {
        let toml_str = r#"
            [monitor]
            poll_interval_secs = 5

            [[rules]]
            pattern = "test pattern (.*)"
            action_type = "immediate"
            command = "test"
        "#;

        let config: Config = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.monitor.lookback_lines, 50); // Should use default
        assert!(config.validate().is_ok());
    }
}
