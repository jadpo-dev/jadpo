//! Geospatial conditions and order (area 20, "Queries"; chunk 5.5u5):
//! `geo.within`, `geo.contains` and `geo.intersects` in `where:`, and
//! `geo.distance` in `order:`.

use super::*;
use jadpo_std::Op;

/// The `geo.` operation a call names, when it is one a `where:` line takes:
/// `within`, `contains` or `intersects`.
pub(super) fn geo_condition_name(call: &ast::CallExpr) -> Option<&'static str> {
    let (head, rest) = dotted(&call.expr()?)?;
    if tree::name_ref_text(&head).as_deref() != Some("geo") || rest.len() != 1 {
        return None;
    }
    match tree::name_ref_text(&rest[0]).as_deref()? {
        "within" => Some("within"),
        "contains" => Some("contains"),
        "intersects" => Some("intersects"),
        _ => None,
    }
}

/// Whether a call is `geo.distance(...)`.
pub(super) fn is_geo_distance(call: &ast::CallExpr) -> bool {
    call.expr()
        .and_then(|callee| dotted(&callee))
        .is_some_and(|(head, rest)| {
            tree::name_ref_text(&head).as_deref() == Some("geo")
                && rest.len() == 1
                && tree::name_ref_text(&rest[0]).as_deref() == Some("distance")
        })
}

/// The `geo.distance` entry of an `order:` list, if it has one.
pub(super) fn distance_entry(member: &ast::Member) -> Option<ast::OrderedExpr> {
    let Some(Expr::ListExpr(list)) = member.value() else {
        return None;
    };
    list.exprs().find_map(|item| match item {
        Expr::OrderedExpr(ordered)
            if matches!(ordered.expr().map(|e| unparen(&e)), Some(Expr::CallExpr(call)) if is_geo_distance(&call)) =>
        {
            Some(ordered)
        }
        _ => None,
    })
}

/// The geospatial standard type a value of `ty` is: one of the six
/// geometries, `Geometry`, `BoundingBox` or `Circle`.
pub(super) fn geo_kind(lookup: &dyn TypeView, ty: &Ty) -> Option<&'static str> {
    use jadpo_std::standard::geo::{
        BOUNDING_BOX, CIRCLE, GEOMETRY, LINE_STRING, MULTI_LINE_STRING, MULTI_POINT, MULTI_POLYGON,
        POINT, POLYGON,
    };
    let name = match lookup.structure(ty.strip_nullable()) {
        Ty::Named(name) | Ty::Opaque(name, _) => name,
        _ => return None,
    };
    [
        POINT,
        LINE_STRING,
        POLYGON,
        MULTI_POINT,
        MULTI_LINE_STRING,
        MULTI_POLYGON,
        GEOMETRY,
        BOUNDING_BOX,
        CIRCLE,
    ]
    .into_iter()
    .find(|known| *known == name)
}

/// Whether a geospatial type is a geometry: one of the six or `Geometry`.
pub(super) fn is_geometry(kind: &str) -> bool {
    use jadpo_std::standard::geo::{BOUNDING_BOX, CIRCLE};
    kind != BOUNDING_BOX && kind != CIRCLE
}

impl Reader<'_> {
    /// `geo.within`, `geo.contains` or `geo.intersects` in a `where:` line
    /// (area 20, "Queries"): one of the entity's geometry fields first,
    /// then a parameter, typed as the operation types them in code.
    pub(super) fn geo_condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        call: &ast::CallExpr,
        whole: &Expr,
    ) -> Option<Condition> {
        use jadpo_std::standard::geo::{BOUNDING_BOX, CIRCLE, MULTI_POLYGON, POINT, POLYGON};
        let name = geo_condition_name(call)?;
        let path = scope.path;
        let written = format!("geo.{name}");
        if let Some(word) = self.under.last() {
            self.unsupported(
                scope,
                whole,
                format!("A GiST index finds the records a `geo.` condition holds for, never those it doesn't, so the condition can't be under `{word}`: it stands on a `where:` line of its own, or joined to the others with `and` (area 20, \"Queries\")."),
                if *word == "or" {
                    "Write each side as a query of its own".to_string()
                } else {
                    "Write the condition without `not`, or filter the records after the query".to_string()
                },
            );
            return None;
        }
        let args: Vec<ast::Arg> = call
            .arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .collect();
        let positional: Vec<Expr> = args
            .iter()
            .filter(|arg| arg.name().is_none())
            .filter_map(ast::Arg::expr)
            .collect();
        let mut distance = None;
        for arg in args.iter().filter(|arg| arg.name().is_some()) {
            let named = arg
                .name()
                .and_then(|n| tree::name_text(&n))
                .unwrap_or_default();
            if name == "within" && named == "distance" && distance.is_none() {
                distance = arg.expr();
                continue;
            }
            self.unsupported(
                scope,
                whole,
                format!(
                    "`{written}` takes the field and a value{}, and no other named argument.",
                    if name == "within" {
                        ", and `distance:` for a radius"
                    } else {
                        ""
                    }
                ),
                format!("Write `{written}({}.<field>, <parameter>)`", entity.name),
            );
            return None;
        }
        let [first, second] = positional.as_slice() else {
            self.unsupported(
                scope,
                whole,
                format!("`{written}` in a query takes two: one of `{}`'s geometry fields, then a value the call supplies.", entity.name),
                format!("Write `{written}({}.<field>, <parameter>)`", entity.name),
            );
            return None;
        };
        let Some(field) = self.field_named(entity, first) else {
            let reason = if self.field_named(entity, second).is_some() {
                format!(
                    "A GiST index on the field serves the condition when the field comes first and the value after: `{written}({}.<field>, <parameter>)` (area 20, \"Queries\").",
                    entity.name
                )
            } else {
                format!(
                    "A `geo.` condition tests one of `{}`'s geometry fields, named first, against a value the call supplies.",
                    entity.name
                )
            };
            self.unsupported(
                scope,
                whole,
                reason,
                format!("Write `{}.<field>` first", entity.name),
            );
            return None;
        };
        let field_written = format!("{}.{}", entity.name, field.name);
        let field_kind = match field.geometry {
            Some(kind) => kind,
            None if matches!(field.compound, Some(compound) if compound == BOUNDING_BOX || compound == CIRCLE) =>
            {
                self.unsupported(
                    scope,
                    whole,
                    format!("`{field_written}` is a `{}`, kept in named columns rather than one `geography` column, so no GiST index holds it (area 20, \"Storage\").", field.compound.unwrap_or_default()),
                    "Test a geometry field, or keep the shape as a `Polygon`".to_string(),
                );
                return None;
            }
            None => {
                if !field.ty.has_error() {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISMATCH,
                            format!("`{field_written}` can't be used where a geometry is required"),
                        )
                        .at(Self::span(path, first), format!("a `{}`", field.ty.strip_nullable()))
                        .reason(format!("`{written}` compares shapes, each a `Point`, a `LineString`, a `Polygon`, one of the three multi-part geometries, or a `Geometry` holding one (area 20, \"Operations\"); `{field_written}` is `{}`.", field.ty.strip_nullable()))
                        .next("Test a geometry field"),
                    );
                }
                return None;
            }
        };
        // The value: a parameter, or a field inside one.
        let Some(given) = self.value_type(scope, second) else {
            let shown_second = shown(second.syntax());
            self.unsupported(
                scope,
                whole,
                format!("A `geo.` condition compares the field with a value the call supplies, a parameter or a field inside one; `{shown_second}` isn't one."),
                "Pass the value as a parameter".to_string(),
            );
            return None;
        };
        if given.has_error() {
            return None;
        }
        if self.lookup.is_nullable(&given) {
            self.optional_in_where(scope, second);
            return None;
        }
        let kind = geo_kind(self.lookup, &given);
        let mismatch = |this: &mut Self, at: &Expr, ty: &Ty, wanted: &str, why: &str| {
            let shown_at = shown(at.syntax());
            this.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{ty}` can't be used where {wanted} is required"),
                )
                .at(Self::span(path, at), format!("`{shown_at}` is `{ty}`"))
                .reason(format!(
                    "`{written}`'s value in a query is {wanted}: {why}."
                ))
                .next(format!("Pass {wanted} here")),
            );
        };
        let point_field = field_kind == POINT;
        let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        let test_value = |this: &mut Self| -> Option<Value> {
            match this.value(scope, second, &given)? {
                (value @ Value::Param { .. }, _) => Some(value),
                _ => None,
            }
        };
        let test = match (name, &distance) {
            ("within", Some(reach)) => {
                if !point_field {
                    mismatch(
                        self,
                        first,
                        &field_ty,
                        "a `Point`",
                        "`within` measures a distance between points only, from a point to a line or a polygon being left out (area 20, question 10)",
                    );
                    return None;
                }
                if kind != Some(POINT) {
                    mismatch(
                        self,
                        second,
                        &given,
                        "a `Point`",
                        "`within` measures a distance between points only, from a point to a line or a polygon being left out (area 20, question 10)",
                    );
                    return None;
                }
                let point = test_value(self)?;
                let distance = self.geo_reach(scope, reach)?;
                (
                    Op::GeoWithin,
                    vec![
                        CallArg::Value(point, Held::Shape(POINT)),
                        CallArg::Value(
                            distance,
                            Held::Builtin(Prim::Quantity(jadpo_std::QuantityKind::Length)),
                        ),
                    ],
                )
            }
            ("within", None) => match kind {
                Some(area @ (POLYGON | MULTI_POLYGON)) => (
                    Op::GeoWithin,
                    vec![CallArg::Value(test_value(self)?, Held::Shape(area))],
                ),
                Some(BOUNDING_BOX) => {
                    if !point_field {
                        self.unsupported(
                            scope,
                            whole,
                            format!("A box's sides are parallels and meridians, and a stored line's or polygon's edges are great circles, so the index the box filter searches holds points only (area 20, question 11); `{field_written}` is `{field_kind}`."),
                            "Test a shape field with `geo.intersects` and a `Polygon`".to_string(),
                        );
                        return None;
                    }
                    (
                        Op::GeoWithin,
                        vec![CallArg::Value(test_value(self)?, Held::Shape(BOUNDING_BOX))],
                    )
                }
                Some(CIRCLE) => {
                    if !point_field {
                        mismatch(
                            self,
                            first,
                            &field_ty,
                            "a `Point`",
                            "a `Circle` holds points only, a distance to a line or a polygon being left out (area 20, question 10)",
                        );
                        return None;
                    }
                    (
                        Op::GeoWithin,
                        vec![CallArg::Value(test_value(self)?, Held::Shape(CIRCLE))],
                    )
                }
                _ => {
                    mismatch(
                        self,
                        second,
                        &given,
                        "a `Polygon`, a `MultiPolygon`, a `BoundingBox` or a `Circle`",
                        "the area the field's records are inside (area 20, \"Queries\")",
                    );
                    return None;
                }
            },
            ("contains" | "intersects", None) => {
                let Some(shape_kind) = kind.filter(|kind| is_geometry(kind)) else {
                    mismatch(
                        self,
                        second,
                        &given,
                        "a geometry",
                        "a `Point`, a `LineString`, a `Polygon`, one of the three multi-part geometries, or a `Geometry` holding one (area 20, \"Operations\")",
                    );
                    return None;
                };
                let shape = test_value(self)?;
                let op = if name == "contains" {
                    Op::GeoContains
                } else {
                    Op::GeoIntersects
                };
                (op, vec![CallArg::Value(shape, Held::Shape(shape_kind))])
            }
            _ => return None,
        };
        let (op, args) = test;
        Some(Condition::Call {
            call: Call {
                op,
                field: field.name.clone(),
                args,
                result: Held::Builtin(Prim::Bool),
            },
            test: Test::Holds,
        })
    }

    /// `within`'s `distance:` in a query: a `Length`, written out or a
    /// parameter, never below 0, as in code (area 20, "Operations").
    pub(super) fn geo_reach(&mut self, scope: &Scope<'_>, reach: &Expr) -> Option<Value> {
        let length = Ty::Prim(Prim::Quantity(jadpo_std::QuantityKind::Length));
        let (value, ty) = self.value(scope, reach, &length)?;
        let written = shown(reach.syntax());
        let negative = |message: String, label: String, why: String| {
            Diagnostic::new(&rule::TYPE_ARGUMENT_NEGATIVE, message)
                .at(Self::span(scope.path, reach), label)
                .reason(format!("`geo.within`'s `distance` is how far apart the two points may be (area 20, \"Operations\"), so it is never negative, which is proved while compiling: a literal isn't below `0` and a value's type has a `min` of at least `0`; {why}"))
                .next("Pass a length whose type has `min: 0metre`, or a length written out")
        };
        match &value {
            Value::Literal(jadpo_std::Literal::Quantity(text)) => {
                if jadpo_std::quantity::parse_literal(text).is_ok_and(|(_, amount)| amount < 0) {
                    self.out.push(negative(
                        "`geo.within`'s `distance` is never negative".to_string(),
                        "below `0`".to_string(),
                        format!("`{written}` is below `0`."),
                    ));
                    return None;
                }
            }
            Value::Param { .. } => {
                if self.lookup.is_nullable(&ty) {
                    self.optional_in_where(scope, reach);
                    return None;
                }
                if self.lookup.representation(&ty)
                    != Some(Prim::Quantity(jadpo_std::QuantityKind::Length))
                {
                    self.mismatch(scope, reach, &ty, &length);
                    return None;
                }
                let floor = self
                    .lookup
                    .rule_chain(&ty)
                    .iter()
                    .any(|(_, rules)| rules.min.is_some_and(|min| min.mantissa >= 0));
                if !floor {
                    self.out.push(negative(
                        format!("`{written}` may be negative"),
                        format!("`{ty}` can be below `0`"),
                        format!("`{ty}` allows values below `0`."),
                    ));
                    return None;
                }
            }
            _ => {
                self.mismatch(scope, reach, &ty, &length);
                return None;
            }
        }
        Some(value)
    }

    /// `order: [geo.distance(Entity.field, point) asc]` (area 20, question
    /// 4): the only order of a list, nearest first, beside a
    /// `geo.within(Entity.field, point, distance:)` line on the same field
    /// and point, which bounds what is sorted.
    pub(super) fn distance_order(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        member: &ast::Member,
        cardinality: Cardinality,
        filter: &[Condition],
    ) -> Option<DistanceOrder> {
        let path = scope.path;
        let Some(Expr::ListExpr(list)) = member.value() else {
            return None;
        };
        let items: Vec<Expr> = list.exprs().collect();
        let entry = distance_entry(member)?;
        let entry_span = Self::span(path, &entry);
        if items.len() > 1 {
            let other = items
                .iter()
                .find(|item| item.syntax() != entry.syntax())
                .map_or_else(|| entry_span.clone(), |item| Self::span(path, item));
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`geo.distance` is the only order a query can have".to_string(),
                )
                .at(other, "another order beside `geo.distance`")
                .reason("Records nearest first are sorted by their distance alone, with `id` breaking ties, as `geo.distance` works it out (area 20, \"Queries\").")
                .next("Remove the other order"),
            );
            return None;
        }
        if cardinality == Cardinality::Page {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "A page can't be ordered by `geo.distance`".to_string(),
                )
                .at(entry_span, "a page by distance")
                .reason("A page continues after a cursor of its order's values, and a distance is worked out, not stored, so no cursor can hold it; nearest first is one list with `limit:` (area 20, question 4).")
                .next("Return a `List` with `limit:`, or page by a field"),
            );
            return None;
        }
        if entry.direction().is_some_and(|d| d.text() != "asc") {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_DIRECTION,
                    "`geo.distance` sorts nearest first only".to_string(),
                )
                .at(entry_span, "not `asc`")
                .reason("Records are sorted by distance only inside the radius a `geo.within(..., distance:)` line gives, which bounds what is read nearest first; farthest first has nothing to bound it (area 20, question 4).")
                .next("Write `asc`"),
            );
            return None;
        }
        let Some(Expr::CallExpr(call)) = entry.expr().map(|e| unparen(&e)) else {
            return None;
        };
        let args: Vec<ast::Arg> = call
            .arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .collect();
        let positional: Vec<Expr> = args.iter().filter_map(ast::Arg::expr).collect();
        let usage = |this: &mut Self, reason: String| {
            this.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`geo.distance` in `order:` measures from a field to a point".to_string(),
                )
                .at(entry_span.clone(), "not a field and a point")
                .reason(reason)
                .next(format!(
                    "Write `order: [geo.distance({}.<point field>, <point>) asc]`",
                    entity.name
                )),
            );
        };
        let ([first, second], true) = (
            positional.as_slice(),
            args.iter().all(|arg| arg.name().is_none()),
        ) else {
            usage(self, "`geo.distance` takes two points: one of the entity's `Point` fields, then a point the call supplies.".to_string());
            return None;
        };
        let Some(field) = self.field_named(entity, first) else {
            usage(
                self,
                format!(
                    "Records are sorted by how far one of `{}`'s `Point` fields, named first, is from a point the call supplies.",
                    entity.name
                ),
            );
            return None;
        };
        if field.geometry != Some(jadpo_std::standard::geo::POINT) {
            usage(
                self,
                format!(
                    "`geo.distance` measures between points only (area 20, question 10), and `{}.{}` is `{}`.",
                    entity.name,
                    field.name,
                    field.ty.strip_nullable()
                ),
            );
            return None;
        }
        let Some(given) = self.value_type(scope, second) else {
            usage(
                self,
                format!(
                    "Records are sorted by how far `{}.{}` is from a point the call supplies, a parameter or a field inside one.",
                    entity.name, field.name
                ),
            );
            return None;
        };
        if geo_kind(self.lookup, &given) != Some(jadpo_std::standard::geo::POINT) {
            usage(
                self,
                format!(
                    "`geo.distance` measures between points only (area 20, question 10), and `{}` is `{given}`.",
                    shown(second.syntax())
                ),
            );
            return None;
        }
        let point = match self.value(scope, second, &given) {
            Some((point @ Value::Param { .. }, _)) => point,
            _ => return None,
        };
        // A radius from the same point on the same field: `within` with
        // its `distance:`.
        let bounded = jadpo_sql::condition::search_calls(filter).iter().any(|call| {
            call.field == field.name
                && call.op == Op::GeoWithin
                && matches!(
                    call.args.as_slice(),
                    [CallArg::Value(near, Held::Shape(jadpo_std::standard::geo::POINT)), CallArg::Value(..)] if *near == point
                )
        });
        if !bounded {
            let shown_point = shown(second.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    format!("Nearest first needs a radius around `{shown_point}`"),
                )
                .at(entry_span, "no radius")
                .reason(format!(
                    "Records are sorted by distance only inside a radius, `where: geo.within({}.{}, {shown_point}, distance: <length>)`, which the index searches; PostGIS's own nearest-first search measures on a sphere, so its order would disagree with `geo.distance` (area 20, question 4).",
                    entity.name, field.name
                ))
                .next(format!(
                    "Add `where: geo.within({}.{}, {shown_point}, distance: <length>)`",
                    entity.name, field.name
                )),
            );
            return None;
        }
        Some(DistanceOrder {
            field: field.name.clone(),
            point,
        })
    }
}
