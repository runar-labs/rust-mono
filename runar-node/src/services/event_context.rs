// EventContext Module
//
// INTENTION:
// This module provides the implementation of EventContext, which encapsulates
// all contextual information needed to process service events, including
// the event topic, logging capabilities, and access to node functionality.
//
// ARCHITECTURAL PRINCIPLE:
// Each event handler should have its own isolated context that provides
// the necessary information and functionality to process the event,
// while maintaining proper isolation between events.

use crate::node::Node; // Added for concrete type
use crate::routing::TopicPath;
use crate::services::PublishOptions; // Restored
use crate::NodeDelegate; // Keep one instance
use anyhow::Result;
use runar_common::logging::{Component, Logger, LoggingContext}; // Restored
use runar_common::types::ArcValue;
use runar_common::types::AsArcValue; // Corrected: Only AsArcValue needed here
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;

/// Context for handling events
///
/// INTENTION: Provide context information for event handlers, allowing them
/// to access event details, logging, and perform operations like
/// publishing other events or making service requests.
///
/// ARCHITECTURAL PRINCIPLE:
/// Event handlers should have access to necessary context for performing
/// their operations, but with appropriate isolation and scope control.
/// The EventContext provides a controlled interface to the node's capabilities.
pub struct EventContext {
    /// Complete topic path for this event
    pub topic_path: TopicPath,

    /// Logger instance specific to this context
    pub logger: Arc<Logger>,

    /// Node delegate for making requests or publishing events
    pub(crate) node_delegate: Arc<Node>,

    /// Delivery options used when publishing this event
    pub delivery_options: Option<PublishOptions>,
}

impl fmt::Debug for EventContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventContext")
            .field("topic_path", &self.topic_path)
            .field("logger", &"<Logger>") // Avoid trying to Debug the Logger
            .field("delivery_options", &self.delivery_options)
            .finish()
    }
}

// Manual implementation of Clone for EventContext
impl Clone for EventContext {
    fn clone(&self) -> Self {
        panic!("EventContext should not be cloned directly. Use new instead");
    }
}

// Manual implementation of Default for EventContext
impl Default for EventContext {
    fn default() -> Self {
        panic!("EventContext should not be created with default. Use new instead");
    }
}

/// Constructors follow the builder pattern principle:
/// - Prefer a single primary constructor with required parameters
/// - Use builder methods for optional parameters
/// - Avoid creating specialized constructors for every parameter combination
impl EventContext {
    /// Create a new EventContext with the given topic path and logger
    ///
    /// This is the primary constructor that takes the minimum required parameters.
    pub fn new(topic_path: &TopicPath, node_delegate: Arc<Node>, logger: Arc<Logger>) -> Self {
        // Add event path to logger if available from topic_path
        let event_path = topic_path.action_path();
        let event_logger = if !event_path.is_empty() {
            // If there's an event path, add it to the logger
            Arc::new(logger.with_event_path(event_path))
        } else {
            logger
        };

        Self {
            topic_path: topic_path.clone(),
            logger: event_logger,
            node_delegate,
            delivery_options: None,
        }
    }

    /// Add node delegate to an EventContext
    ///
    /// Used to make service requests from within an event handler.
    pub fn with_node_delegate(mut self, delegate: Arc<Node>) -> Self {
        self.node_delegate = delegate;
        self
    }

    /// Add delivery options to an EventContext
    ///
    /// Used to specify how this event was delivered.
    pub fn with_delivery_options(mut self, options: PublishOptions) -> Self {
        self.delivery_options = Some(options);
        self
    }

    /// Helper method to log debug level message
    pub fn debug(&self, message: impl Into<String>) {
        self.logger.debug(message);
    }

    /// Helper method to log info level message
    pub fn info(&self, message: impl Into<String>) {
        self.logger.info(message);
    }

    /// Helper method to log warning level message
    pub fn warn(&self, message: impl Into<String>) {
        self.logger.warn(message);
    }

    /// Helper method to log error level message
    pub fn error(&self, message: impl Into<String>) {
        self.logger.error(message);
    }

    /// Publish an event
    ///
    /// INTENTION: Allow event handlers to publish their own events.
    /// This method provides a convenient way to publish events from within
    /// an event handler.
    ///
    /// Handles different path formats:
    /// - Full path with network ID: "network:service/topic" (used as is)
    /// - Path with service: "service/topic" (network ID added)
    /// - Simple topic: "topic" (both service path and network ID added)
    pub async fn publish(&self, topic: impl Into<String>, data: Option<ArcValue>) -> Result<()> {
        let topic_string = topic.into();

        // Process the topic based on its format
        let full_topic = if topic_string.contains(':') {
            // Already has network ID, use as is
            topic_string
        } else if topic_string.contains('/') {
            // Has service/topic but no network ID
            format!(
                "{network_id}:{topic_string}",
                network_id = self.topic_path.network_id()
            )
        } else {
            // Simple topic name - add service path and network ID
            format!(
                "{}:{}/{}",
                self.topic_path.network_id(),
                self.topic_path.service_path(),
                topic_string
            )
        };

        self.logger
            .debug(format!("Publishing to processed topic: {full_topic}"));
        self.node_delegate.publish(full_topic, data).await
    }

    /// Make a service request
    ///
    /// INTENTION: Allow event handlers to make requests to other services.
    /// This method provides a convenient way to call service actions from
    /// within an event handler.
    ///
    /// Handles different path formats:
    /// - Full path with network ID: "network:service/action" (used as is)
    /// - Path with service: "service/action" (network ID added)
    /// - Simple action: "action" (both service path and network ID added - calls own service)
    pub async fn request<P, T>(&self, path: impl Into<String>, payload: Option<P>) -> Result<T>
    where
        P: AsArcValue + Send + Sync,
        T: 'static + Send + Sync + Clone + Debug + for<'de> serde::Deserialize<'de>,
    {
        let path_string = path.into();

        // Process the path based on its format
        let full_path = if path_string.contains(':') {
            // Already has network ID, use as is
            path_string
        } else if path_string.contains('/') {
            // Has service/action but no network ID
            format!(
                "{network_id}:{path_string}",
                network_id = self.topic_path.network_id()
            )
        } else {
            // Simple action name - add both service path and network ID
            format!(
                "{}:{}/{}",
                self.topic_path.network_id(),
                self.topic_path.service_path(),
                path_string
            )
        };

        self.logger
            .debug(format!("Making request to processed path: {full_path}"));

        // Call Node::request, specifying the generic types P and T.
        // Node::request itself will handle deserialization to T.
        self.node_delegate.request::<P, T>(full_path, payload).await
    }
}

impl LoggingContext for EventContext {
    fn component(&self) -> Component {
        Component::Service
    }

    fn service_path(&self) -> Option<&str> {
        // Convert the owned String to a string slice that lives as long as self
        let path = self.topic_path.service_path();
        Some(Box::leak(path.into_boxed_str()))
    }

    fn event_path(&self) -> Option<&str> {
        // Get from logger's event_path
        self.logger.event_path()
    }

    fn logger(&self) -> &Logger {
        &self.logger
    }
}
