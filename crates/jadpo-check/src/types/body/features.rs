//! GeoJSON's `Feature<G, P>` and `FeatureCollection<G, P>` (area 20,
//! "Interop", question 6; chunk 5.5u6): two standard object types with
//! type arguments, `G` a geometry type (one of the six or `Geometry`) and
//! `P` an `Object` type of the application's, built from their fields and
//! read by them, as `Range<T>` is.

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::standard::geo::{
    FEATURE, FEATURE_COLLECTION, FEATURE_COLLECTION_FIELDS, FEATURE_FIELDS, FEATURE_GEOMETRIES,
};
use jadpo_syntax::ast::{self, AstNode};

use super::super::rules::Rules;
use super::super::shape::FieldsBase;
use super::super::table::{Def, FieldDef, TypeView};
use super::super::ty::Ty;
use super::{Body, node_key};
use crate::tree;

/// Whether `name` is one of the two feature types.
pub(crate) fn is_feature(name: &str) -> bool {
    name == FEATURE || name == FEATURE_COLLECTION
}

/// A feature type's fields for its type arguments `[G, P]`: a feature's
/// `geometry` and `properties`, a collection's `features`; `None` for any
/// other type, or the wrong number of arguments.
pub(crate) fn feature_fields(name: &str, args: &[Ty]) -> Option<Vec<FieldDef>> {
    let [geometry, properties] = args else {
        return None;
    };
    let typed: Vec<(&str, Ty)> = match name {
        FEATURE => vec![
            (FEATURE_FIELDS[0], geometry.clone()),
            (FEATURE_FIELDS[1], properties.clone()),
        ],
        FEATURE_COLLECTION => vec![(
            FEATURE_COLLECTION_FIELDS[0],
            Ty::List(Box::new(Ty::Opaque(FEATURE.to_string(), args.to_vec()))),
        )],
        _ => return None,
    };
    Some(
        typed
            .into_iter()
            .map(|(name, declared)| FieldDef {
                name: name.to_string(),
                declared,
                rules: Rules::default(),
                optional: false,
                default: None,
                header: None,
                copied: false,
                replaces: None,
                generated: false,
            })
            .collect(),
    )
}

/// What is wrong with `[G, P]` as a feature type's arguments, if anything:
/// `G` isn't a geometry type, or `P` isn't an `Object` type.
pub(crate) fn wrong_arguments(
    table: &dyn TypeView,
    name: &str,
    args: &[Ty],
    span: Span,
) -> Option<Diagnostic> {
    let [geometry, properties] = args else {
        return None;
    };
    if !geometry.has_error() && !is_geometry(table, geometry) {
        return Some(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("A `{name}`'s geometry is a geometry type, not `{geometry}`"),
            )
            .at(span, format!("its geometry is `{geometry}`"))
            .reason(format!(
                "A GeoJSON feature holds one geometry (area 20, \"Interop\"): its first type argument is `Point`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon`, or `Geometry` for any of them; `{geometry}` isn't one."
            ))
            .next(format!("Write `{name}<Polygon, ...>`, or the geometry type the feature holds")),
        );
    }
    if !properties.has_error() && !is_object(table, properties) {
        return Some(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("A `{name}`'s properties are an `Object` type, not `{properties}`"),
            )
            .at(span, format!("its properties are `{properties}`"))
            .reason(format!(
                "A GeoJSON feature's `properties` is a JSON object (area 20, \"Interop\"), so its second type argument is an `Object` type of the application's, whose fields are the properties; `{properties}` isn't one."
            ))
            .next("Declare `type ... = Object { ... }` with the feature's properties, and use it here"),
        );
    }
    None
}

/// Whether `ty` is one of the six geometries or `Geometry`, never `none`.
fn is_geometry(table: &dyn TypeView, ty: &Ty) -> bool {
    // `Geometry`, a standard choice, is named as an enum is.
    !table.is_nullable(ty)
        && match table.structure(ty) {
            Ty::Opaque(name, args) => {
                args.is_empty() && FEATURE_GEOMETRIES.contains(&name.as_str())
            }
            Ty::Named(name) => name == jadpo_std::standard::geo::GEOMETRY,
            _ => false,
        }
}

/// Whether `ty` is an `Object` type the application declares, never
/// `none`.
fn is_object(table: &dyn TypeView, ty: &Ty) -> bool {
    !table.is_nullable(ty)
        && matches!(table.structure(ty), Ty::Named(name) if matches!(
            table.def(&name),
            Some(Def::Fields { base: FieldsBase::Object, .. })
        ))
}

impl Body<'_> {
    /// `Feature { geometry: ..., properties: ... }` or `FeatureCollection {
    /// features: [...] }`: its type arguments are `args` where the
    /// literal's place gives them, else its fields' values give them.
    pub(super) fn feature_literal(
        &mut self,
        record: &ast::RecordExpr,
        name: &str,
        args: Option<Vec<Ty>>,
    ) -> Ty {
        let head_span = match record.expr() {
            Some(head) => self.span(&head),
            None => self.span(record),
        };
        let args = match args {
            Some(args) => args,
            None => {
                let value = |field: &str| {
                    tree::members(record.member_list())
                        .into_iter()
                        .find(|(key, _)| tree::name_text(key).as_deref() == Some(field))
                        .and_then(|(_, member)| member.value())
                };
                let given = |body: &mut Self, field: &str| match value(field) {
                    Some(value) => {
                        let ty = body.infer(&value);
                        body.table.structure(&ty)
                    }
                    None => Ty::Error,
                };
                let args = if name == FEATURE {
                    vec![
                        given(self, FEATURE_FIELDS[0]),
                        given(self, FEATURE_FIELDS[1]),
                    ]
                } else {
                    match given(self, FEATURE_COLLECTION_FIELDS[0]) {
                        Ty::List(item) => match *item {
                            Ty::Opaque(feature, args) if feature == FEATURE && args.len() == 2 => {
                                args
                            }
                            _ => vec![Ty::Error, Ty::Error],
                        },
                        _ => vec![Ty::Error, Ty::Error],
                    }
                };
                if let Some(wrong) = wrong_arguments(self.table, name, &args, head_span.clone()) {
                    self.out.push(wrong);
                    self.walk(|walk| walk.member_values(record.member_list()));
                    return Ty::Error;
                }
                args
            }
        };
        let fields = feature_fields(name, &args).unwrap_or_default();
        self.facts.builds.insert(
            node_key(record.syntax()),
            super::facts::BuildFact::Object(name.to_string()),
        );
        self.fields_given(
            record.member_list(),
            name,
            &[],
            &fields,
            super::expr::Required::All,
            head_span,
            &[],
        );
        self.facts
            .constructions
            .insert(node_key(record.syntax()), super::Fallible::Never);
        Ty::Opaque(name.to_string(), args)
    }
}
