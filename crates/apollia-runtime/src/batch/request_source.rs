//! A source of one item, given whole in the launch itself.
//!
//! An agent asked something by a single request, such as a call of the
//! compatible server, runs as a batch of one item so it follows the rule of
//! every run nobody watches: an access nobody granted makes the item wait for
//! a decision instead of failing. The item is the launch's `options`, one field
//! each; `location` only names where the request came from.

use apollia_core::batch_source::{
    BatchSource, OriginRef, SourceError, SourceItem, SourceItems, SourceRegistry, SourceSpec,
};

/// The kind under which this source is registered.
pub const REQUEST_SOURCE_KIND: &str = "request";

/// The origin of the one item.
const ORIGIN: &str = "request";

/// One item, whose fields are the options of the launch.
#[derive(Debug, Clone)]
pub struct RequestSource {
    fields: Vec<String>,
    item: SourceItem,
}

impl RequestSource {
    /// The item `spec` carries in its options.
    pub fn open(spec: &SourceSpec) -> Self {
        Self {
            fields: spec.options.keys().cloned().collect(),
            item: SourceItem {
                origin: OriginRef::new(ORIGIN),
                fields: spec.options.clone(),
            },
        }
    }
}

impl BatchSource for RequestSource {
    fn fields(&self) -> &[String] {
        &self.fields
    }

    fn items(&self) -> Result<SourceItems<'_>, SourceError> {
        Ok(SourceItems {
            estimated_count: 1,
            items: Box::new(std::iter::once(Ok(self.item.clone()))),
        })
    }
}

/// Registers [`REQUEST_SOURCE_KIND`] in `registry`.
///
/// # Errors
///
/// [`SourceError::DuplicateKind`] when the kind is already registered.
pub fn register_request_source(registry: &mut SourceRegistry) -> Result<(), SourceError> {
    registry.register(
        REQUEST_SOURCE_KIND,
        Box::new(|spec| Ok(Box::new(RequestSource::open(spec)) as Box<dyn BatchSource>)),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn test_a_request_source_yields_its_options_as_one_item() {
        // GIVEN a registry holding the request source, and a launch carrying
        // a request and the conversation before it
        let mut registry = SourceRegistry::new();
        register_request_source(&mut registry).unwrap();
        let spec = SourceSpec {
            location: "compatible server".into(),
            options: BTreeMap::from([
                ("conversation".to_owned(), "user: hello".to_owned()),
                ("request".to_owned(), "Qualify this demand.".to_owned()),
            ]),
        };

        // WHEN it is opened and read
        let source = registry.open(REQUEST_SOURCE_KIND, &spec).unwrap();
        let items: Vec<SourceItem> = source
            .items()
            .unwrap()
            .items
            .collect::<Result<_, _>>()
            .unwrap();

        // THEN it holds one item carrying both fields
        assert_eq!(source.fields(), ["conversation", "request"]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields, spec.options);
    }
}
