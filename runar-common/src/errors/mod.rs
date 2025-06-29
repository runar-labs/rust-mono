// Error utilities for runar_common

// Use standard error utilities from third-party libraries
pub use anyhow::{anyhow, Result};
pub use thiserror::Error;

// Export common error utilities
pub mod utils {
    use crate::types::ArcValue;

    /// Convert an error to a string value
    pub fn error_to_string_value(error: impl std::fmt::Display) -> ArcValue {
        // Just use the error message as a string for simplicity
        let error_message = error.to_string();

        // Return as string value
        ArcValue::new_primitive(error_message)
    }
}
