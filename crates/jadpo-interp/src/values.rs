//! The plan's values on one thread: its constants and shapes decoded into
//! the runtime's `Rc`-shared values, once per thread (values never cross
//! threads, §2.6), and the codecs that build a value of a type from its
//! parts the way a body builds one.

use jadpo_graph::TypeRef;
use jadpo_plan::{ShapePlan, Types};
use jadpo_value::Value;
pub(crate) use jadpo_value::shape::Shape;

/// Values of a project's types built from their parts, as its bodies build
/// them: for tests and tools that pass a callable arguments.
#[derive(Clone, Copy, Debug)]
pub struct Codec<'a> {
    pub(crate) types: &'a Types,
}

impl Codec<'_> {
    /// An object of type `ty` from every one of its fields by name; `None`
    /// when `ty` isn't an object or a field is missing or unknown.
    pub fn object(&self, ty: &str, fields: Vec<(&str, Value)>) -> Option<Value> {
        let key = self.types.named(ty)?;
        self.build(&self.types.shape_plan(&key)?, fields)
    }

    /// Variant `variant` of enum `ty`, with a payload variant's fields by
    /// name.
    pub fn variant(&self, ty: &str, variant: &str, fields: Vec<(&str, Value)>) -> Option<Value> {
        let key = self.types.variant_named(ty, variant);
        self.types.graph().get(&key)?;
        self.build(&self.types.shape_plan(&key)?, fields)
    }

    /// A value of the named type `ty` from `text`, as a literal of it would
    /// be: a UUID, an instant or a date from its ISO text, text otherwise.
    pub fn text(&self, ty: &str, text: &str) -> Option<Value> {
        let key = self.types.named(ty)?;
        self.types
            .text(&TypeRef::Node(key), text)
            .map(|constant| Value::constant(&constant))
    }

    fn build(&self, plan: &ShapePlan, fields: Vec<(&str, Value)>) -> Option<Value> {
        if fields.len() != plan.fields.len() {
            return None;
        }
        let given = fields
            .into_iter()
            .map(|(name, value)| Some((Some(plan.slot_named(name)?), value)))
            .collect::<Option<Vec<_>>>()?;
        Shape::new(plan).build(given)
    }
}
