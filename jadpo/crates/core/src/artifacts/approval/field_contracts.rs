//! Checked source contracts. Nominal types and syntax are not database origins.
use super::{canonical_bytes, contract_tokens, digest, AnalyzedProject, Value};
use jadpo_syntax::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
mod lexical;

fn path(names: &[Name]) -> String {
    names
        .iter()
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn contract_id(kind: &str, owner: &Value, address: &[String]) -> String {
    digest(canonical_bytes(&json!(["field_contract.v1", kind, owner, address])).as_bytes())
}

struct Context<'a> {
    project: &'a AnalyzedProject,
    // Compiler-local identifiers are lookup keys only, never public identities.
    generated: BTreeMap<(String, String), BTreeSet<Vec<String>>>,
    inline_records: BTreeMap<(String, String), &'a RecordDeclaration>,
    authored_names: BTreeSet<String>,
    known_types: BTreeSet<String>,
}

impl<'a> Context<'a> {
    fn new(project: &'a AnalyzedProject) -> Self {
        let mut context = Self {
            project,
            generated: BTreeMap::new(),
            inline_records: BTreeMap::new(),
            authored_names: BTreeSet::new(),
            known_types: project
                .semantics
                .nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        super::NodeKind::PreludeType
                            | super::NodeKind::Type
                            | super::NodeKind::Enum
                            | super::NodeKind::Principal
                            | super::NodeKind::PrincipalVariant
                            | super::NodeKind::Entity
                            | super::NodeKind::EntityReference
                            | super::NodeKind::Value
                            | super::NodeKind::Input
                            | super::NodeKind::Output
                            | super::NodeKind::Field
                    )
                })
                .map(|node| node.name.clone())
                .collect(),
        };
        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                match declaration {
                    Declaration::Record(record) => {
                        let lowered_inline = record.kind == RecordKind::Value
                            && record.name.text != "Object"
                            && source.tokens.iter().any(|token| {
                                token.range == record.name.range
                                    && token.text(&source.source_text) == "Object"
                            });
                        if lowered_inline {
                            let key = (source.source_name.clone(), record.name.text.clone());
                            context.inline_records.insert(key.clone(), record);
                            context.generated.entry(key).or_default();
                        } else {
                            context.authored_names.insert(record.name.text.clone());
                        }
                    }
                    Declaration::Type(declaration) => {
                        context.authored_names.insert(declaration.name.text.clone());
                    }
                    Declaration::Enum(declaration) => {
                        context.authored_names.insert(declaration.name.text.clone());
                        for variant in &declaration.variants {
                            context
                                .authored_names
                                .insert(format!("{}.{}", declaration.name.text, variant.name.text));
                        }
                    }
                    _ => {}
                }
            }
        }
        // Traverse authored declaration/type-reference structure. Nested parser
        // field_owners strings can contain counters, so they are not identities.
        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                match declaration {
                    Declaration::Record(record)
                        if !context.inline_records.contains_key(&(
                            source.source_name.clone(),
                            record.name.text.clone(),
                        )) =>
                    {
                        for field in &record.fields {
                            context.associate(
                                source,
                                &field.field_type,
                                vec![
                                    "record".into(),
                                    record.name.text.clone(),
                                    "field".into(),
                                    field.name.text.clone(),
                                    "type".into(),
                                ],
                                &mut BTreeSet::new(),
                            );
                        }
                    }
                    Declaration::Callable(callable) => {
                        for parameter in &callable.parameters {
                            context.associate(
                                source,
                                &parameter.parameter_type,
                                vec![
                                    "callable".into(),
                                    callable.name.text.clone(),
                                    "parameter".into(),
                                    parameter.name.text.clone(),
                                ],
                                &mut BTreeSet::new(),
                            );
                        }
                        context.associate(
                            source,
                            &callable.return_type,
                            vec![
                                "callable".into(),
                                callable.name.text.clone(),
                                "return".into(),
                            ],
                            &mut BTreeSet::new(),
                        );
                    }
                    Declaration::Enum(declaration) => {
                        for variant in &declaration.variants {
                            for field in &variant.fields {
                                context.associate(
                                    source,
                                    &field.field_type,
                                    vec![
                                        "enum".into(),
                                        declaration.name.text.clone(),
                                        variant.name.text.clone(),
                                        "field".into(),
                                        field.name.text.clone(),
                                    ],
                                    &mut BTreeSet::new(),
                                );
                            }
                        }
                    }
                    Declaration::Route(route) => {
                        let owner = format!("{} {}", super::method_name(route.method), route.path);
                        for (surface, reference) in [
                            ("input", &route.input),
                            ("query", &route.query),
                            ("output", &route.output),
                        ] {
                            if let Some(reference) = reference {
                                context.associate(
                                    source,
                                    reference,
                                    vec!["route".into(), owner.clone(), surface.into()],
                                    &mut BTreeSet::new(),
                                );
                            }
                        }
                        for (surface, fields) in [("path", &route.path_fields)] {
                            for field in fields {
                                context.associate(
                                    source,
                                    &field.field_type,
                                    vec![
                                        "route".into(),
                                        owner.clone(),
                                        surface.into(),
                                        field.name.text.clone(),
                                    ],
                                    &mut BTreeSet::new(),
                                );
                            }
                        }
                        for header in &route.headers {
                            context.associate(
                                source,
                                &header.field_type,
                                vec![
                                    "route".into(),
                                    owner.clone(),
                                    "header".into(),
                                    header.name.text.clone(),
                                ],
                                &mut BTreeSet::new(),
                            );
                        }
                        for (surface, present) in [
                            ("path", !route.path_fields.is_empty()),
                            ("headers", !route.headers.is_empty()),
                        ] {
                            if present {
                                context
                                    .generated
                                    .entry((
                                        source.source_name.clone(),
                                        format!("__route_{surface}_{}", route.range.start),
                                    ))
                                    .or_default()
                                    .insert(vec!["route".into(), owner.clone(), surface.into()]);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        context
    }

    fn associate(
        &mut self,
        source: &ParsedSyntax,
        reference: &TypeReference,
        address: Vec<String>,
        visited: &mut BTreeSet<String>,
    ) {
        let name = path(&reference.path);
        let key = (source.source_name.clone(), name.clone());
        if let Some(record) = self.inline_records.get(&key).copied() {
            self.generated
                .entry(key)
                .or_default()
                .insert(address.clone());
            if visited.insert(name.clone()) {
                for field in &record.fields {
                    let mut child = address.clone();
                    child.extend(["field".into(), field.name.text.clone(), "type".into()]);
                    self.associate(source, &field.field_type, child, visited);
                }
                visited.remove(&name);
            }
        }
        for (index, argument) in reference.arguments.iter().enumerate() {
            let mut child = address.clone();
            child.extend(["argument".into(), index.to_string()]);
            self.associate(source, argument, child, visited);
        }
    }

    fn symbol(&self, source: &ParsedSyntax, name: &str) -> Value {
        // Exact components/prefixes at '.' boundaries only, never arbitrary
        // substring replacement in a type name or authored expression/literal.
        let matched = self
            .generated
            .iter()
            .filter(|((file, generated), _)| {
                file == &source.source_name
                    && (name == generated
                        || name
                            .strip_prefix(generated.as_str())
                            .is_some_and(|suffix| suffix.starts_with('.')))
            })
            .max_by_key(|((_, generated), _)| generated.len());
        if let Some(((_, generated), addresses)) = matched {
            if self.authored_names.contains(generated) {
                return json!({"status":"unresolved","reason":"authored_and_generated_type_name_collision"});
            }
            let suffix = name
                .strip_prefix(generated.as_str())
                .unwrap()
                .trim_start_matches('.');
            if addresses.len() == 1 {
                return json!({"status":"checked_structural_shape","address":addresses.first().unwrap(),
                    "field_suffix":if suffix.is_empty() { vec![] } else { suffix.split('.').collect::<Vec<_>>() }});
            }
            return json!({"status":"unresolved","reason":if addresses.is_empty() { "generated_shape_owner_unavailable" } else { "generated_shape_owner_ambiguous" }});
        }
        json!({"status":"authored_or_checked_named_contract","name":name})
    }

    fn declared_type(&self, source: &ParsedSyntax, reference: &TypeReference) -> Value {
        json!({"evidence":"checked_declared_contract","symbol":self.symbol(source,&path(&reference.path)),
            "arguments":reference.arguments.iter().map(|argument|self.declared_type(source,argument)).collect::<Vec<_>>(),
            "nullable":reference.nullable,"effective_nullable":self.nullable(reference),"tokens":contract_tokens(source,reference.range)})
    }

    fn nullable(&self, reference: &TypeReference) -> bool {
        // Use the same checked inherited-nullability catalogue as the checker;
        // a nominal Customer.nickname can include absence without a local '?'.
        reference.nullable
            || self
                .project
                .semantics
                .nullable_types
                .contains(&path(&reference.path))
    }

    fn display(&self, source: &ParsedSyntax, display: &str) -> Vec<Value> {
        let mut components = Vec::new();
        let mut characters = display.chars().peekable();
        while let Some(character) = characters.next() {
            if character.is_whitespace() {
                continue;
            }
            if character.is_alphanumeric() || character == '_' {
                let mut name = character.to_string();
                while characters.peek().is_some_and(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | '.')
                }) {
                    name.push(characters.next().unwrap());
                }
                let symbol = self.symbol(source, &name);
                if symbol["status"] == "authored_or_checked_named_contract"
                    && !self.known_types.contains(&name)
                    && !self.authored_names.contains(&name)
                {
                    components.push(json!({"status":"unresolved","reason":"inferred_display_component_not_in_checked_catalogue"}));
                } else {
                    components.push(symbol);
                }
            } else {
                components.push(json!({"punctuation":character.to_string()}));
            }
        }
        components
    }

    fn observed_types(&self, source: &ParsedSyntax, range: TextRange) -> BTreeSet<String> {
        self.project
            .typing
            .expressions
            .iter()
            .filter(|observation| {
                observation.source == source.source_name && observation.range == range
            })
            .map(|observation| observation.type_name.clone())
            .collect()
    }

    fn expression(&self, source: &ParsedSyntax, expression: &Expression) -> Value {
        let observations = self.observed_types(source, expression.range());
        json!({"tokens":contract_tokens(source,expression.range()),"type_evidence":{
            "evidence":"observed_inferred_display","status":match observations.len() { 0=>"observation_missing",1=>"unique_observation",_=>"conflicting_observations" },
            "observations":observations.iter().map(|display|self.display(source,display)).collect::<Vec<_>>()},
            "data_origin":{"status":"not_resolved","reason":"syntax_and_nominal_type_do_not_prove_value_origin"}})
    }

    fn field(&self, source: &ParsedSyntax, field: &FieldDeclaration) -> Value {
        json!({"field":field.name.text,"optional":field.optional,"type":self.declared_type(source,&field.field_type),
            "reference":field.reference.as_ref().map(|reference|self.declared_type(source,&reference.target)),
            "declaration_tokens":contract_tokens(source,field.range),"evidence":"checked_declared_contract"})
    }

    fn catalogue(&self) -> BTreeMap<String, Value> {
        let mut contracts = BTreeMap::new();
        for (file_index, source) in self.project.syntax.sources.iter().enumerate() {
            for (declaration_index, declaration) in source.file.declarations.iter().enumerate() {
                match declaration {
                    Declaration::Record(record) => {
                        let symbol = self.symbol(source, &record.name.text);
                        let owner = if symbol["status"] == "unresolved" {
                            json!({"symbol":symbol,"unresolved_declaration_address":[file_index,declaration_index]})
                        } else {
                            symbol
                        };
                        for field in &record.fields {
                            let address = vec!["field".into(), field.name.text.clone()];
                            let id = contract_id("declaration_field", &owner, &address);
                            contracts.insert(id.clone(),json!({"id":id,"kind":"declaration_field","owner":owner,
                                "record_kind":format!("{:?}",record.kind).to_ascii_lowercase(),"contract":self.field(source,field)}));
                        }
                    }
                    Declaration::Enum(declaration) => {
                        for variant in &declaration.variants {
                            for field in &variant.fields {
                                let owner = json!({"name":format!("{}.{}",declaration.name.text,variant.name.text),"kind":"enum_payload"});
                                let id = contract_id(
                                    "declaration_field",
                                    &owner,
                                    &[field.name.text.clone()],
                                );
                                contracts.insert(id.clone(),json!({"id":id,"kind":"declaration_field","owner":owner,"record_kind":"enum_payload","contract":self.field(source,field)}));
                            }
                        }
                    }
                    Declaration::Type(declaration) => {
                        let owner = json!({"name":declaration.name.text,"kind":"scalar_alias"});
                        let id = contract_id("scalar_alias", &owner, &[]);
                        contracts.insert(id.clone(),json!({"id":id,"kind":"scalar_alias","owner":owner,
                            "parent":self.declared_type(source,&declaration.parent),"declaration_tokens":contract_tokens(source,declaration.range)}));
                    }
                    _ => {}
                }
            }
        }
        contracts
    }
}

fn child(address: &[String], parts: impl IntoIterator<Item = String>) -> Vec<String> {
    address.iter().cloned().chain(parts).collect()
}

// Each row is a checked source contract, not an executed flow. Ordered arrays
// retain evaluation/tuple order; maps below only sort independent facts.
struct Walker<'a, 'b> {
    context: &'b Context<'a>,
    source: &'a ParsedSyntax,
    owner: Value,
    rows: BTreeMap<String, Value>,
    lexical: lexical::Links,
}

impl Walker<'_, '_> {
    fn outcome_pattern(&self, pattern: &OutcomeMatchPattern) -> Value {
        json!({"kind":match pattern {OutcomeMatchPattern::Success(_)=>"success",OutcomeMatchPattern::Failure(_)=>"failure"},
            "tokens":contract_tokens(self.source,pattern.range())})
    }
    fn emit(&mut self, kind: &str, address: &[String], conditions: &[Value], contract: Value) {
        let id = contract_id(kind, &self.owner, address);
        let previous = self.rows.insert(
            id.clone(),
            json!({"id":id,"kind":kind,"owner":self.owner,
            "address":address,"conditions":conditions,"condition_status":"symbolic_not_proved",
            "contract":contract}),
        );
        assert!(
            previous.is_none(),
            "structural contract addresses must be unique"
        );
    }

    fn expr(&self, expression: &Expression) -> Value {
        let mut value = self.context.expression(self.source, expression);
        if let Expression::Name(name) = expression {
            value["source_binding_use"] = self.lexical.expression(name);
        }
        value
    }

    fn type_ref(&self, reference: &TypeReference) -> Value {
        self.context.declared_type(self.source, reference)
    }

    fn field_ref(&self, target: &str, field: &str) -> Value {
        let declarations = self
            .context
            .project
            .syntax
            .sources
            .iter()
            .flat_map(|source| {
                source
                    .file
                    .declarations
                    .iter()
                    .filter_map(move |declaration| match declaration {
                        Declaration::Record(record) if record.name.text == target => {
                            Some((source, &record.fields))
                        }
                        Declaration::Enum(declaration) => declaration
                            .variants
                            .iter()
                            .find(|variant| {
                                format!("{}.{}", declaration.name.text, variant.name.text) == target
                            })
                            .map(|variant| (source, &variant.fields)),
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        let fields = declarations
            .iter()
            .filter_map(|(source, fields)| {
                fields
                    .iter()
                    .find(|item| item.name.text == field)
                    .map(|item| (*source, item))
            })
            .collect::<Vec<_>>();
        if let [(source, field)] = fields.as_slice() {
            json!({"status":"checked_declared_field","target":self.context.symbol(source,target),
                "contract":self.context.field(source,field)})
        } else {
            json!({"status":"unresolved","target":self.context.symbol(self.source,target),"field":field,
                "reason":if fields.is_empty(){"checked_target_field_unavailable"}else{"checked_target_field_ambiguous"}})
        }
    }

    fn initializers(&self, target: Option<&str>, fields: &[FieldInitialiser]) -> Vec<Value> {
        fields
            .iter()
            .map(|field| {
                json!({"field":field.name.text,
            "target_field":target.map(|target|self.field_ref(target,&field.name.text)),
            "value":self.expr(&field.value)})
            })
            .collect()
    }

    fn reject(
        &mut self,
        reject: &RejectStatement,
        trigger: &str,
        address: &[String],
        conditions: &[Value],
    ) {
        self.emit(
            "failure_boundary",
            address,
            conditions,
            json!({"trigger":trigger,"failure":reject.failure.text,
            "values":self.initializers(None,&reject.values)}),
        );
        self.walk_fields(&reject.values, address, conditions, &[]);
    }

    fn conflicts(
        &mut self,
        conflicts: &[ConflictBinding],
        address: &[String],
        conditions: &[Value],
    ) {
        for (index, conflict) in conflicts.iter().enumerate() {
            let address = child(address, ["conflict".into(), index.to_string()]);
            let mut conditions = conditions.to_vec();
            conditions.push(json!({"trigger":"constraint_conflict","constraint":conflict.constraint.as_ref().map(|constraint|path(&constraint.path))}));
            self.reject(
                &conflict.rejection,
                "constraint_conflict",
                &address,
                &conditions,
            );
        }
    }

    fn walk_fields(
        &mut self,
        fields: &[FieldInitialiser],
        address: &[String],
        conditions: &[Value],
        target_path: &[String],
    ) {
        for (index, field) in fields.iter().enumerate() {
            let address = child(
                address,
                ["field".into(), field.name.text.clone(), index.to_string()],
            );
            let target_path = child(target_path, [field.name.text.clone()]);
            self.walk_expr(&field.value, &address, conditions, &target_path);
        }
    }

    fn patch(&self, patch: &NameExpression, entity: &str) -> Value {
        let observations = self.context.observed_types(self.source, patch.range);
        let occurrence = self.expr(&Expression::Name(patch.clone()));
        if observations.len() != 1 {
            return json!({"occurrence":occurrence,"status":"whole_patch_unresolved","reason":if observations.is_empty(){"patch_occurrence_type_observation_missing"}else{"patch_occurrence_type_observations_conflict"},"mappings":null});
        }
        let name = observations.first().unwrap();
        let records = self
            .context
            .project
            .syntax
            .sources
            .iter()
            .flat_map(|source| {
                source
                    .file
                    .declarations
                    .iter()
                    .filter_map(move |declaration| match declaration {
                        Declaration::Record(record)
                            if record.name.text == *name
                                && matches!(record.kind, RecordKind::Value | RecordKind::Input) =>
                        {
                            Some((source, record))
                        }
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        if let [(source, record)] = records.as_slice() {
            let mut mappings = record.fields.iter().map(|field|json!({"source_patch_field":self.context.field(source,field),
                "target_field":self.field_ref(entity,&field.name.text),
                "condition":{"kind":"supplied","path":patch.path.iter().map(|part|part.text.as_str()).chain(std::iter::once(field.name.text.as_str())).collect::<Vec<_>>(),"status":"symbolic_not_proved"},
                "omission":"no_assignment","present_null":if self.context.nullable(&field.field_type){"assignment_of_null"}else{"not_in_declared_type"},
                "data_origin":"not_resolved"})).collect::<Vec<_>>();
            // These are declarative same-name associations, not initializer
            // evaluation order. The checked patch record/entity are fixed here.
            mappings.sort_by_key(|mapping| {
                mapping["source_patch_field"]["field"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            });
            json!({"occurrence":occurrence,"status":"checked_patch_record_mapping","record":self.context.symbol(source,name),
                "record_kind":format!("{:?}",record.kind).to_ascii_lowercase(),"mappings":mappings})
        } else {
            json!({"occurrence":occurrence,"status":"whole_patch_unresolved","reason":if records.is_empty(){"unique_occurrence_display_does_not_resolve_checked_patch_record"}else{"checked_patch_record_ambiguous"},"mappings":null})
        }
    }

    fn block(&mut self, block: &Block, address: &[String], conditions: &[Value]) {
        for (index, statement) in block.statements.iter().enumerate() {
            let kind = match statement {
                Statement::Binding(_) => "binding",
                Statement::Assignment(_) => "assignment",
                Statement::Return(_) => "return",
                Statement::Reject(_) => "reject",
                Statement::If(_) => "if",
                Statement::Match(_) => "match",
                Statement::Assert(_) => "assert",
                Statement::AdvanceClock(_) => "advance_clock",
                Statement::Unsupported(_) => "unsupported",
            };
            let address = child(
                address,
                ["statement".into(), index.to_string(), kind.into()],
            );
            match statement {
                Statement::Binding(statement)=>{
                    self.emit("binding",&address,conditions,json!({"name":statement.name.text,"mutable":statement.mutable,
                        "annotation":statement.annotation.as_ref().map(|reference|self.type_ref(reference)),"value":self.expr(&statement.value),"alias_resolution":"not_performed"}));
                    self.walk_expr(&statement.value,&child(&address,["value".into()]),conditions,&[]);
                }
                Statement::Assignment(statement)=>{
                    self.emit("assignment",&address,conditions,json!({"target":statement.target.text,"value":self.expr(&statement.value),"alias_resolution":"not_performed"}));
                    self.walk_expr(&statement.value,&child(&address,["value".into()]),conditions,&[]);
                }
                Statement::Return(statement)=>{
                    self.emit("return_boundary",&address,conditions,json!({"value":self.expr(&statement.value),
                        "boundary":if matches!(statement.value,Expression::Construction(_)|Expression::Object(_)){"constructed_value"}else{"whole_value"},
                        "origin_resolution":"not_performed"}));
                    self.walk_expr(&statement.value,&child(&address,["value".into()]),conditions,&[]);
                }
                Statement::Reject(statement)=>self.reject(statement,"explicit_reject",&address,conditions),
                Statement::If(statement)=>{
                    self.emit("branch",&address,conditions,json!({"condition":self.expr(&statement.condition),"else_present":statement.else_block.is_some()}));
                    self.walk_expr(&statement.condition,&child(&address,["condition".into()]),conditions,&[]);
                    for (arm, block) in [("then",Some(&statement.then_block)),("else",statement.else_block.as_ref())] {
                        if let Some(block)=block {
                            let mut conditions=conditions.to_vec(); conditions.push(json!({"kind":"if","arm":arm,"condition":self.expr(&statement.condition)}));
                            self.block(block,&child(&address,[arm.into()]),&conditions);
                        }
                    }
                }
                Statement::Match(statement)=>{
                    self.emit("match",&address,conditions,json!({"subject":self.expr(&statement.subject),"patterns":statement.arms.iter().map(|arm|contract_tokens(self.source,arm.pattern.range())).collect::<Vec<_>>()}));
                    self.walk_expr(&statement.subject,&child(&address,["subject".into()]),conditions,&[]);
                    for (index,arm) in statement.arms.iter().enumerate() {
                        let mut conditions=conditions.to_vec(); conditions.push(json!({"kind":"match_arm","subject":self.expr(&statement.subject),"pattern":contract_tokens(self.source,arm.pattern.range())}));
                        self.block(&arm.body,&child(&address,["arm".into(),index.to_string()]),&conditions);
                    }
                }
                Statement::Assert(statement)=>{
                    self.emit("assert",&address,conditions,json!({"condition":self.expr(&statement.condition)}));
                    self.walk_expr(&statement.condition,&child(&address,["condition".into()]),conditions,&[]);
                }
                Statement::AdvanceClock(statement)=>{
                    self.emit("advance_clock",&address,conditions,json!({"duration":self.expr(&statement.duration)}));
                    self.walk_expr(&statement.duration,&child(&address,["duration".into()]),conditions,&[]);
                }
                Statement::Unsupported(statement)=>self.emit("unsupported_statement",&address,conditions,json!({"keyword":statement.keyword,"tokens":contract_tokens(self.source,statement.range),"resolution":"not_supported"})),
            }
        }
    }

    fn walk_expr(
        &mut self,
        expression: &Expression,
        address: &[String],
        conditions: &[Value],
        target_path: &[String],
    ) {
        // Even missing/conflicting inference retains the source occurrence.
        self.emit("expression", address, conditions, self.expr(expression));
        let detail = child(address, ["detail".into()]);
        match expression {
            Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
            Expression::Invocation(invocation) => {
                self.emit("call_boundary",&detail,conditions,json!({"callee":path(&invocation.callee.path),
                    "arguments":invocation.arguments.iter().map(|argument|self.expr(argument)).collect::<Vec<_>>(),
                    "named_arguments":self.initializers(None,&invocation.named_arguments),"result_origin":"not_resolved"}));
                for (index, argument) in invocation.arguments.iter().enumerate() {
                    self.walk_expr(
                        argument,
                        &child(address, ["argument".into(), index.to_string()]),
                        conditions,
                        &[],
                    );
                }
                self.walk_fields(&invocation.named_arguments, address, conditions, &[]);
            }
            Expression::TestCall(call) => self.walk_expr(
                &Expression::Invocation(call.invocation.clone()),
                &child(address, ["test_call".into()]),
                conditions,
                target_path,
            ),
            Expression::Construction(construction) => {
                let target = path(&construction.target.path);
                self.emit("constructor_mapping",&detail,conditions,json!({"target":self.context.symbol(self.source,&target),
                    "target_path":target_path,"fields":self.initializers(Some(&target),&construction.fields),"origin_resolution":"not_performed"}));
                self.walk_fields(
                    &construction.fields,
                    address,
                    conditions,
                    &child(target_path, [target]),
                );
            }
            Expression::Object(object) => {
                self.emit("object_mapping",&detail,conditions,json!({"target_path":target_path,"fields":self.initializers(None,&object.fields),"target_resolution":"structural_value_only"}));
                self.walk_fields(&object.fields, address, conditions, target_path);
            }
            Expression::Create(create) => {
                let target = path(&create.target.path);
                self.emit("create_fields",&detail,conditions,json!({"target":target,"fields":self.initializers(Some(&target),&create.fields),"conflict_count":create.conflicts.len()}));
                self.walk_fields(&create.fields, address, conditions, &[target]);
                self.conflicts(&create.conflicts, address, conditions);
            }
            Expression::Query(query) => {
                let target = path(&query.target.path);
                let order = |order: &QueryOrder| json!({"field":self.field_ref(&target,&order.field.text),"direction":format!("{:?}",order.direction).to_ascii_lowercase()});
                let page=query.page.as_ref().map(|page|json!({"result":self.type_ref(&page.result),"projection":self.type_ref(&page.projection),"cursor":self.type_ref(&page.cursor),
                    "predicates":page.predicates.iter().map(|predicate|json!({"field":self.field_ref(&target,&predicate.field.text),"operator":format!("{:?}",predicate.operator),"value":self.expr(&predicate.value)})).collect::<Vec<_>>(),
                    "order":page.order.iter().map(order).collect::<Vec<_>>(),"cursor_fields":page.cursor_fields.iter().map(|field|self.field_ref(&target,&field.text)).collect::<Vec<_>>(),
                    "after":self.expr(&page.after),"after_optional":page.after_optional,"limit":self.expr(&page.limit),"evidence":"checked_declared_page_contract"}));
                let includes=query.includes.iter().map(|include|json!({"relationship":include.relationship.text,"nested_relationship":include.nested_relationship.as_ref().map(|name|&name.text),
                    "cardinality":format!("{:?}",include.cardinality).to_ascii_lowercase(),"result":self.type_ref(&include.result),
                    "order":if include.cardinality==QueryIncludeCardinality::Many {json!({"field":include.order.field.text,"direction":format!("{:?}",include.order.direction).to_ascii_lowercase()})}else{Value::Null},
                    "pagination":if include.cardinality==QueryIncludeCardinality::Many {json!({"limit":self.expr(&include.pagination.limit),"offset":self.expr(&include.pagination.offset)})}else{Value::Null},
                    "bounds_evidence":if include.cardinality==QueryIncludeCardinality::Many{"checked_authored_many_bounds"}else{"no_authored_order_or_pagination"},"execution":"not_proved"})).collect::<Vec<_>>();
                self.emit("query_fields",&detail,conditions,json!({"target":target,"cardinality":format!("{:?}",query.cardinality).to_ascii_lowercase(),
                    "predicate":if query.page.is_none(){json!({"field":self.field_ref(&target,&query.field.text),"operator":"Equal","value":self.expr(&query.value)})}else{Value::Null},
                    "order":if query.page.is_none(){query.order.as_ref().map(order)}else{None},
                    "pagination":query.pagination.as_ref().map(|pagination|json!({"limit":self.expr(&pagination.limit),"offset":self.expr(&pagination.offset)})),
                    "page":page,"includes":includes,"missing_present":query.missing.is_some(),"execution":"not_proved"}));
                if let Some(page) = &query.page {
                    for (index, predicate) in page.predicates.iter().enumerate() {
                        self.walk_expr(
                            &predicate.value,
                            &child(address, ["page_predicate".into(), index.to_string()]),
                            conditions,
                            &[],
                        );
                    }
                    self.walk_expr(
                        &page.after,
                        &child(address, ["page_after".into()]),
                        conditions,
                        &[],
                    );
                    self.walk_expr(
                        &page.limit,
                        &child(address, ["page_limit".into()]),
                        conditions,
                        &[],
                    );
                } else {
                    self.walk_expr(
                        &query.value,
                        &child(address, ["predicate_value".into()]),
                        conditions,
                        &[],
                    );
                }
                if let Some(pagination) = &query.pagination {
                    self.walk_expr(
                        &pagination.limit,
                        &child(address, ["limit".into()]),
                        conditions,
                        &[],
                    );
                    self.walk_expr(
                        &pagination.offset,
                        &child(address, ["offset".into()]),
                        conditions,
                        &[],
                    );
                }
                for (index, include) in query
                    .includes
                    .iter()
                    .enumerate()
                    .filter(|(_, include)| include.cardinality == QueryIncludeCardinality::Many)
                {
                    self.walk_expr(
                        &include.pagination.limit,
                        &child(
                            address,
                            ["include".into(), index.to_string(), "limit".into()],
                        ),
                        conditions,
                        &[],
                    );
                    self.walk_expr(
                        &include.pagination.offset,
                        &child(
                            address,
                            ["include".into(), index.to_string(), "offset".into()],
                        ),
                        conditions,
                        &[],
                    );
                }
                if let Some(missing) = &query.missing {
                    let mut conditions = conditions.to_vec();
                    conditions.push(json!({"trigger":"query_missing"}));
                    self.reject(
                        missing,
                        "query_missing",
                        &child(address, ["missing".into()]),
                        &conditions,
                    );
                }
            }
            Expression::Update(update) => {
                let target = path(&update.target.path);
                self.emit("update_fields",&detail,conditions,json!({"target":target,"predicate":{"field":self.field_ref(&target,&update.field.text),"operator":"Equal","value":self.expr(&update.value)},
                    "changes":self.initializers(Some(&target),&update.changes),
                    "conditional_changes":update.conditional_changes.iter().map(|change|json!({"field":self.field_ref(&target,&change.change.name.text),"value":self.expr(&change.change.value),"condition":{"kind":"supplied","path":path(&change.supplied.path),"status":"symbolic_not_proved"}})).collect::<Vec<_>>(),
                    "patch":update.patch.as_ref().map(|patch|self.patch(patch,&target)),
                    "transition":update.transition.as_ref().map(|transition|json!({"name":transition.text,"checked_lifecycle_path":format!("{target}.lifecycle.transition.{}",transition.text),"impact_category":"lifecycles","sql_writes":"not_inferred"})),"empty_present":update.empty.is_some(),"conflict_count":update.conflicts.len()}));
                self.walk_expr(
                    &update.value,
                    &child(address, ["predicate_value".into()]),
                    conditions,
                    &[],
                );
                self.walk_fields(&update.changes, address, conditions, &[target.clone()]);
                for (index, change) in update.conditional_changes.iter().enumerate() {
                    let mut conditions = conditions.to_vec();
                    conditions.push(json!({"kind":"supplied","path":path(&change.supplied.path)}));
                    self.walk_expr(
                        &change.change.value,
                        &child(
                            address,
                            [
                                "conditional".into(),
                                change.change.name.text.clone(),
                                index.to_string(),
                            ],
                        ),
                        &conditions,
                        &[target.clone(), change.change.name.text.clone()],
                    );
                }
                if let Some(patch) = &update.patch {
                    self.walk_expr(
                        &Expression::Name(patch.clone()),
                        &child(address, ["patch".into()]),
                        conditions,
                        &[],
                    );
                }
                if let Some(empty) = &update.empty {
                    let mut conditions = conditions.to_vec();
                    conditions.push(json!({"trigger":"patch_empty"}));
                    self.reject(
                        empty,
                        "patch_empty",
                        &child(address, ["empty".into()]),
                        &conditions,
                    );
                }
                let mut missing_conditions = conditions.to_vec();
                missing_conditions.push(json!({"trigger":"update_missing"}));
                self.reject(
                    &update.missing,
                    "update_missing",
                    &child(address, ["missing".into()]),
                    &missing_conditions,
                );
                self.conflicts(&update.conflicts, address, conditions);
            }
            Expression::Delete(delete) => {
                let target = path(&delete.target.path);
                self.emit("delete_fields",&detail,conditions,json!({"target":target,"predicate":{"field":self.field_ref(&target,&delete.field.text),"operator":"Equal","value":self.expr(&delete.value)},"conflict_count":delete.conflicts.len()}));
                self.walk_expr(
                    &delete.value,
                    &child(address, ["predicate_value".into()]),
                    conditions,
                    &[],
                );
                let mut missing_conditions = conditions.to_vec();
                missing_conditions.push(json!({"trigger":"delete_missing"}));
                self.reject(
                    &delete.missing,
                    "delete_missing",
                    &child(address, ["missing".into()]),
                    &missing_conditions,
                );
                self.conflicts(&delete.conflicts, address, conditions);
            }
            Expression::Attempt(attempt) => {
                self.emit(
                    "attempt_boundary",
                    &detail,
                    conditions,
                    json!({"trigger":"failure_propagation","value":self.expr(&attempt.value)}),
                );
                self.walk_expr(
                    &attempt.value,
                    &child(address, ["attempt".into()]),
                    conditions,
                    target_path,
                );
            }
            Expression::OutcomeMatch(outcome) => {
                self.emit("outcome_boundary",&detail,conditions,json!({"subject":self.expr(&outcome.subject),"patterns":outcome.arms.iter().map(|arm|self.outcome_pattern(&arm.pattern)).collect::<Vec<_>>()}));
                self.walk_expr(
                    &outcome.subject,
                    &child(address, ["subject".into()]),
                    conditions,
                    &[],
                );
                for (index, arm) in outcome.arms.iter().enumerate() {
                    let arm_address = child(address, ["outcome_arm".into(), index.to_string()]);
                    let mut conditions = conditions.to_vec();
                    conditions.push(json!({"kind":"outcome_arm","pattern":self.outcome_pattern(&arm.pattern),"subject":self.expr(&outcome.subject)}));
                    match &arm.body {
                        OutcomeMatchArmBody::Value(value) => {
                            self.walk_expr(value, &arm_address, &conditions, target_path)
                        }
                        OutcomeMatchArmBody::Reject(reject) => {
                            self.reject(reject, "outcome_reject", &arm_address, &conditions)
                        }
                        OutcomeMatchArmBody::Propagate(_) => self.emit(
                            "failure_boundary",
                            &arm_address,
                            &conditions,
                            json!({"trigger":"outcome_propagate"}),
                        ),
                    }
                }
            }
            Expression::Unary(unary) => self.walk_expr(
                &unary.value,
                &child(address, ["unary_value".into()]),
                conditions,
                target_path,
            ),
            Expression::Binary(binary) => {
                self.walk_expr(
                    &binary.left,
                    &child(address, ["left".into()]),
                    conditions,
                    &[],
                );
                self.walk_expr(
                    &binary.right,
                    &child(address, ["right".into()]),
                    conditions,
                    &[],
                );
            }
            Expression::Grouped(grouped) => self.walk_expr(
                &grouped.value,
                &child(address, ["grouped".into()]),
                conditions,
                target_path,
            ),
        }
    }
}

pub(super) fn derive(project: &AnalyzedProject, facts: &mut BTreeMap<String, Value>) -> Value {
    use jadpo_semantic::{NodeId, NodeKind};
    use std::collections::VecDeque;
    let context = Context::new(project);
    let catalogue = context.catalogue();
    let mut operations = BTreeMap::<String, Value>::new();
    let mut route_surfaces = BTreeMap::new();
    for source in &project.syntax.sources {
        for declaration in &source.file.declarations {
            let (name, owner, body, surface) = match declaration {
                Declaration::Callable(callable) => (
                    callable.name.text.clone(),
                    json!({"kind":format!("{:?}",callable.kind).to_ascii_lowercase(),"name":callable.name.text}),
                    Some(&callable.body),
                    json!({"parameters":callable.parameters.iter().map(|parameter|json!({"name":parameter.name.text,"type":context.declared_type(source,&parameter.parameter_type)})).collect::<Vec<_>>(),
                        "return":context.declared_type(source,&callable.return_type),"failures":callable.failures.iter().map(|failure|failure.text.clone()).collect::<BTreeSet<_>>(),
                        "receiver":callable.receiver.map(|receiver|format!("{:?}",receiver).to_ascii_lowercase()),"mutation_guard":callable.mutation_guard.map(|guard|format!("{:?}",guard).to_ascii_lowercase()),
                        "freshness":callable.freshness.map(|freshness|format!("{:?}",freshness).to_ascii_lowercase()),"consistency":callable.consistency.map(|consistency|format!("{:?}",consistency).to_ascii_lowercase())}),
                ),
                Declaration::Route(route) => {
                    let name = format!("{} {}", super::method_name(route.method), route.path);
                    let surface = json!({"route":name,"input":route.input.as_ref().map(|reference|context.declared_type(source,reference)),"query":route.query.as_ref().map(|reference|context.declared_type(source,reference)),
                        "output":route.output.as_ref().map(|reference|context.declared_type(source,reference)),"success":format!("{:?}",route.success).to_ascii_lowercase(),
                        "path_fields":route.path_fields.iter().map(|field|context.field(source,field)).collect::<Vec<_>>(),"headers":route.headers.iter().map(|header|json!({"name":header.name.text,"wire_name":header.wire_name.text,"optional":header.optional,"type":context.declared_type(source,&header.field_type)})).collect::<Vec<_>>(),
                        "run":route.run.as_ref().map(|invocation|context.expression(source,&Expression::Invocation(invocation.clone()))),
                        "inline_failures":route.inline_action.as_ref().map(|action|action.failures.iter().map(|failure|failure.text.clone()).collect::<BTreeSet<_>>()),"body_kind":if route.inline_action.is_some(){"inline_action"}else{"run_boundary"}});
                    route_surfaces.insert(name.clone(), surface.clone());
                    (
                        name.clone(),
                        json!({"kind":"route","name":name}),
                        route.inline_action.as_ref().map(|action| &action.body),
                        surface,
                    )
                }
                _ => continue,
            };
            let lexical = lexical::Links::derive(declaration, &owner);
            let mut walker = Walker {
                lexical,
                context: &context,
                source,
                owner,
                rows: BTreeMap::new(),
            };
            walker.emit("operation_surface", &[], &[], surface);
            if let Some(body) = body {
                walker.block(body, &["body".into()], &[]);
            }
            if let Declaration::Route(route) = declaration {
                if let Some(invocation) = &route.run {
                    walker.walk_expr(
                        &Expression::Invocation(invocation.clone()),
                        &["run".into()],
                        &[],
                        &[],
                    );
                }
            }
            let restricted_reads=project.policy.operations.iter().filter(|operation|operation.operation==name||operation.operation==format!("route:{}",name.replace(' ',":"))).flat_map(|operation|operation.field_reads.iter().map(|read|json!({"entity":read.entity,"field":read.field,"source":read.source,"subjects":read.subjects.iter().collect::<BTreeSet<_>>()}))).collect::<Vec<_>>();
            operations.insert(name.clone(),json!({"owner":walker.owner,"contracts":walker.rows,"restricted_field_reads":restricted_reads}));
        }
    }
    let graph = &project.semantics;
    let mut adjacency = BTreeMap::<NodeId, Vec<NodeId>>::new();
    for edge in &graph.calls {
        adjacency.entry(edge.caller).or_default().push(edge.callee);
    }
    for children in adjacency.values_mut() {
        children.sort_by_key(|id| {
            (
                &graph.nodes[id.0 as usize].name,
                graph.nodes[id.0 as usize].kind.as_str(),
            )
        });
        children.dedup();
    }
    let mut routes = BTreeMap::new();
    let mut incoming = BTreeMap::<String, BTreeSet<String>>::new();
    for route in graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Route)
    {
        let mut queue = VecDeque::from([(route.id, vec![route.name.clone()])]);
        let mut visited = BTreeSet::new();
        let mut reached = BTreeMap::new();
        while let Some((id, witness)) = queue.pop_front() {
            if !visited.insert(id) {
                continue;
            }
            let node = &graph.nodes[id.0 as usize];
            if matches!(
                node.kind,
                NodeKind::Route | NodeKind::Action | NodeKind::Function | NodeKind::Query
            ) {
                if let Some(contract) = operations.get(&node.name) {
                    incoming
                        .entry(node.name.clone())
                        .or_default()
                        .insert(route.name.clone());
                    reached.insert(
                        node.name.clone(),
                        json!({"operation":node.name,"path":witness,"contract":contract}),
                    );
                }
            }
            for child in adjacency.get(&id).into_iter().flatten() {
                let mut witness = witness.clone();
                witness.push(graph.nodes[child.0 as usize].name.clone());
                queue.push_back((*child, witness));
            }
        }
        routes.insert(route.name.clone(),json!({"route":route.name,"surface":route_surfaces.get(&route.name),"operations":reached,"reachability":"checked_may_call","feasibility":"not_analyzed"}));
    }
    for (name, contract) in &mut operations {
        contract["entry_points"] = json!(incoming.get(name).cloned().unwrap_or_default());
        contract["entry_point_status"] = json!(if incoming.contains_key(name) {
            "checked_may_call_links"
        } else {
            "no_checked_route_link"
        });
    }
    for route in routes.values_mut() {
        for (name, reached) in route["operations"].as_object_mut().unwrap() {
            reached["contract"] = operations[name].clone();
        }
    }
    for (id, contract) in &catalogue {
        facts.insert(format!("field_contract:{id}"), contract.clone());
    }
    for (name, operation) in &operations {
        facts.insert(
            format!("operation_field_contracts:{name}"),
            operation.clone(),
        );
        for (id, contract) in operation["contracts"].as_object().unwrap() {
            facts.insert(format!("source_contract:{id}"), contract.clone());
        }
    }
    for (name, contract) in &routes {
        facts.insert(format!("route_field_contracts:{name}"), contract.clone());
    }
    json!({"status":"compiler_checked_source_contracts","catalogue":catalogue,"operations":operations,"routes":routes,
        "identity":"owner_and_build_local_structural_address_not_insertion_stable","data_origins":"not_resolved","feasibility":"not_analyzed","runtime_conformance":"not_established"})
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_syntax::SourceFile;

    fn analyzed(source: &str) -> AnalyzedProject {
        let project =
            crate::analyze_sources(vec![SourceFile::new("app.jadpo".into(), source.into())])
                .unwrap();
        super::super::checked(&project).expect("fixture must be checked source");
        project
    }

    #[test]
    fn inline_shapes_use_checked_owner_paths_not_parser_counters() {
        let source = "output Envelope { nested: Object { label: Text } }";
        let earlier = format!("output Earlier {{ unrelated: Object {{ ok: Bool }} }}\n{source}");
        let first = analyzed(source);
        let next = analyzed(&earlier);
        let select = |project: &AnalyzedProject| {
            Context::new(project)
                .catalogue()
                .into_values()
                .filter(|row| {
                    row["owner"]["name"] == "Envelope"
                        || row["owner"]["address"]
                            .as_array()
                            .is_some_and(|parts| parts.iter().any(|part| part == "Envelope"))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(select(&first), select(&next));
        assert!(!serde_json::to_string(&select(&first))
            .unwrap()
            .contains("__jadpo_"));
    }

    #[test]
    fn authored_synthetic_looking_names_and_tokens_are_not_rewritten() {
        let project = analyzed(
            "type __jadpo_authored_12 = Text {}\noutput Result { label: __jadpo_authored_12 }",
        );
        let contracts = Context::new(&project).catalogue();
        assert!(contracts
            .values()
            .any(|row| row["contract"]["type"]["symbol"]["name"] == "__jadpo_authored_12"));
    }

    #[test]
    fn missing_conflicting_and_duplicate_observations_preserve_source_and_patch_boundary() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let text =
            std::fs::read_to_string(root.join("tests/compile/pass/40_omission_aware_patch.jadpo"))
                .unwrap();
        let project = analyzed(&text);
        let occurrence = project
            .typing
            .expressions
            .iter()
            .find(|row| row.type_name == "PatchCustomer")
            .unwrap()
            .clone();
        let inspect = |project: &AnalyzedProject| {
            let facts = derive(project, &mut BTreeMap::new());
            let row = facts["operations"]["patch_customer"]["contracts"]
                .as_object()
                .unwrap()
                .values()
                .find(|row| row["kind"] == "update_fields")
                .unwrap();
            row["contract"]["patch"].clone()
        };
        let checked = inspect(&project);
        let mut duplicate = project.clone();
        duplicate.typing.expressions.push(occurrence.clone());
        assert_eq!(checked, inspect(&duplicate));
        let mut missing = project.clone();
        missing
            .typing
            .expressions
            .retain(|row| !(row.source == occurrence.source && row.range == occurrence.range));
        let missing = inspect(&missing);
        assert_eq!(missing["status"], "whole_patch_unresolved");
        assert_eq!(
            missing["reason"],
            "patch_occurrence_type_observation_missing"
        );
        assert!(missing["mappings"].is_null());
        assert_eq!(missing["occurrence"]["tokens"], json!(["input"]));
        let mut conflicting = project.clone();
        let mut other = occurrence;
        other.type_name = "Customer".into();
        conflicting.typing.expressions.push(other);
        let conflict = inspect(&conflicting);
        assert_eq!(
            conflict["reason"],
            "patch_occurrence_type_observations_conflict"
        );
        assert_eq!(
            conflict["occurrence"]["type_evidence"]["status"],
            "conflicting_observations"
        );
        assert!(conflict["mappings"].is_null());
    }

    #[test]
    fn whole_headers_display_uses_exact_route_not_byte_offset() {
        let text="type Result = Object { ok: Bool }\nroute GET /items { auth: none headers: { trace: Text from \"X-Trace\" optional } output: Result action: { var request = headers return Result { ok: true } } }";
        let earlier = format!("output Earlier {{ shape: Object {{ label: Text }} }}\n{text}");
        let first = analyzed(text);
        let next = analyzed(&earlier);
        let extract = |project: &AnalyzedProject| {
            derive(project, &mut BTreeMap::new())["operations"]["GET /items"].clone()
        };
        assert_eq!(extract(&first), extract(&next));
        let encoded = serde_json::to_string(&extract(&first)).unwrap();
        assert!(!encoded.contains("__route_"));
        assert!(!encoded.contains("__jadpo_"));
        let binding = extract(&first)["contracts"]
            .as_object()
            .unwrap()
            .values()
            .find(|row| row["kind"] == "binding")
            .unwrap()
            .clone();
        assert_eq!(
            binding["contract"]["value"]["type_evidence"]["observations"][0][0]["address"],
            json!(["route", "GET /items", "headers"])
        );
    }
}
