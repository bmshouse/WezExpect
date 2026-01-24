pub mod builtin;
pub mod factory;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::any::Any;

/// Core trait that all actions must implement
///
/// Actions are responsible for:
/// - Validating their configuration
/// - Checking if terminal content matches their pattern
/// - Executing when a match is found
///
/// # Match Data Pattern
///
/// This trait uses `Box<dyn Any + Send>` for passing match data between
/// `check_match()` and `execute()`. This pattern enables type-safe communication
/// while maintaining trait object compatibility.
///
/// Each action implementation:
/// 1. Defines its own match data struct (e.g., `WaitForTimeMatch`)
/// 2. Returns it boxed from `check_match()` using `Box::new(match_data)`
/// 3. Downcasts it in `execute()` using `.downcast::<MatchDataType>()`
///
/// While this requires runtime type checking via downcasting, it's a pragmatic
/// trade-off that enables:
/// - Trait object compatibility
/// - Type safety within each action implementation
/// - Flexibility for action-specific match data structures
///
/// Alternative approaches like Generic Associated Types (GATs) would provide
/// stronger compile-time guarantees but are incompatible with trait objects.
#[async_trait]
pub trait Action: Send + Sync {
    /// Returns the action type identifier (e.g., "wait_for_time", "immediate")
    fn action_type(&self) -> &str;

    /// Validates the action-specific configuration
    ///
    /// Called during config loading to ensure all required fields are present
    /// and valid for this action type.
    fn validate_config(&self, config: &ActionConfig) -> Result<()>;

    /// Checks if the terminal content matches this action's pattern
    ///
    /// Returns Some(Box<dyn Any>) with action-specific match data if there's a match,
    /// or None if no match. The match data is passed to execute() later.
    fn check_match(
        &self,
        content: &str,
        pattern: &str,
    ) -> Result<Option<Box<dyn Any + Send>>>;

    /// Executes the action using the match data from check_match
    ///
    /// The match_data parameter contains action-specific data extracted
    /// during check_match (e.g., captured groups, extracted time info, etc.)
    async fn execute(
        &self,
        pane_id: u32,
        match_data: Box<dyn Any + Send>,
        config: &ActionConfig,
    ) -> Result<()>;

    /// Returns the required number of capture groups, if applicable
    ///
    /// Some actions like wait_for_time require specific capture groups.
    /// Returns None if there are no capture group requirements.
    fn required_captures(&self) -> Option<usize> {
        None
    }

    /// Optional post-execution hook
    ///
    /// Called after execute() completes successfully. Useful for cleanup
    /// or actions that need to wait for content changes (e.g., immediate action).
    async fn post_execute(&self, _pane_id: u32) -> Result<()> {
        Ok(())
    }
}

/// Configuration for an action, loaded from TOML
///
/// The `flatten` attribute allows action-specific fields to be defined
/// in the TOML without hardcoding them in this struct.
#[derive(Debug, Clone, Deserialize)]
pub struct ActionConfig {
    /// The type of action (e.g., "wait_for_time", "immediate", "conditional")
    pub action_type: String,

    /// Command to send to the terminal (used by most actions)
    #[serde(default)]
    pub command: Option<String>,

    /// Action-specific configuration fields
    ///
    /// These are flattened into the TOML, allowing each action type
    /// to define its own required fields without modifying this struct.
    #[serde(flatten)]
    pub extra: toml::Value,
}

/// Factory trait for creating action instances
///
/// Decouples action registration from instantiation.
pub trait ActionFactory: Send + Sync {
    /// Creates an action instance for the given action type
    ///
    /// Returns None if the action type is not supported by this factory.
    fn create(&self, action_type: &str) -> Option<Box<dyn Action>>;

    /// Returns a list of all action types supported by this factory
    fn supported_types(&self) -> Vec<String>;
}
