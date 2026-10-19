//! Caller-built immutable registry of exact negotiation routes.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use worldrunner_world_model::{
    SchemaDescriptor, validate_semantic_identifier, validate_structural_uri,
};

use crate::error::RuntimeRegistryError;
use crate::handler::{CapabilityHandler, ComponentHandler, DocumentHandler, HandlerDescriptor};

/// Kinds of routes the registry carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    /// The one exact top-level document schema.
    DocumentSchema,
    /// The one handler governing the complete document contribution closure.
    DocumentRoot,
    /// Component handlers keyed by exact schema URI.
    Component,
    /// Capability handlers keyed by exact capability URI.
    Capability,
}

/// Immutable set of exact routes a [`crate::RuntimePlanner`] negotiates against.
///
/// The registry is built only through [`RuntimeRegistryBuilder`]; there is no
/// permissive or hash-only spelling, no default route, and no way to register
/// a second handler for one exact key.
pub struct RuntimeRegistry {
    document_schema: SchemaDescriptor,
    document_handler: Arc<dyn DocumentHandler>,
    component_handlers: BTreeMap<String, Arc<dyn ComponentHandler>>,
    capability_handlers: BTreeMap<String, Arc<dyn CapabilityHandler>>,
}

impl fmt::Debug for RuntimeRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeRegistry")
            .field("document_schema", &self.document_schema)
            .field("document_handler", self.document_handler.descriptor())
            .field("component_schemas", &self.component_handlers.keys())
            .field("capability_uris", &self.capability_handlers.keys())
            .finish()
    }
}

impl RuntimeRegistry {
    /// Returns the one exact top-level schema this registry accepts.
    pub fn document_schema(&self) -> &SchemaDescriptor {
        &self.document_schema
    }

    /// Returns the strict handler for the complete document root contract.
    pub fn document_handler(&self) -> Arc<dyn DocumentHandler> {
        Arc::clone(&self.document_handler)
    }

    /// Returns the component handler registered for an exact schema URI, if any.
    pub fn component_handler(&self, schema_uri: &str) -> Option<Arc<dyn ComponentHandler>> {
        self.component_handlers.get(schema_uri).cloned()
    }

    /// Returns the capability handler registered for an exact capability URI, if any.
    pub fn capability_handler(&self, capability_uri: &str) -> Option<Arc<dyn CapabilityHandler>> {
        self.capability_handlers.get(capability_uri).cloned()
    }
}

/// Builder that assembles one [`RuntimeRegistry`] from exact registrations.
///
/// Every duplicate registration fails immediately, so no partially duplicated
/// route can survive into planning.
pub struct RuntimeRegistryBuilder {
    document_schema: Option<SchemaDescriptor>,
    document_handler: Option<Arc<dyn DocumentHandler>>,
    component_handlers: BTreeMap<String, Arc<dyn ComponentHandler>>,
    capability_handlers: BTreeMap<String, Arc<dyn CapabilityHandler>>,
}

impl Default for RuntimeRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeRegistryBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self {
            document_schema: None,
            document_handler: None,
            component_handlers: BTreeMap::new(),
            capability_handlers: BTreeMap::new(),
        }
    }

    /// Registers the one exact top-level schema the registry will accept.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeRegistryError::DuplicateHandler`] when a schema was
    /// already registered.
    pub fn document_schema(
        &mut self,
        tag: &str,
        version: u32,
    ) -> Result<&mut Self, RuntimeRegistryError> {
        validate_route(HandlerKind::DocumentSchema, tag)?;
        if self.document_schema.is_some() {
            return Err(RuntimeRegistryError::DuplicateHandler {
                handler_kind: HandlerKind::DocumentSchema,
                key: tag.to_owned(),
            });
        }
        self.document_schema = Some(SchemaDescriptor {
            tag: tag.to_owned(),
            version,
        });
        Ok(self)
    }

    /// Registers the one mandatory strict document-root handler.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeRegistryError::DuplicateHandler`] when a root handler
    /// was already registered, or
    /// [`RuntimeRegistryError::InvalidHandlerDescriptor`] for an invalid identity.
    pub fn document_handler(
        &mut self,
        handler: Arc<dyn DocumentHandler>,
    ) -> Result<&mut Self, RuntimeRegistryError> {
        validate_descriptor(handler.descriptor())?;
        if self.document_handler.is_some() {
            return Err(RuntimeRegistryError::DuplicateHandler {
                handler_kind: HandlerKind::DocumentRoot,
                key: "document-root".to_owned(),
            });
        }
        self.document_handler = Some(handler);
        Ok(self)
    }

    /// Registers a component handler for one exact schema URI.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeRegistryError::DuplicateHandler`] when the schema URI
    /// already carries a handler, or
    /// [`RuntimeRegistryError::InvalidHandlerDescriptor`] when the handler's
    /// identity is unusable.
    pub fn component_handler(
        &mut self,
        schema_uri: &str,
        handler: Arc<dyn ComponentHandler>,
    ) -> Result<&mut Self, RuntimeRegistryError> {
        validate_descriptor(handler.descriptor())?;
        validate_route(HandlerKind::Component, schema_uri)?;
        if self.component_handlers.contains_key(schema_uri) {
            return Err(RuntimeRegistryError::DuplicateHandler {
                handler_kind: HandlerKind::Component,
                key: schema_uri.to_owned(),
            });
        }
        self.component_handlers
            .insert(schema_uri.to_owned(), handler);
        Ok(self)
    }

    /// Registers a capability handler for one exact capability URI.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeRegistryError::DuplicateHandler`] when the capability
    /// URI already carries a handler, or
    /// [`RuntimeRegistryError::InvalidHandlerDescriptor`] when the handler's
    /// identity is unusable.
    pub fn capability_handler(
        &mut self,
        capability_uri: &str,
        handler: Arc<dyn CapabilityHandler>,
    ) -> Result<&mut Self, RuntimeRegistryError> {
        validate_descriptor(handler.descriptor())?;
        validate_route(HandlerKind::Capability, capability_uri)?;
        if self.capability_handlers.contains_key(capability_uri) {
            return Err(RuntimeRegistryError::DuplicateHandler {
                handler_kind: HandlerKind::Capability,
                key: capability_uri.to_owned(),
            });
        }
        self.capability_handlers
            .insert(capability_uri.to_owned(), handler);
        Ok(self)
    }

    /// Freezes the registrations into one immutable registry.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeRegistryError::MissingDocumentSchema`] when no exact
    /// top-level schema was registered.
    pub fn build(self) -> Result<RuntimeRegistry, RuntimeRegistryError> {
        let document_schema = self
            .document_schema
            .ok_or(RuntimeRegistryError::MissingDocumentSchema)?;
        let document_handler = self
            .document_handler
            .ok_or(RuntimeRegistryError::MissingDocumentHandler)?;
        Ok(RuntimeRegistry {
            document_schema,
            document_handler,
            component_handlers: self.component_handlers,
            capability_handlers: self.capability_handlers,
        })
    }
}

fn validate_descriptor(descriptor: &HandlerDescriptor) -> Result<(), RuntimeRegistryError> {
    validate_semantic_identifier(&descriptor.handler_id).map_err(|_| {
        RuntimeRegistryError::InvalidHandlerDescriptor {
            handler_id: descriptor.handler_id.clone(),
        }
    })
}

fn validate_route(handler_kind: HandlerKind, key: &str) -> Result<(), RuntimeRegistryError> {
    validate_structural_uri(key).map_err(|source| RuntimeRegistryError::InvalidRoutingKey {
        handler_kind,
        key: key.to_owned(),
        source,
    })
}
