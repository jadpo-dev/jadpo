//! What compiled code's generated readers call (chunk 8.1p;
//! docs/compiler-runtime.md §2.5, "A typed per-route entry"). A reader
//! generated from a codec walks an [`Item`] in the codec's order and calls
//! these for every rule, each the one [`Codecs::read`] follows: whether a
//! field is missing, `null` or read ([`super::presence`]), a scalar read
//! and checked ([`super::scalar`], [`Codecs::check`]), a list's or a set's
//! items ([`super::collected`]), the members an object doesn't declare,
//! and the value built by its shape. So what it reads, and every problem
//! it finds, field and rule, in order, is what the codec walk finds.
//!
//! A problem's place is an [`At`], a chain of borrowed steps on the
//! stack, written out only when a problem is reported, so reading a
//! request with no problems makes no path. A codec a reader doesn't walk
//! itself is handed to [`Codecs::read`] at that part of the document
//! ([`Reading::generic`]), with its path and the document's numbers.

use std::rc::Rc;

use jadpo_std::Builtin;
use jadpo_std::http::CodecId;
use jadpo_std::types::ValidatorId;

use super::doc::{Item, Items, Kind, Members};
use super::{Codecs, Numbers, Path, Presence, Problem};
use crate::Value;

/// Where in the input a value is: the top, a member of an object, or an
/// item of a list, each step borrowing the one before.
#[derive(Clone, Copy, Debug)]
pub enum At<'a> {
    Top,
    Member(&'a At<'a>, &'a str),
    Item(&'a At<'a>, usize),
}

impl At<'_> {
    /// Its steps from the top: members by name, items by their place.
    pub fn parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        self.push_parts(&mut parts);
        parts
    }

    fn push_parts(&self, parts: &mut Vec<String>) {
        match self {
            At::Top => {}
            At::Member(up, name) => {
                up.push_parts(parts);
                parts.push(name.to_string());
            }
            At::Item(up, place) => {
                up.push_parts(parts);
                parts.push(place.to_string());
            }
        }
    }

    /// As a problem names it: `lines.2.quantity`.
    pub fn shown(&self) -> String {
        self.parts().join(".")
    }
}

/// How a field of an object is read when its member is missing or `null`
/// (area 2): whether a value built without it holds something, whether
/// its shape is a provider's, open, and its type nullable, whether it is
/// an optional `Input` field without a default, and whether its type is a
/// list or a set, which a query string gives as a repeated key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub absent: bool,
    pub open_nullable: bool,
    pub omittable: bool,
    pub list: bool,
}

/// What an object's field is given.
#[derive(Debug)]
pub enum Got<'i> {
    /// Its member, to read.
    Read(Item<'i>),
    /// `none`, without reading anything.
    None,
    /// Nothing: left to what it holds when absent, or missing.
    Left,
}

/// A value of a type read by a generated reader, from an item at a place.
pub type Reader = fn(&mut Reading<'_>, Item<'_>, &At<'_>) -> Option<Value>;

/// A placeholder's value read from its text (area 7, "Paths").
pub type TextReader = fn(&mut Reading<'_>, &str, &At<'_>) -> Option<Value>;

/// A route's input read from its members, headers' fields among them.
pub type InputReader = fn(&mut Reading<'_>, &Members<'_>) -> Option<Value>;

/// A value of a type written as JSON by a generated writer.
pub type Writer = fn(&Codecs<'_>, &Value, &mut String);

/// One request's reading: the program's codecs, with its validators and
/// shapes, and the problems found so far, in order.
#[derive(Debug)]
pub struct Reading<'c> {
    codecs: Codecs<'c>,
    problems: Vec<Problem>,
    /// The document's numbers, made once the first time a codec read by
    /// [`Codecs::read`] needs them.
    numbers: Option<Rc<Numbers>>,
}

impl<'c> Reading<'c> {
    pub fn new(codecs: Codecs<'c>, problems: Vec<Problem>) -> Reading<'c> {
        Reading {
            codecs,
            problems,
            numbers: None,
        }
    }

    pub fn codecs(&self) -> &Codecs<'c> {
        &self.codecs
    }

    /// How many problems have been found.
    pub fn count(&self) -> usize {
        self.problems.len()
    }

    /// The problems found, in order.
    pub fn into_problems(self) -> Vec<Problem> {
        self.problems
    }

    pub fn problems_mut(&mut self) -> &mut Vec<Problem> {
        &mut self.problems
    }

    /// A problem: the value at `at` broke `rule`.
    pub fn problem(&mut self, at: &At<'_>, rule: &str) {
        self.problems.push(Problem {
            field: at.shown(),
            rule: rule.to_string(),
        });
    }

    /// `required` at `at`, for a value that isn't nullable read as `null`;
    /// `false` then, and `true` for anything else.
    pub fn given(&mut self, item: Item<'_>, at: &At<'_>) -> bool {
        if item.is_null() {
            self.problem(at, "required");
            return false;
        }
        true
    }

    /// An object's members; `type` at `at` for anything else.
    pub fn members<'i>(&mut self, item: Item<'i>, at: &At<'_>) -> Option<Members<'i>> {
        let members = item.members();
        if members.is_none() {
            self.problem(at, "type");
        }
        members
    }

    /// Field `name` of an object, by its member among `members` and how
    /// `field` is read without one, as [`Codecs::read`] decides it:
    /// `required` at `at` when it is missing and must be given.
    pub fn field<'i>(
        &mut self,
        members: &Members<'i>,
        name: &str,
        field: Field,
        at: &At<'_>,
    ) -> Got<'i> {
        let member = members.get(name, field.list);
        match super::presence(
            member.map(Item::is_null),
            field.absent,
            field.open_nullable,
            field.omittable,
        ) {
            Presence::Left => Got::Left,
            Presence::None => Got::None,
            Presence::Missing => {
                self.problem(at, "required");
                Got::Left
            }
            Presence::Read => member.map_or(Got::Left, Got::Read),
        }
    }

    /// `unknown_field` for each of `members` not in `declared`, in name
    /// order.
    pub fn unknown(&mut self, members: &Members<'_>, declared: &[&str], at: &At<'_>) {
        for name in members.unknown(declared) {
            self.problem(&At::Member(at, &name), "unknown_field");
        }
    }

    /// The value shape `shape` builds from `slots`, each field's by its
    /// place, `None` for one not given ([`crate::shape::Shape::build_slots`]).
    pub fn build(&self, shape: u32, slots: &mut [Option<Value>]) -> Option<Value> {
        self.codecs.shapes.get(shape as usize)?.build_slots(slots)
    }

    /// `value` stored and checked against validator `validator`, its
    /// broken rule a problem at `at`, or at a vector's item.
    pub fn check(&mut self, validator: u32, value: Value, at: &At<'_>) -> Option<Value> {
        match self.codecs.check(ValidatorId(validator), value) {
            Ok(value) => Some(value),
            Err(broken) => {
                match broken.item {
                    Some(item) => self.problem(&At::Item(at, item), &broken.rule),
                    None => self.problem(at, &broken.rule),
                }
                None
            }
        }
    }

    /// A scalar of `builtin` from `item`, checked against `validator`.
    /// A standard type sent as its members is read by [`Codecs::read`].
    pub fn scalar(
        &mut self,
        codec: u32,
        builtin: Builtin,
        validator: u32,
        item: Item<'_>,
        at: &At<'_>,
    ) -> Option<Value> {
        match super::scalar(builtin, item) {
            Some(Ok(value)) => self.check(validator, value, at),
            Some(Err(rule)) => {
                self.problem(at, rule);
                None
            }
            None => self.generic(codec, item, at),
        }
    }

    /// A scalar of `builtin` from `text`, a placeholder's, read as
    /// [`Codecs::read_text`] reads one and checked against `validator`.
    pub fn text(
        &mut self,
        builtin: Builtin,
        validator: u32,
        text: &str,
        at: &At<'_>,
    ) -> Option<Value> {
        match self
            .codecs
            .text_scalar(builtin, ValidatorId(validator), text)
        {
            Ok(value) => self.check(validator, value, at),
            Err(rule) => {
                self.problem(at, rule);
                None
            }
        }
    }

    /// An array's items; `type` at `at` for anything else.
    pub fn items<'i>(&mut self, item: Item<'i>, at: &At<'_>) -> Option<Items<'i>> {
        if item.kind() != Kind::Array {
            self.problem(at, "type");
            return None;
        }
        Some(item.items())
    }

    /// A list, or a set when `set`, of the items of `item`, each read by
    /// `read`, then checked against `validator`: `type` for anything but
    /// an array, each item's problems at its place, and `duplicate` for a
    /// set that repeats one.
    pub fn list(
        &mut self,
        item: Item<'_>,
        at: &At<'_>,
        read: Reader,
        set: bool,
        validator: u32,
    ) -> Option<Value> {
        let items = self.items(item, at)?;
        let before = self.count();
        let mut values = Vec::with_capacity(items.len());
        for (place, entry) in items.enumerate() {
            if let Some(value) = read(self, entry, &At::Item(at, place)) {
                values.push(value);
            }
        }
        if self.count() > before {
            return None;
        }
        let Some(value) = super::collected(values, set) else {
            self.problem(at, "duplicate");
            return None;
        };
        self.check(validator, value, at)
    }

    /// `tag`'s text among an object's `members`: `required` at it when it
    /// isn't a string.
    pub fn tag<'i>(
        &mut self,
        members: &Members<'i>,
        at: &At<'_>,
    ) -> Option<std::borrow::Cow<'i, str>> {
        let tag = members
            .get("tag", false)
            .filter(|item| item.kind() == Kind::String)
            .and_then(Item::text);
        if tag.is_none() {
            self.problem(&At::Member(at, "tag"), "required");
        }
        tag
    }

    /// A value of codec `codec` read by [`Codecs::read`] from `item`, as
    /// a `serde_json` value, at `at` in its document: a codec no reader is
    /// generated for.
    pub fn generic(&mut self, codec: u32, item: Item<'_>, at: &At<'_>) -> Option<Value> {
        if self.numbers.is_none() {
            self.numbers = item.numbers();
        }
        let path = Path::of(at.parts(), self.numbers.clone());
        self.codecs
            .read(CodecId(codec), &item.to_json(), &path, &mut self.problems)
    }

    /// A value of codec `codec` read by [`Codecs::read_text`] from `text`,
    /// a placeholder's.
    pub fn generic_text(&mut self, codec: u32, text: &str, at: &At<'_>) -> Option<Value> {
        let path = Path::of(at.parts(), None);
        self.codecs
            .read_text(CodecId(codec), text, &path, &mut self.problems)
    }
}
