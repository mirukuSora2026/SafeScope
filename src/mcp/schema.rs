//! Flattening the schemas before they are offered.
//!
//! `schemars` describes a named type by reference: the parameter carries
//! `{"$ref": "#/$defs/RequestedOperation"}` and the definition sits in a `$defs`
//! table beside it. That is correct JSON Schema and unusable here.
//!
//! Measured against the host rather than reasoned about. A server offering
//! seven tools had all seven dropped — not the three using `$ref`, all of them:
//! the tool list arrived, and the session then had no SafeScope tools at all and
//! no way to search for any. Serving the same seven from a different process
//! reproduced it, and serving only the four without a `$ref` made those four
//! appear. One tool the client will not read costs the server every tool it has,
//! which is indistinguishable from a plugin that was never installed.
//!
//! So the references are resolved here, before anything is offered, and the
//! `$defs` table is dropped. Nothing about what the tools accept changes — an
//! inlined definition is the definition — only how it is written down.

use std::sync::Arc;

use rmcp::model::{JsonObject, Tool};
use serde_json::{Map, Value};

/// How deep a reference chain is followed before it is left alone.
///
/// A schema that refers to itself has no finite inlining, and a definition
/// nested past this is not something this crate writes. Stopping leaves the
/// `$ref` in place for that one branch rather than looping.
const MAX_DEPTH: usize = 16;

/// Rewrites every tool's schemas so they stand alone.
pub fn flatten(tool: &mut Tool) {
    tool.input_schema = Arc::new(flatten_object(&tool.input_schema));
    tool.output_schema = tool
        .output_schema
        .as_ref()
        .map(|schema| Arc::new(flatten_object(schema)));
}

/// One schema with its `$ref`s resolved and its `$defs` removed.
fn flatten_object(schema: &JsonObject) -> JsonObject {
    let definitions = schema
        .get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut flattened = match resolve(&Value::Object(schema.clone()), &definitions, 0) {
        Value::Object(object) => object,
        // Only reachable if the whole schema were a `$ref` to something that is
        // not an object, which is not a schema this crate can produce.
        _ => schema.clone(),
    };
    flattened.remove("$defs");
    flattened
}

/// Replaces `$ref`s with what they point at, everywhere in `value`.
fn resolve(value: &Value, definitions: &Map<String, Value>, depth: usize) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve(item, definitions, depth))
                .collect(),
        ),
        Value::Object(object) => {
            if depth < MAX_DEPTH
                && let Some(target) = object.get("$ref").and_then(Value::as_str)
                && let Some(definition) = lookup(target, definitions)
            {
                // Siblings of a `$ref` are kept and win over the definition: a
                // field's own description is about that field, and the shared
                // definition's is about the type.
                let mut merged = match resolve(definition, definitions, depth + 1) {
                    Value::Object(resolved) => resolved,
                    other => return other,
                };
                for (key, sibling) in object {
                    if key != "$ref" {
                        merged.insert(key.clone(), resolve(sibling, definitions, depth));
                    }
                }
                return Value::Object(merged);
            }
            Value::Object(
                object
                    .iter()
                    .map(|(key, nested)| (key.clone(), resolve(nested, definitions, depth)))
                    .collect(),
            )
        }
        scalar => scalar.clone(),
    }
}

/// The definition a local `#/$defs/Name` reference names.
///
/// Only that form is understood. A reference to another document is somebody
/// else's schema, and fetching it is not this server's business.
fn lookup<'a>(target: &str, definitions: &'a Map<String, Value>) -> Option<&'a Value> {
    definitions.get(target.strip_prefix("#/$defs/")?)
}
