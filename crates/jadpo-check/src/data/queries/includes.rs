//! A query's `include:` block and the result type's fields (area 6,
//! "Children and parents"): each include read into an [`Include`], and
//! what fills each field of the records a query returns.

use super::*;

impl Reader<'_> {
    /// The entries of an `include:` block, at nesting `level` (1 for the
    /// query's own).
    pub(super) fn includes(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        member: &ast::Member,
        level: usize,
        query: &Symbol,
    ) -> Vec<Include> {
        let key_span = member.member_key().map_or_else(
            || Span::new(scope.path, 0, 0),
            |k| Self::span(scope.path, &k),
        );
        if level > INCLUDE_LEVELS {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_DEPTH,
                    "Includes nest at most two levels".to_string(),
                )
                .at(key_span, "a third level")
                .reason("Each include is one more database query, and a query's includes nest up to two levels so its cost stays easy to see; a deeper shape is a second query called from the action.")
                .next("Read the deeper records with a query of their own"),
            );
            return Vec::new();
        }
        let Some(block) = member.block() else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_UNKNOWN,
                    "`include:` is a block of named includes".to_string(),
                )
                .at(key_span, "not a block")
                .reason("Each include puts related records under a field of the result: `include: { <field>: <Entity>.<reference> }`.")
                .next("Write `include: { <field>: <Entity>.<reference> }`"),
            );
            return Vec::new();
        };
        let mut out: Vec<Include> = Vec::new();
        let mut names: BTreeMap<String, Span> = BTreeMap::new();
        for item in block.member_items() {
            let MemberItem::Member(entry) = item else {
                continue;
            };
            let Some(key) = entry.member_key() else {
                continue;
            };
            let Some(name) = key.names().next().and_then(|n| tree::name_text(&n)) else {
                continue;
            };
            let name_span = Self::span(scope.path, &key);
            if let Some(earlier) = names.get(&name) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_REPEATED_SECTION,
                        format!("`{name}` is included twice"),
                    )
                    .at(name_span, "again")
                    .also(earlier.clone(), "first here")
                    .reason(
                        "Each include fills one field of the result, so a field is included once.",
                    )
                    .next(format!("Remove one `{name}`")),
                );
                continue;
            }
            names.insert(name.clone(), name_span.clone());
            if let Some(include) =
                self.include(scope, entity, &name, &name_span, &entry, level, query)
            {
                out.push(include);
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn include(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        name: &str,
        name_span: &Span,
        entry: &ast::Member,
        level: usize,
        query: &Symbol,
    ) -> Option<Include> {
        let sections = match entry.block() {
            Some(block) => {
                self.sections(scope.path, Some(block), &INCLUDE_SETTINGS, "include", name)
            }
            None => Sections::default(),
        };
        let via_expr = match (entry.value(), sections.first.get("via")) {
            (Some(value), _) => value,
            (None, Some(via)) => via.value()?,
            (None, None) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_SECTION_REQUIRED,
                        format!("The include `{name}` doesn't say which reference it follows"),
                    )
                    .at(name_span.clone(), "no `via:`")
                    .reason("An include follows a reference: one of this entity's own gives the record it points at, and another entity's reference to this one gives the records that point here.")
                    .next("Add `via: <Entity>.<reference>` to the block"),
                );
                return None;
            }
        };
        let via_span = Self::span(scope.path, &via_expr);
        let (qualifier, field) = match dotted(&via_expr) {
            Some((first, rest)) if rest.len() == 1 => (
                tree::name_ref_text(&first).unwrap_or_default(),
                tree::name_ref_text(&rest[0]).unwrap_or_default(),
            ),
            _ => (String::new(), String::new()),
        };
        let entities = self.entities;
        let mut found: Option<(IncludeKind, &EntityModel, &FieldModel)> = None;
        if qualifier == entity.name {
            if let Some(f) = entity.fields.iter().find(|f| f.name == field)
                && let Some(reference) = &f.reference
                && let Some(target) = entities.get(&reference.target)
            {
                found = Some((IncludeKind::Parent, target, f));
            }
        } else if let Some(other) = entities.get(&qualifier)
            && let Some(f) = other.fields.iter().find(|f| f.name == field)
            && f.reference
                .as_ref()
                .is_some_and(|r| r.target == entity.name)
        {
            // A unique reference gives one child, unless the key holds the
            // tenancy as well, one child in each tenant (area 9).
            let kind = if entities.unique_alone(other, f) {
                IncludeKind::Child
            } else {
                IncludeKind::Children
            };
            found = Some((kind, other, f));
        }
        let Some((kind, target, via)) = found else {
            // A reference whose type names nothing is resolution's error.
            if entities
                .get(&qualifier)
                .and_then(|e| e.fields.iter().find(|f| f.name == field))
                .is_some_and(|f| f.ty.has_error())
            {
                return None;
            }
            let written = shown(via_expr.syntax());
            let mut options: Vec<String> = entity
                .fields
                .iter()
                .filter(|f| f.reference.is_some())
                .map(|f| format!("`{}.{}`", entity.name, f.name))
                .collect();
            for other in &entities.all().entities {
                for f in &other.fields {
                    if f.reference
                        .as_ref()
                        .is_some_and(|r| r.target == entity.name)
                    {
                        options.push(format!("`{}.{}`", other.name, f.name));
                    }
                }
            }
            options.sort();
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_UNKNOWN,
                    format!("`{written}` isn't a reference to or from `{}`", entity.name),
                )
                .at(via_span, "not a reference")
                .reason(format!(
                    "An include follows a reference: one of `{e}`'s own gives the record it points at, and another entity's reference to `{e}` gives the records that point here.{}",
                    if options.is_empty() {
                        format!(" No reference leads to or from `{e}`.", e = entity.name)
                    } else {
                        format!(" Those here are {}.", crate::and_list(&options))
                    },
                    e = entity.name
                ))
                .next(if options.is_empty() {
                    "Add a reference field, or read the records with a query of their own".to_string()
                } else {
                    "Name one of those references".to_string()
                }),
            );
            return None;
        };
        // An include stays inside the domain.
        let theirs = self
            .lookup
            .declared(&target.name)
            .iter()
            .find(|s| s.kind == ItemKind::Entity)
            .map(|s| s.ownership.clone());
        if let (Some(Ownership::Domain(theirs)), Ownership::Domain(ours)) =
            (&theirs, &query.ownership)
            && theirs != ours
        {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_OTHER_DOMAIN,
                    format!("The include `{name}` reads `{}`, an entity of domain `{theirs}`", target.name),
                )
                .at(via_span, format!("domain `{theirs}`"))
                .reason(format!(
                    "Includes follow references inside the domain; another domain's data comes from its public queries, so `{theirs}` decides what it shares."
                ))
                .next(format!("Call a public query of `{theirs}` for these records")),
            );
            return None;
        }
        // What the block may hold.
        let allowed: &[&str] = match kind {
            IncludeKind::Parent => &["via", "include"],
            IncludeKind::Child => &["via", "where", "where_present", "include"],
            IncludeKind::Children => &INCLUDE_SETTINGS,
        };
        for (section, member) in &sections.first {
            if !allowed.contains(&section.as_str()) {
                self.include_unexpected(scope.path, member, section, kind, target);
            }
        }
        if kind == IncludeKind::Parent {
            for (_, member) in &sections.filters {
                self.include_unexpected(
                    scope.path,
                    member,
                    if member
                        .member_key()
                        .is_some_and(|k| tree::written(k.syntax()) == "where_present")
                    {
                        "where_present"
                    } else {
                        "where"
                    },
                    kind,
                    target,
                );
            }
        }
        let mut filter = Vec::new();
        if kind != IncludeKind::Parent {
            for (present, member) in &sections.filters {
                self.search_refused = Some("an include's `where:`");
                let read = self.filter_line(scope, target, member, *present);
                self.search_refused = None;
                if let Some(condition) = read {
                    // An include's records aren't rechecked in code, which
                    // a `geo.` condition needs (area 20, chunk 5.5u5).
                    if condition.searches() {
                        let at = member.value().map_or_else(
                            || name_span.clone(),
                            |value| Self::span(scope.path, &value),
                        );
                        self.out.push(
                            Diagnostic::new(
                                &rule::TOOLCHAIN_NOT_BUILT_YET,
                                "A `geo.` condition in an include isn't built yet".to_string(),
                            )
                            .at(at, "in an include")
                            .reason("A `geo.` condition's records are found through a GiST index and each is held to Jadpo's own test again as the query runs (area 20, \"Queries\"); this compiler does that for a query's own records, not yet for those an include adds.")
                            .next("Query the related records with a query of their own"),
                        );
                        continue;
                    }
                    filter.push(condition);
                }
            }
        }
        let mut order = Vec::new();
        let mut limit = None;
        if kind == IncludeKind::Children {
            match sections.first.get("order") {
                Some(member) => order = self.order(scope.path, target, member, false),
                None => self.include_needs(name, name_span, target, "order"),
            }
            match sections.first.get("limit") {
                Some(member) => limit = self.bound(scope, member.value(), member, "limit"),
                None => self.include_needs(name, name_span, target, "limit"),
            }
        }
        let includes = match sections.first.get("include") {
            Some(member) => self.includes(scope, target, member, level + 1, query),
            None => Vec::new(),
        };
        let written = sections
            .first
            .get("include")
            .and_then(|m| m.block())
            .map(|b| {
                tree::members(Some(b))
                    .iter()
                    .filter_map(|(n, _)| tree::name_text(n))
                    .collect()
            })
            .unwrap_or_default();
        Some(Include {
            written,
            name: name.to_string(),
            kind,
            entity: target.name.clone(),
            via: via.name.clone(),
            optional: kind == IncludeKind::Parent && via.nullable,
            // Filled in when the result type's field is read.
            element: Element {
                ty: String::new(),
                fields: Vec::new(),
            },
            filter,
            order,
            limit,
            includes,
        })
    }

    pub(super) fn include_needs(
        &mut self,
        name: &str,
        span: &Span,
        target: &EntityModel,
        section: &str,
    ) {
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_REQUIRED,
                format!("The include `{name}` has no `{section}:`"),
            )
            .at(span.clone(), format!("no `{section}:`"))
            .reason(format!(
                "`{name}` is a list of `{}` records, which comes in a fixed order and is bounded, as every list is: its block gives `order:` and `limit:`.",
                target.name
            ))
            .next(format!(
                "Write `{name}: {{ via: ... order: [...] limit: ... }}`"
            )),
        );
    }

    pub(super) fn include_unexpected(
        &mut self,
        path: &str,
        member: &ast::Member,
        section: &str,
        kind: IncludeKind,
        target: &EntityModel,
    ) {
        let Some(key) = member.member_key() else {
            return;
        };
        let reason = match kind {
            IncludeKind::Parent => format!(
                "This include is the one `{}` record a reference points at, which is there whenever the reference is, so it takes no conditions, order or limit.",
                target.name
            ),
            _ => format!(
                "This include is at most one `{}` record, found through a unique reference, so it has nothing to order or limit.",
                target.name
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_UNEXPECTED,
                format!("This include takes no `{section}:`"),
            )
            .at(Self::span(path, &key), "not for this include")
            .reason(reason)
            .next(format!("Remove `{section}:`"))
            .edit(Self::span(path, member), ""),
        );
    }

    // --- the result's fields -----------------------------------------------------

    /// The fields of the records a query or include returns: each a field
    /// of `entity` or one of `includes`, in the type's declared order.
    pub(super) fn element(
        &mut self,
        path: &str,
        ty: &str,
        entity: &EntityModel,
        includes: &[Include],
        include_member: Option<&ast::Member>,
        span: &Span,
    ) -> Option<(Element, Vec<Include>)> {
        let written: Vec<String> = include_member
            .and_then(|m| m.block())
            .map(|b| {
                tree::members(Some(b))
                    .iter()
                    .filter_map(|(n, _)| tree::name_text(n))
                    .collect()
            })
            .unwrap_or_default();
        self.element_with(path, ty, entity, includes, &written, include_member, span)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn element_with(
        &mut self,
        path: &str,
        ty: &str,
        entity: &EntityModel,
        includes: &[Include],
        written: &[String],
        include_member: Option<&ast::Member>,
        span: &Span,
    ) -> Option<(Element, Vec<Include>)> {
        let include_span = |name: &str| -> Span {
            include_member
                .and_then(|m| m.block())
                .into_iter()
                .flat_map(|b| tree::members(Some(b)))
                .find(|(n, _)| tree::name_text(n).as_deref() == Some(name))
                .map_or_else(|| span.clone(), |(n, _)| Self::span(path, &n))
        };
        if ty == entity.name {
            if let Some(first) = includes.first() {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_INCLUDE_RESULT,
                        format!("`{ty}` has no field `{}` to hold the include", first.name),
                    )
                    .at(include_span(&first.name), "nowhere to put it")
                    .reason(format!(
                        "The result is the entity `{ty}` itself, which holds only its own fields; included records go under fields of a result type made for them."
                    ))
                    .next(format!(
                        "Return a type with `{}` and the fields you need, such as `type {ty}View = Output {{ pick: [...] {}: ... }}`",
                        first.name, first.name
                    )),
                );
                return None;
            }
            let fields = entity
                .fields
                .iter()
                .map(|f| (f.name.clone(), Source::Column(f.name.clone())))
                .collect();
            return Some((
                Element {
                    ty: ty.to_string(),
                    fields,
                },
                Vec::new(),
            ));
        }
        let declared = self
            .lookup
            .fields(ty)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut ok = true;
        let mut fields = Vec::new();
        let mut filled: Vec<Include> = includes.to_vec();
        for include in includes {
            if !declared.iter().any(|f| f.name == include.name) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_INCLUDE_RESULT,
                        format!("`{ty}` has no field `{}` to hold the include", include.name),
                    )
                    .at(include_span(&include.name), "nowhere to put it")
                    .reason(format!(
                        "Each include fills the result's field of the same name, and `{ty}` declares none called `{}`.",
                        include.name
                    ))
                    .next(format!(
                        "Add `{}: ...` to `{ty}`, or rename the include",
                        include.name
                    )),
                );
            }
        }
        for field in &declared {
            if let Some(at) = includes.iter().position(|i| i.name == field.name) {
                let include = &includes[at];
                match self.include_field(path, ty, field, include, &include_span(&include.name)) {
                    Some(filled_include) => filled[at] = filled_include,
                    None => ok = false,
                }
                fields.push((field.name.clone(), Source::Include(at)));
                continue;
            }
            let Some(column) = entity.fields.iter().find(|f| f.name == field.name) else {
                ok = false;
                // An include written for it that was wrong has been
                // reported.
                if written.contains(&field.name) {
                    continue;
                }
                let names: Vec<String> = entity
                    .fields
                    .iter()
                    .map(|f| format!("`{}`", f.name))
                    .collect();
                // A map layer's properties are its records' own fields
                // (chunk 5.5u8).
                let problem = if self.layer {
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}.{}` isn't a field of `{}`", field.name, entity.name),
                    )
                    .at(span.clone(), format!("`{}` can't be filled", field.name))
                    .reason(format!(
                        "A map layer fills its features' properties from `{}`'s fields of the same name ({}); nothing gives `{}`.",
                        entity.name,
                        crate::and_list(&names),
                        field.name
                    ))
                    .next(format!(
                        "Remove `{}` from `{ty}`, or name it after a field of `{}`",
                        field.name, entity.name
                    ))
                } else {
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}.{}` isn't a field of `{}`, or an include", field.name, entity.name),
                    )
                    .at(span.clone(), format!("`{}` can't be filled", field.name))
                    .reason(format!(
                        "A query fills its result's fields from `{}`'s fields of the same name ({}) and from its includes; nothing gives `{}`.",
                        entity.name,
                        crate::and_list(&names),
                        field.name
                    ))
                    .next(format!(
                        "Remove `{}` from `{ty}`, or include records under it",
                        field.name
                    ))
                };
                self.out.push(problem);
                continue;
            };
            let from = Ty::Field(entity.name.clone(), vec![column.name.clone()]);
            let from = if column.nullable {
                from.nullable()
            } else {
                from
            };
            if !field.declared.has_error() && !self.lookup.assignable(&from, &field.declared) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}.{}` is `{}`, which `{}.{}` can't fill", field.name, field.declared, entity.name, column.name),
                    )
                    .at(span.clone(), format!("`{}` doesn't fit", field.name))
                    .reason(format!(
                        "`{}.{}` is `{}`, and a stored value goes into the result as it is, so the result's field takes that type or one it widens to.",
                        entity.name,
                        column.name,
                        field_type_name(&column.ty)
                    ))
                    .next(format!(
                        "Declare `{}` as `{}.{}`, or pick it: `pick: [{}.{}]`",
                        field.name, entity.name, column.name, entity.name, column.name
                    )),
                );
                continue;
            }
            fields.push((field.name.clone(), Source::Column(column.name.clone())));
        }
        ok.then(|| {
            (
                Element {
                    ty: ty.to_string(),
                    fields,
                },
                filled,
            )
        })
    }

    /// The result field an include fills: the shape its kind gives, of the
    /// included entity or a type derived from it.
    pub(super) fn include_field(
        &mut self,
        path: &str,
        host: &str,
        field: &crate::types::FieldDef,
        include: &Include,
        span: &Span,
    ) -> Option<Include> {
        let target = self.entities.get(&include.entity)?;
        let declared = &field.declared;
        if declared.has_error() {
            return None;
        }
        let inner = match (include.kind, declared) {
            (IncludeKind::Children, Ty::List(inner)) => Some(inner.as_ref()),
            (IncludeKind::Child, Ty::Nullable(inner)) => Some(inner.as_ref()),
            (IncludeKind::Parent, Ty::Nullable(inner)) if include.optional => Some(inner.as_ref()),
            (IncludeKind::Parent, other)
                if !include.optional && !matches!(other, Ty::Nullable(_) | Ty::List(_)) =>
            {
                Some(other)
            }
            _ => None,
        };
        let element = match inner {
            Some(Ty::Named(name))
                if *name == target.name
                    || matches!(
                        self.lookup.def(name),
                        Some(Def::Fields {
                            base: FieldsBase::Object | FieldsBase::Output,
                            ..
                        })
                    ) =>
            {
                Some(name.clone())
            }
            _ => None,
        };
        let Some(element) = element else {
            let wanted = match include.kind {
                IncludeKind::Children => format!("List<{}>", target.name),
                IncludeKind::Child => format!("{}?", target.name),
                IncludeKind::Parent if include.optional => format!("{}?", target.name),
                IncludeKind::Parent => target.name.clone(),
            };
            let why = match include.kind {
                IncludeKind::Children => format!(
                    "`{}` follows `{}.{}`, which many `{}` records can share, so it is a list.",
                    include.name, target.name, include.via, target.name
                ),
                IncludeKind::Child => format!(
                    "`{}` follows `{}.{}`, which is unique, so at most one `{}` record points here and it may be absent.",
                    include.name, target.name, include.via, target.name
                ),
                IncludeKind::Parent if include.optional => format!(
                    "`{}` follows a reference that may be `none`, so the record it points at may be absent.",
                    include.name
                ),
                IncludeKind::Parent => format!(
                    "`{}` follows a required reference, so the one record it points at is always there.",
                    include.name
                ),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_RESULT,
                    format!("`{host}.{}` is `{declared}`, which the include `{}` can't fill", field.name, include.name),
                )
                .at(span.clone(), format!("needs `{wanted}`"))
                .reason(format!(
                    "{why} Its field is `{wanted}`, where `{}` may also be an object type derived from it.",
                    target.name
                ))
                .next(format!("Declare `{}: {wanted}` in `{host}`", field.name)),
            );
            return None;
        };
        let (element, includes) = self.element_with(
            path,
            &element,
            target,
            &include.includes,
            &include.written,
            None,
            span,
        )?;
        Some(Include {
            element,
            includes,
            ..include.clone()
        })
    }
}
