use super::{Action, ActionFactory};
use std::collections::HashMap;

/// Built-in action factory that manages built-in actions
pub struct BuiltinActionFactory {
    /// Map of action type names to factory functions
    factories: HashMap<String, Box<dyn Fn() -> Box<dyn Action> + Send + Sync>>,
}

impl BuiltinActionFactory {
    /// Creates a new factory with all built-in actions registered
    pub fn new() -> Self {
        let mut factory = Self {
            factories: HashMap::new(),
        };

        // Register built-in actions
        factory.register(
            "wait_for_time",
            Box::new(|| Box::new(super::builtin::WaitForTimeAction) as Box<dyn Action>),
        );
        factory.register(
            "immediate",
            Box::new(|| Box::new(super::builtin::ImmediateAction) as Box<dyn Action>),
        );
        factory.register(
            "conditional",
            Box::new(|| Box::new(super::builtin::ConditionalAction) as Box<dyn Action>),
        );
        factory.register(
            "external_command",
            Box::new(|| Box::new(super::builtin::ExternalCommandAction) as Box<dyn Action>),
        );

        factory
    }

    /// Registers a new action type with a factory function
    fn register<F>(&mut self, action_type: &str, factory_fn: Box<F>)
    where
        F: Fn() -> Box<dyn Action> + Send + Sync + 'static,
    {
        self.factories
            .insert(action_type.to_string(), factory_fn);
    }

}

impl ActionFactory for BuiltinActionFactory {
    fn create(&self, action_type: &str) -> Option<Box<dyn Action>> {
        self.factories.get(action_type).map(|f| f())
    }

    fn supported_types(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

impl Default for BuiltinActionFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creates_builtin_actions() {
        let factory = BuiltinActionFactory::new();

        assert!(factory.create("wait_for_time").is_some());
        assert!(factory.create("immediate").is_some());
        assert!(factory.create("conditional").is_some());
        assert!(factory.create("external_command").is_some());
        assert!(factory.create("nonexistent").is_none());
    }

    #[test]
    fn test_supported_types() {
        let factory = BuiltinActionFactory::new();
        let types = factory.supported_types();

        assert!(types.contains(&"wait_for_time".to_string()));
        assert!(types.contains(&"immediate".to_string()));
        assert!(types.contains(&"conditional".to_string()));
        assert!(types.contains(&"external_command".to_string()));
    }
}
