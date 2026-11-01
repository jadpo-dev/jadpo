//! Reading a query's declaration: its sections, the entity `from:`
//! names, its result type and what each cardinality needs, and a map
//! layer's geometry (area 6, "Named queries"; chunk 5.5u8).

use super::*;

impl<'a> Reader<'a> {
    pub(super) fn span(path: &str, node: &impl AstNode) -> Span {
        node_span(path, node.syntax())
    }

    pub(super) fn query(&mut self, symbol: &Symbol, decl: &ast::QueryDecl) -> Option<QueryModel> {
        let mut model = self.read_query(symbol, decl)?;
        let list = decl.member_list();
        let written =
            list.iter()
                .flat_map(|list| list.member_items())
                .find_map(|item| match item {
                    MemberItem::Member(member)
                        if member
                            .member_key()
                            .and_then(|key| key.names().next())
                            .and_then(|name| tree::name_text(&name))
                            .as_deref()
                            == Some("include_erased") =>
                    {
                        Some(member)
                    }
                    _ => None,
                });
        if let Some(member) = written {
            model.include_erased = self.include_erased(symbol.path.as_str(), &model, &member);
        }
        Some(model)
    }

    /// `include_erased: true` (area 15, "The person's own record"; chunk
    /// P4b): the query finds the people erasure keeps a record of too, so it
    /// reads nothing but their `id` and the fields erasure keeps, and
    /// brings no other records with them. Whether it says so, with each
    /// problem in what it reads reported.
    fn include_erased(&mut self, path: &str, model: &QueryModel, member: &ast::Member) -> bool {
        let key = member
            .member_key()
            .map_or_else(|| Span::new(path, 0, 0), |key| Self::span(path, &key));
        let value = member.value();
        if !value.as_ref().is_some_and(|value| {
            crate::types::Const::from_expr(value) == crate::types::Const::Bool(true)
        }) {
            let at = value
                .as_ref()
                .map_or_else(|| key.clone(), |value| Self::span(path, value));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_SETTING_VALUE,
                    "`include_erased:` takes only `true`",
                )
                .at(at, "not `true`")
                .reason("`include_erased: true` says a query also finds the people erasure keeps a record of; a query that leaves them out, as every query does, leaves the line out.")
                .next("Write `include_erased: true`, or remove the line"),
            );
            return false;
        }
        let name = &model.name;
        let entity = &model.entity;
        let Some(found) = self.entities.get(entity) else {
            return false;
        };
        if !found.keeps_erased() {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_ERASED_UNUSED,
                    format!(
                        "`{name}` includes erased people, and erasing a `{entity}` keeps nothing to find"
                    ),
                )
                .at(key, "nothing to include")
                .reason(format!(
                    "Erasing a `{entity}` deletes their whole record, since none of its fields says `erase: Erasure.retained`, so nothing is left for `include_erased:` to find."
                ))
                .next("Remove `include_erased: true`"),
            );
            return false;
        }
        if !model.includes.is_empty() {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_ERASED_READS,
                    format!(
                        "`{name}` brings other records with what erasure keeps of a `{entity}`"
                    ),
                )
                .at(key.clone(), "reads what erasure kept")
                .reason("`include_erased:` lets a query read a person's kept record for the reason the law keeps it, and nothing else: their `id` and the fields that say `erase: Erasure.retained`. An include reads more.")
                .next("Read the other records in a query of their own that leaves erased people out"),
            );
        }
        let kept = |field: &str| {
            field == "id"
                || found.fields.iter().any(|f| {
                    f.name == field
                        && f.reference.is_none()
                        && f.erase == Some(jadpo_std::Erasure::Retained)
                })
        };
        let mut reads: BTreeSet<String> = BTreeSet::new();
        for (_, source) in &model.element.fields {
            match source {
                Source::Column(field) => {
                    reads.insert(field.clone());
                }
                Source::Entry {
                    field: Some(field), ..
                } => {
                    reads.insert(field.clone());
                }
                Source::Entry { field: None, .. } | Source::Include(_) => {}
            }
        }
        for condition in &model.filter {
            condition.fields(&mut reads);
        }
        reads.extend(model.order.iter().map(|order| order.field.clone()));
        reads.extend(model.nearest.iter().map(|order| order.field.clone()));
        reads.extend(model.distance.iter().map(|order| order.field.clone()));
        reads.extend(model.geometry.iter().cloned());
        if let Some(grouping) = &model.grouping {
            reads.extend(
                grouping
                    .entries
                    .iter()
                    .filter_map(|(_, entry)| entry.field().map(str::to_string)),
            );
        }
        for field in reads.iter().filter(|field| !kept(field)) {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_ERASED_READS,
                    format!(
                        "`{name}` reads `{entity}.{field}`, which isn't `erase: Erasure.retained`"
                    ),
                )
                .at(key.clone(), "reads what erasure kept")
                .reason("`include_erased:` lets a query read a person's kept record for the reason the law keeps it, and nothing else: their `id` and the fields that say `erase: Erasure.retained`. Erasure clears a marked field, and an unmarked one isn't kept for that reason.")
                .next(format!(
                    "Stop reading `{field}` here: leave it out of the result, the conditions and the order"
                )),
            );
        }
        true
    }

    fn read_query(&mut self, symbol: &Symbol, decl: &ast::QueryDecl) -> Option<QueryModel> {
        let path = symbol.path.as_str();
        let name_node = decl.name()?;
        let name = tree::name_text(&name_node)?;
        let span = Self::span(path, &name_node);
        let sig = self.lookup.signature(&name)?.clone();
        let scope = Scope {
            path,
            params: &sig.params,
        };
        let list = decl.member_list();
        let sections = self.sections(path, list.clone(), &SECTIONS, "query", &name);

        // `from:` names the entity read.
        let Some(from) = sections.first.get("from") else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_SECTION_REQUIRED,
                    format!("`{name}` doesn't say what it reads"),
                )
                .at(span.clone(), "no `from:`")
                .reason("A query reads one entity, named in `from:`; its conditions, order and includes are about that entity's records.")
                .next("Add `from: <Entity>` as the query's first line"),
            );
            return None;
        };
        let entity = self.from(path, from, symbol)?;

        // The result type says how many records come back.
        let result_span = decl
            .ret_type()
            .and_then(|ret| ret.ty())
            .map_or_else(|| span.clone(), |ty| Self::span(path, &ty));
        let (cardinality, element_ty, nearest_result, layer) =
            self.result(&name, &sig.result, entity, &result_span)?;
        // A result type with rules across its fields (chunk 5.8c).
        if let Some((ty, rule)) = self.result_rule(&element_ty, &mut Vec::new()) {
            self.result_rule_refused(&name, &ty, &rule, &result_span);
            return None;
        }
        // A map layer (chunk 5.5u8) names its features' geometry field, and
        // only a map layer does.
        self.layer = layer.is_some();
        let mut layer_ok = true;
        if let Some(member) = sections.first.get("geometry")
            && layer.is_none()
        {
            layer_ok = false;
            self.section_unexpected(path, member, "geometry", cardinality, &sig.result);
        }
        if layer.is_some() {
            for section in ["group", "aggregate", "include"] {
                if let Some(member) = sections.first.get(section) {
                    layer_ok = false;
                    self.layer_unexpected(path, member, section, &sig.result);
                }
            }
            if !layer_ok {
                return None;
            }
        }
        // A nearest-neighbour query (area 21): `nearest` in `order:`, and
        // `List<Nearest<T>>` as its result.
        let nearest_written = sections.first.get("order").and_then(nearest_entry);
        if (nearest_written.is_some() || nearest_result) && cardinality == Cardinality::Page {
            self.nearest_page(&name, &result_span, &element_ty);
            return None;
        }
        // An aggregate query (chunk 5.6c): `group:` or `aggregate:`.
        if sections.first.contains_key("group") || sections.first.contains_key("aggregate") {
            let shape = aggregates::Shape {
                name: &name,
                span: &span,
                result: &sig.result,
                result_span: &result_span,
                cardinality,
                element: &element_ty,
                nearest: nearest_written.is_some() || nearest_result,
            };
            return self.aggregate_query(symbol, decl, &scope, entity, &sections, &shape);
        }
        if let Some(problem) = self.nearest_result(
            &name,
            &sig.result,
            &result_span,
            (nearest_written.is_some(), nearest_result, cardinality),
            &element_ty,
        ) {
            self.out.push(problem);
            return None;
        }

        // What each cardinality needs and refuses. A search by words
        // (area 16, "Search"; chunk B12) needs no `order:`: without one,
        // the best matches come first.
        let with_matches = sections
            .filters
            .iter()
            .any(|(_, member)| member.value().is_some_and(|value| holds_matches(&value)));
        let (needs, refuses): (&[&str], &[&str]) = match (cardinality, with_matches) {
            (Cardinality::One, _) => (&["missing"], &["order", "limit", "page"]),
            (Cardinality::Optional, _) => (&[], &["missing", "order", "limit", "page"]),
            (Cardinality::List, false) => (&["order", "limit"], &["page", "missing"]),
            (Cardinality::List, true) => (&["limit"], &["page", "missing"]),
            (Cardinality::Page, false) => (&["order", "page"], &["limit", "missing"]),
            (Cardinality::Page, true) => (&["page"], &["limit", "missing"]),
        };
        let mut complete = true;
        for section in needs {
            if sections.first.contains_key(*section) {
                continue;
            }
            complete = false;
            if !sections.suggested.contains(*section) {
                self.section_required(&name, &span, cardinality, &sig.result, section);
            }
        }
        for section in refuses {
            if let Some(member) = sections.first.get(*section) {
                self.section_unexpected(path, member, section, cardinality, &sig.result);
            }
        }

        // Conditions.
        let mut filter = Vec::new();
        for (present, member) in &sections.filters {
            if let Some(condition) = self.filter_line(&scope, entity, member, *present) {
                filter.push(condition);
            }
        }
        if matches!(cardinality, Cardinality::One | Cardinality::Optional) {
            self.unique(path, entity, &sections, &filter, &span, &sig.result);
        }
        // One search a query, ordering its records by how well they match,
        // which a nearest or distance order, or a map layer, would not.
        let found = searches(&filter);
        let mut search_ok = true;
        if found.len() > 1 {
            search_ok = false;
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    format!("`{name}` searches by words twice"),
                )
                .at(span.clone(), "two `matches`")
                .reason("A search ranks the records it finds by how well they match one phrase, so a query holds one `matches`; one `SearchText` takes several words, quoted phrases and `-word`.")
                .next("Keep one `matches` line"),
            );
        } else if !found.is_empty()
            && (nearest_written.is_some()
                || sections
                    .first
                    .get("order")
                    .and_then(distance_entry)
                    .is_some()
                || layer.is_some())
        {
            search_ok = false;
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    format!("`{name}` searches by words and orders by nearness"),
                )
                .at(span.clone(), "`matches` and a nearest order")
                .reason("A search orders its records by how well they match, or by the fields `order:` lists; a `nearest` or `geo.distance` order, or a map layer, orders them by place, and the two can't be combined.")
                .next("Search in a query of its own"),
            );
        }

        // Order, limit and page. A query ordered by `geo.distance` (area
        // 20, question 4) has that order alone.
        let distance_written = sections.first.get("order").and_then(distance_entry);
        let mut distance_ok = true;
        let distance = match (sections.first.get("order"), &distance_written) {
            (Some(member), Some(_)) if !refuses.contains(&"order") => {
                let found = self.distance_order(&scope, entity, member, cardinality, &filter);
                distance_ok = found.is_some();
                found
            }
            _ => None,
        };
        let order = match sections.first.get("order") {
            Some(_) if nearest_written.is_some() || distance_written.is_some() => Vec::new(),
            Some(member) if !refuses.contains(&"order") => {
                self.order(path, entity, member, cardinality == Cardinality::Page)
            }
            _ => Vec::new(),
        };
        let limit = match (cardinality, sections.first.get("limit")) {
            (Cardinality::List, Some(member)) => {
                self.bound(&scope, member.value(), member, "limit")
            }
            _ => None,
        };
        let mut nearest_ok = true;
        let nearest = match (sections.first.get("order"), &nearest_written) {
            (Some(member), Some(_)) => {
                let found = self.nearest(&scope, entity, member, sections.first.get("limit"));
                nearest_ok = found.is_some();
                found
            }
            _ => None,
        };
        let (size, after) = match (cardinality, sections.first.get("page")) {
            (Cardinality::Page, Some(member)) => self.page(&scope, member),
            _ => (None, None),
        };

        // Includes and the result's fields.
        let includes = match sections.first.get("include") {
            Some(member) => self.includes(&scope, entity, member, 1, symbol),
            None => Vec::new(),
        };
        let element = self.element(
            path,
            &element_ty,
            entity,
            &includes,
            sections.first.get("include"),
            &result_span,
        );
        // The includes with their own records' fields filled in.
        let (element, includes) = match element {
            Some((element, filled)) => (Some(element), filled),
            None => (None, includes),
        };

        // `missing:` and the `fails` list.
        let missing = match (cardinality, sections.first.get("missing")) {
            (Cardinality::One, Some(member)) => self.missing(&scope, member),
            _ => None,
        };
        self.fails(
            path,
            decl,
            &name,
            missing.as_ref(),
            sections.first.get("missing"),
        );

        // A map layer's geometry: `geometry: Entity.field`.
        let geometry = match (&layer, sections.first.get("geometry")) {
            (Some(g), Some(member)) => self.geometry(path, entity, member, g, &sig.result),
            (Some(_), None) => {
                if !sections.suggested.contains("geometry") {
                    self.section_required(&name, &span, cardinality, &sig.result, "geometry");
                }
                None
            }
            (None, _) => None,
        };

        let element = element?;
        if !complete || !nearest_ok || !distance_ok || !layer_ok || !search_ok {
            return None;
        }
        if layer.is_some() && geometry.is_none() {
            return None;
        }
        let key = self.lookup.registry().key(Kind::Query, &name, None);
        Some(QueryModel {
            name,
            key,
            path: path.to_string(),
            entity: entity.name.clone(),
            params: sig.params.clone(),
            cardinality,
            element,
            filter,
            order,
            limit: limit.or(size),
            after,
            includes,
            missing,
            nearest,
            distance,
            grouping: None,
            geometry,
            include_erased: false,
        })
    }

    /// The members of a query's or include's block, each section once
    /// except the condition lines, which stack.
    pub(super) fn sections(
        &mut self,
        path: &str,
        list: Option<ast::MemberList>,
        known: &[&str],
        what: &str,
        name: &str,
    ) -> Sections {
        let mut out = Sections::default();
        for item in list.iter().flat_map(|list| list.member_items()) {
            let (node, written) = match &item {
                MemberItem::Member(member) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let names: Vec<String> =
                        key.names().filter_map(|n| tree::name_text(&n)).collect();
                    match names.as_slice() {
                        [word] if known.contains(&word.as_str()) => {
                            if word == "where" || word == "where_present" {
                                out.filters.push((word == "where_present", member.clone()));
                            } else if let Some(earlier) = out.first.get(word) {
                                let earlier = earlier.member_key().map_or_else(
                                    || Span::new(path, 0, 0),
                                    |k| Self::span(path, &k),
                                );
                                self.out.push(
                                    Diagnostic::new(
                                        &rule::QUERY_REPEATED_SECTION,
                                        format!("`{word}:` is given twice"),
                                    )
                                    .at(Self::span(path, &key), "given again")
                                    .also(earlier, "first given here")
                                    .reason(format!(
                                        "A {what} has one `{word}:`; only `where:` and `where_present:` lines stack, each one more condition."
                                    ))
                                    .next(format!("Remove one `{word}:`")),
                                );
                            } else {
                                out.first.insert(word.clone(), member.clone());
                            }
                            continue;
                        }
                        _ => (key.syntax().clone(), names.join(".")),
                    }
                }
                MemberItem::BareMember(bare) => {
                    (bare.syntax().clone(), tree::written(bare.syntax()))
                }
                MemberItem::Decl(decl) => (decl.syntax().clone(), first_words(decl.syntax())),
                MemberItem::RouteEntry(entry) => {
                    (entry.syntax().clone(), first_words(entry.syntax()))
                }
                // A rule here is reported where it is written (area 2).
                MemberItem::Elision(_) | MemberItem::RuleMember(_) => continue,
            };
            let span = node_span(path, &node);
            let near = nearest(&written, known.iter().copied())
                .map(str::to_string)
                .or_else(|| renamed(&written).map(str::to_string));
            if let Some(near) = &near {
                out.suggested.insert(near.clone());
            }
            let listed: Vec<String> = known.iter().map(|s| format!("`{s}:`")).collect();
            let problem = Diagnostic::new(
                &rule::QUERY_UNKNOWN_SECTION,
                format!("`{written}` isn't something a {what} holds"),
            )
            .at(span.clone(), "not a section")
            .reason(format!(
                "A {what}'s block holds {}.{}",
                crate::and_list(&listed),
                near.as_ref()
                    .map(|n| format!(" Did you mean `{n}:`?"))
                    .unwrap_or_default()
            ));
            self.out.push(match near {
                Some(near) if matches!(item, MemberItem::Member(_)) => {
                    problem.next(format!("Use `{near}:`")).edit(span, near)
                }
                _ => problem.next(format!("Remove it from `{name}`")),
            });
        }
        out
    }

    /// `from: Entity`, checked: an entity of the query's own domain.
    pub(super) fn from(
        &mut self,
        path: &str,
        member: &ast::Member,
        query: &Symbol,
    ) -> Option<&'a EntityModel> {
        let value = member.value();
        let span = value.as_ref().map_or_else(
            || {
                member
                    .member_key()
                    .map_or_else(|| Span::new(path, 0, 0), |k| Self::span(path, &k))
            },
            |v| Self::span(path, v),
        );
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let named = match &value {
            Some(Expr::NameRef(name)) => tree::name_ref_text(name),
            _ => None,
        };
        let symbol = named.as_ref().and_then(|name| {
            self.lookup
                .declared(name)
                .iter()
                .find(|symbol| symbol.library.is_none())
                .cloned()
        });
        match symbol {
            Some(symbol) if symbol.kind == ItemKind::Entity => {
                if let (Ownership::Domain(theirs), Ownership::Domain(ours)) =
                    (&symbol.ownership, &query.ownership)
                    && theirs != ours
                {
                    self.out.push(
                        Diagnostic::new(
                            &rule::QUERY_OTHER_DOMAIN,
                            format!("`{}` reads `{written}`, an entity of domain `{theirs}`", query.name),
                        )
                        .at(span, format!("domain `{theirs}`"))
                        .reason(format!(
                            "A query reads its own domain's entities only; another domain's data comes from that domain's public queries, so `{theirs}` decides what it shares."
                        ))
                        .next(format!(
                            "Call a public query of `{theirs}`, or move `{}` into `{theirs}`",
                            query.name
                        )),
                    );
                    return None;
                }
                self.entities.get(&symbol.name)
            }
            found => {
                let what = match found.as_ref().map(|s| &s.kind) {
                    Some(ItemKind::Type(_)) => format!("`{written}` is a type"),
                    Some(kind) => format!("`{written}` is {}", kind.with_a()),
                    None if named.is_some() => format!("Nothing called `{written}` is declared"),
                    None => format!("`{written}` isn't a name"),
                };
                let all = self.entities.all();
                let near = named
                    .as_deref()
                    .and_then(|n| nearest(n, all.entities.iter().map(|e| e.name.as_str())))
                    .map(str::to_string);
                let problem = Diagnostic::new(
                    &rule::QUERY_FROM_NOT_ENTITY,
                    format!("`from:` names `{written}`, which isn't an entity"),
                )
                .at(span.clone(), "not an entity")
                .reason(format!(
                    "{what}; a query reads stored records, so `from:` names an entity declared in the domain's `entities/` folder."
                ));
                self.out.push(match near {
                    Some(near) => problem.next(format!("Use `from: {near}`")).edit(span, near),
                    None => problem.next("Name the entity the query reads"),
                });
                None
            }
        }
    }

    /// The cardinality the result type gives, the type of its records,
    /// whether each is a `Nearest<T>`, and for a map layer, a
    /// `FeatureCollection<G, P>` (chunk 5.5u8), its geometry type `G`: a
    /// list of features, each record's `P`.
    pub(super) fn result(
        &mut self,
        name: &str,
        result: &Ty,
        entity: &EntityModel,
        span: &Span,
    ) -> Option<(Cardinality, String, bool, Option<Ty>)> {
        if result.has_error() {
            return None;
        }
        if let Ty::Opaque(collection, args) = result
            && collection == jadpo_std::standard::geo::FEATURE_COLLECTION
            && let [geometry, Ty::Named(properties)] = args.as_slice()
            && matches!(
                self.lookup.def(properties),
                Some(Def::Fields {
                    base: FieldsBase::Object,
                    ..
                })
            )
        {
            return Some((
                Cardinality::List,
                properties.clone(),
                false,
                Some(geometry.clone()),
            ));
        }
        let (cardinality, element) = match result {
            Ty::List(inner) => (Cardinality::List, inner.as_ref()),
            Ty::Opaque(page, args) if page == PAGE && args.len() == 1 => {
                (Cardinality::Page, &args[0])
            }
            Ty::Nullable(inner) => (Cardinality::Optional, inner.as_ref()),
            other => (Cardinality::One, other),
        };
        // Each record a `Nearest<T>` (area 21).
        let (element, nearest) = match element {
            Ty::Opaque(nearest, args) if nearest == NEAREST && args.len() == 1 => (&args[0], true),
            other => (other, false),
        };
        if element.has_error() {
            return None;
        }
        let fits = match element {
            Ty::Named(ty) if *ty == entity.name => true,
            Ty::Named(ty) => matches!(
                self.lookup.def(ty),
                Some(Def::Fields {
                    base: FieldsBase::Object | FieldsBase::Output,
                    ..
                })
            ),
            _ => false,
        };
        if let (true, Ty::Named(ty)) = (fits, element) {
            return Some((cardinality, ty.clone(), nearest, None));
        }
        let shown = if matches!(result, Ty::Prim(Prim::Unit)) {
            "nothing".to_string()
        } else {
            format!("`{result}`")
        };
        let entity_name = &entity.name;
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_RESULT_TYPE,
                format!("`{name}` reads `{entity_name}` records but returns {shown}"),
            )
            .at(span.clone(), "not records of the entity")
            .reason(format!(
                "A query's result is `{entity_name}` or an object type its fields fill (`type {entity_name}View = Output {{ pick: [...] }}`), as `T` for exactly one record, `T?` for at most one, `List<T>` or `Page<T>`, or a map layer, `FeatureCollection<G, P>`."
            ))
            .next(format!(
                "Return `{entity_name}`, or an `Output` type that picks the fields you need"
            )),
        );
        None
    }

    /// The first type among the records a query returns, `ty` and the
    /// object types its fields hold (an include's records), that names a
    /// rule across its fields, with that rule's name (chunk 5.8c).
    pub(super) fn result_rule(&self, ty: &str, seen: &mut Vec<String>) -> Option<(String, String)> {
        if seen.iter().any(|s| s == ty) {
            return None;
        }
        seen.push(ty.to_string());
        let Some(Def::Fields { fields, rules, .. }) = self.lookup.def(ty) else {
            return None;
        };
        if let Some(rule) = rules.first() {
            return Some((ty.to_string(), rule.name.clone()));
        }
        let fields = fields.clone();
        for field in &fields {
            let mut inner = &field.declared;
            loop {
                inner = match inner {
                    Ty::List(item) | Ty::Nullable(item) => item.as_ref(),
                    Ty::Opaque(_, args) if args.len() == 1 => &args[0],
                    _ => break,
                };
            }
            if let Ty::Named(name) = inner
                && let Some(found) = self.result_rule(name, seen)
            {
                return Some(found);
            }
        }
        None
    }

    /// `query.result_rule` (chunk 5.8c): a read makes its values from
    /// stored records with no `attempt` around it, so it can't check, or
    /// report, a rule across a result's fields.
    pub(super) fn result_rule_refused(&mut self, name: &str, ty: &str, rule: &str, span: &Span) {
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_RESULT_RULE,
                format!("`{name}` returns `{ty}`, whose rule `{rule}` no read can check"),
            )
            .at(span.clone(), format!("`{ty}` has rules"))
            .reason(format!(
                "A rule across an object's fields is checked whenever a value is made, and a value that breaks it is a failure an `attempt` maps; a query makes `{ty}` from stored records with no `attempt` around it, so its result type has no rules across its fields."
            ))
            .next(format!(
                "Return a type without rules, and build `{ty}` from it with `attempt` where its rules are checked"
            )),
        );
    }

    pub(super) fn section_required(
        &mut self,
        name: &str,
        span: &Span,
        cardinality: Cardinality,
        result: &Ty,
        section: &str,
    ) {
        let (reason, next) = match section {
            "missing" => (
                format!(
                    "`{result}` is exactly one record, so the query says which failure a caller gets when no record matches: a `not_found` failure in `missing:`. A query that may find nothing returns `{result}?`."
                ),
                "Add `missing: <Entity>NotFound`, or return an optional result".to_string(),
            ),
            "order" => (
                format!(
                    "A {} of records comes back in a fixed order, so its pages and items never change places between calls; `order:` names the fields to sort by.",
                    cardinality.word()
                ),
                "Add `order: [<Entity>.<field> asc]`".to_string(),
            ),
            "limit" => (
                "Every list is bounded: `limit:` is a number, or a value whose type has a `max:`.".to_string(),
                "Add `limit: <number>`, or return `Page<T>` to read it a page at a time".to_string(),
            ),
            "geometry" => (
                "A map layer's features each hold one geometry, the stored field `geometry:` names, with their properties filled from the record's other fields.".to_string(),
                "Add `geometry: <Entity>.<field>`, naming the field each feature's geometry is".to_string(),
            ),
            _ => (
                "A page query reads one page at a time: `page:` gives its `size:` and the cursor to continue `after:`.".to_string(),
                "Add `page: { size: ... after: ... }`".to_string(),
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_REQUIRED,
                format!("`{name}` returns `{result}` but has no `{section}:`"),
            )
            .at(span.clone(), format!("no `{section}:`"))
            .reason(reason)
            .next(next),
        );
    }

    pub(super) fn section_unexpected(
        &mut self,
        path: &str,
        member: &ast::Member,
        section: &str,
        cardinality: Cardinality,
        result: &Ty,
    ) {
        let Some(key) = member.member_key() else {
            return;
        };
        let reason = match (section, cardinality) {
            ("missing", Cardinality::Optional) => format!(
                "`{result}` already says the record may be absent: the caller gets `none`, so there is no failure to name."
            ),
            ("missing", _) => format!(
                "`missing:` names the failure for a query that returns exactly one record; `{result}` holds as many as match, none included."
            ),
            ("order" | "limit", Cardinality::One | Cardinality::Optional) => format!(
                "`{result}` is at most one record, found by a unique field, so there is nothing to order or limit."
            ),
            ("limit", _) => {
                "A page's size is its `page:` block's `size:`; `limit:` bounds a `List<T>`."
                    .to_string()
            }
            ("page", Cardinality::List) => format!(
                "`page:` belongs to a query returning `Page<T>`; `{result}` is one bounded list, sized by `limit:`."
            ),
            ("geometry", _) => format!(
                "`geometry:` names the field each feature of a map layer holds, for a query returning `FeatureCollection<G, P>`; `{result}` holds records, not features."
            ),
            _ => format!("`{result}` is at most one record, so it has no pages."),
        };
        let removal = Self::span(path, member);
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_UNEXPECTED,
                format!("A query returning `{result}` takes no `{section}:`"),
            )
            .at(Self::span(path, &key), "not for this result")
            .reason(reason)
            .next(format!("Remove `{section}:`"))
            .edit(removal, ""),
        );
    }

    /// A section a map layer doesn't take (chunk 5.5u8): its features are
    /// its records, each its own fields, so it neither groups them nor
    /// includes others.
    pub(super) fn layer_unexpected(
        &mut self,
        path: &str,
        member: &ast::Member,
        section: &str,
        result: &Ty,
    ) {
        let Some(key) = member.member_key() else {
            return;
        };
        let reason = match section {
            "include" => {
                "A map layer's properties are each record's own fields, picked as they are stored; a related record's fields come from a query of their own."
            }
            _ => {
                "A map layer's features are its records, one each; counting or adding up records is a query of its own."
            }
        };
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_UNEXPECTED,
                format!("A query returning `{result}` takes no `{section}:`"),
            )
            .at(Self::span(path, &key), "not for a map layer")
            .reason(reason)
            .next(format!("Remove `{section}:`"))
            .edit(Self::span(path, member), ""),
        );
    }

    /// `geometry: Entity.field` (chunk 5.5u8): the field each feature of a
    /// map layer holds as its geometry, of the layer's geometry type `G`
    /// itself and never `none`.
    pub(super) fn geometry(
        &mut self,
        path: &str,
        entity: &EntityModel,
        member: &ast::Member,
        g: &Ty,
        result: &Ty,
    ) -> Option<String> {
        let value = member.value()?;
        let span = Self::span(path, &value);
        let written = tree::written(value.syntax());
        let named = dotted(&value).is_some_and(|(_, rest)| rest.len() == 1);
        if !named {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_GEOMETRY_FIELD,
                    format!("`geometry:` is `{written}`, not a field"),
                )
                .at(span, "not a field")
                .reason(format!(
                    "Each feature's geometry is a stored field of `{}`, picked as it is, so `geometry:` names one with its entity; a query computes nothing.",
                    entity.name
                ))
                .next(format!("Write `geometry: {}.<field>`", entity.name)),
            );
            return None;
        }
        let field = self.field_ref(path, entity, &value)?;
        if field.sealed.is_some() {
            self.out.push(crate::data::sealed::used(
                &entity.name,
                &field.name,
                "A map layer",
                Self::span(path, &value),
                "Name a geometry field that isn't encrypted".to_string(),
            ));
            return None;
        }
        if g.has_error() || field.ty.has_error() {
            return None;
        }
        let wanted = geo_kind(self.lookup, g);
        let found = geo_kind(self.lookup, &field.ty);
        let shown = format!("{}.{}", entity.name, field.name);
        if found.is_none() || found != wanted {
            let geometries: Vec<String> = entity
                .fields
                .iter()
                .filter(|f| !f.nullable && geo_kind(self.lookup, &f.ty) == wanted)
                .map(|f| format!("{}.{}", entity.name, f.name))
                .collect();
            let problem = Diagnostic::new(
                &rule::QUERY_GEOMETRY_FIELD,
                format!("`{shown}` isn't a `{g}`, so it can't be each feature's geometry"),
            )
            .at(span.clone(), format!("a `{}`", field_type_name(&field.ty)))
            .reason(format!(
                "`{result}`'s features each hold a `{g}`, and a stored value goes into the layer as it is, so `geometry:` names a field of that type."
            ));
            self.out.push(match geometries.as_slice() {
                [one] => problem.next(format!("Use `{one}`")).edit(span, one.clone()),
                _ => problem.next(format!(
                    "Name a `{g}` field of `{}`, or return the layer of the field's own type",
                    entity.name
                )),
            });
            return None;
        }
        if field.nullable {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_GEOMETRY_FIELD,
                    format!("`{shown}` may be `none`, and every feature has a geometry"),
                )
                .at(span, "may be `none`")
                .reason(format!(
                    "A feature of `{result}` holds a `{g}`, never `none`, so its geometry is a field every record has."
                ))
                .next(format!(
                    "Make `{}` required, or name a field every `{}` has",
                    field.name, entity.name
                )),
            );
            return None;
        }
        Some(field.name.clone())
    }
}
