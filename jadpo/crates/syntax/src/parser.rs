use crate::ast::*;
use crate::{lex, TextRange, Token, TokenKind};
use jadpo_diagnostics::{Diagnostic, DiagnosticFact, SourceSpan, TextEdit};
use std::collections::BTreeSet;
use std::path::Path;

mod delivery;

pub type ParseResult = ParsedSyntax;

pub fn parse(source_name: &Path, source: &str) -> ParseResult {
    let lexed = lex(source_name, source);
    Parser::new(source_name, source, lexed.tokens, lexed.diagnostics).parse_file()
}

struct Parser<'source> {
    source_name: String,
    source: &'source str,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: Vec<Diagnostic>,
    synthetic_declarations: Vec<Declaration>,
    field_owners: Vec<String>,
    inline_type_counter: usize,
}

struct ParsedRecordBody {
    fields: Vec<FieldDeclaration>,
    inverses: Vec<InverseDeclaration>,
    persistence_constraints: Vec<PersistenceConstraintDeclaration>,
    dossier: Option<EntityDossier>,
    membership: Option<MembershipDeclaration>,
    policy: Option<PolicyDeclaration>,
    end: usize,
}

struct ParsedEntityPersistence {
    declaration: EntityPersistence,
    identities: Vec<Name>,
    uniques: Vec<Name>,
    indexes: Vec<Name>,
    constraints: Vec<PersistenceConstraintDeclaration>,
    references: Vec<PersistenceReferenceDeclaration>,
    inverses: Vec<InverseDeclaration>,
}

struct ParsedFieldOptions {
    constraints: Vec<Constraint>,
    generated: Option<GeneratedFieldRole>,
    role: Option<RoleBinding>,
    immutable: bool,
    policy: Option<PolicyDeclaration>,
}

impl<'source> Parser<'source> {
    fn new(
        source_name: &Path,
        source: &'source str,
        tokens: Vec<Token>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        let mut parser = Self {
            source_name: source_name.to_string_lossy().into_owned(),
            source,
            tokens,
            cursor: 0,
            diagnostics,
            synthetic_declarations: Vec::new(),
            field_owners: Vec::new(),
            inline_type_counter: 0,
        };
        parser.skip_trivia();
        parser
    }

    fn parse_file(mut self) -> ParseResult {
        let module = if self.at(TokenKind::Module) {
            self.parse_module_declaration()
        } else {
            None
        };
        let mut imports = Vec::new();
        while self.at(TokenKind::Import) {
            if let Some(import) = self.parse_import_declaration() {
                imports.push(import);
            }
        }
        if module.is_none() && !imports.is_empty() {
            self.error_at("SYN_IMPORT_REQUIRES_MODULE", imports[0].range);
        }
        let mut declarations = Vec::new();
        let mut exports = Vec::new();
        let mut persistence = Vec::new();
        let mut services = Vec::new();

        while !self.at(TokenKind::Eof) {
            let before = self.cursor;
            let diagnostics_before = self.diagnostics.len();
            if self.at(TokenKind::Persist) {
                if let Some(declaration) = self.parse_persistence_declaration() {
                    persistence.push(declaration);
                }
                if self.cursor == before {
                    self.bump();
                }
                continue;
            }
            let public = if self.at(TokenKind::Public) {
                self.bump();
                true
            } else {
                false
            };
            if self.at_contextual("service") {
                if public {
                    let token = self.current();
                    let diagnostic = Diagnostic::error("SYN_UNEXPECTED_TOKEN")
                        .with_fact(DiagnosticFact::Expected(
                            "a declaration kind that supports public visibility".to_owned(),
                        ))
                        .with_fact(DiagnosticFact::FoundValue("service".to_owned()));
                    self.diagnostic_at(diagnostic, token.range);
                }
                if let Some(service) = self.parse_service_declaration() {
                    services.push(service);
                }
                if self.cursor == before {
                    self.bump();
                }
                continue;
            }
            if self.at(TokenKind::Async) {
                let unsupported = self.bump();
                self.error_at("EFFECT_AUTHORED_ASYNC", unsupported.range);
            }
            if let Some(declaration) = self.parse_declaration() {
                if public {
                    if matches!(declaration, Declaration::Job(_)) {
                        self.error_at("SYN_JOB_EXPORT_INVALID", declaration.range());
                    } else if let Some(name) = declaration_name(&declaration) {
                        exports.push(name.clone());
                    } else {
                        self.error_at("SYN_ROUTE_EXPORT_INVALID", declaration.range());
                    }
                }
                declarations.push(declaration);
            } else {
                if self.diagnostics.len() == diagnostics_before {
                    let token = self.current();
                    self.diagnostic_at(
                        Diagnostic::error("SYN_EXPECTED_DECLARATION").with_fact(
                            DiagnosticFact::FoundValue(diagnostic_token_label(token, self.source)),
                        ),
                        token.range,
                    );
                }
                self.recover_declaration();
            }

            if self.cursor == before {
                self.bump();
            }
        }

        declarations.extend(std::mem::take(&mut self.synthetic_declarations));

        ParseResult {
            source_name: self.source_name,
            source_text: self.source.to_owned(),
            file: SyntaxFile {
                module,
                imports,
                exports,
                persistence,
                declarations,
                services,
                range: TextRange::new(0, self.source.len()),
            },
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn parse_module_declaration(&mut self) -> Option<ModuleDeclaration> {
        let start = self
            .expect(TokenKind::Module, "expected `module`")?
            .range
            .start;
        let path = self.parse_qualified_name()?;
        let end = path.last()?.range.end;
        Some(ModuleDeclaration {
            path,
            range: TextRange::new(start, end),
        })
    }

    fn parse_import_declaration(&mut self) -> Option<ImportDeclaration> {
        let start = self
            .expect(TokenKind::Import, "expected `import`")?
            .range
            .start;
        let module = self.parse_qualified_name()?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before selectively imported names",
        )?;
        let mut names = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            names.push(self.expect_name("expected imported declaration name")?);
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        if names.is_empty() {
            self.error_current("SYN_EMPTY_IMPORT");
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after imported names")?
            .range
            .end;
        Some(ImportDeclaration {
            module,
            names,
            range: TextRange::new(start, end),
        })
    }

    fn parse_declaration(&mut self) -> Option<Declaration> {
        if self.at_contextual("job") {
            return self.parse_job_declaration().map(Declaration::Job);
        }
        if self.at_contextual("locales") {
            return self.parse_locales_declaration().map(Declaration::Locales);
        }
        if self.at_contextual("application") {
            return self
                .parse_application_declaration()
                .map(Declaration::Application);
        }
        if self.at_contextual("principal") {
            return self
                .parse_principal_declaration()
                .map(Declaration::Principal);
        }
        if self.at_contextual("authentication") {
            return self
                .parse_authentication_strategy_declaration()
                .map(Declaration::AuthenticationStrategy);
        }
        match self.current_kind() {
            TokenKind::Config => self.parse_config_declaration().map(Declaration::Config),
            TokenKind::Type => self.parse_type_declaration(),
            TokenKind::Enum => self.parse_enum_declaration().map(Declaration::Enum),
            TokenKind::Entity => self
                .parse_record_declaration(RecordKind::Entity)
                .map(Declaration::Record),
            TokenKind::Value => self
                .parse_record_declaration(RecordKind::Value)
                .map(Declaration::Record),
            TokenKind::Input => self
                .parse_record_declaration(RecordKind::Input)
                .map(Declaration::Record),
            TokenKind::Output => self
                .parse_record_declaration(RecordKind::Output)
                .map(Declaration::Record),
            TokenKind::Failure => self.parse_failure_declaration().map(Declaration::Failure),
            TokenKind::Function => self
                .parse_callable_declaration(CallableKind::Function)
                .map(Declaration::Callable),
            TokenKind::Action => self
                .parse_callable_declaration(CallableKind::Action)
                .map(Declaration::Callable),
            TokenKind::Query => self
                .parse_callable_declaration(CallableKind::Query)
                .map(Declaration::Callable),
            TokenKind::Fixture => self.parse_fixture_declaration().map(Declaration::Fixture),
            TokenKind::Test => self.parse_test_declaration().map(Declaration::Test),
            TokenKind::Route => self.parse_route_declaration().map(Declaration::Route),
            _ => None,
        }
    }

    fn parse_job_declaration(&mut self) -> Option<JobDeclaration> {
        let start = self.bump().range.start;
        let name = self.expect_name("expected job name")?;
        let every_name = self.expect_contextual_name("expected `every` after job name")?;
        if every_name.text != "every" {
            self.error_at("SYN_JOB_EVERY_REQUIRED", every_name.range);
        }
        let Some(duration) = self.parse_config_default() else {
            self.error_current("SYN_JOB_DURATION_REQUIRED");
            return None;
        };
        if duration.kind != ConfigDefaultKind::Duration {
            self.error_at("SYN_JOB_DURATION_REQUIRED", duration.range);
        }
        let every = DurationLiteral {
            text: duration.text,
            range: duration.range,
        };
        self.expect(TokenKind::LeftBrace, "expected `{` after job interval")?;
        let mut seen = BTreeSet::new();
        let mut concurrency = None;
        let mut run = None;
        let mut retry = None;
        let mut delivery = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            if seen.contains("delivery") && self.delivery_declaration_ahead() {
                break;
            }
            let before = self.cursor;
            let item = self.bump();
            let key = item.text(self.source).to_owned();
            if !seen.insert(key.clone()) {
                self.error_at("SYN_JOB_ITEM_DUPLICATE", item.range);
            }
            match key.as_str() {
                "concurrency" | "retry" => {
                    self.expect(TokenKind::Colon, "expected `:` after job clause")?;
                    let value = self.expect_contextual_name("expected closed job clause value")?;
                    match (key.as_str(), value.text.as_str()) {
                        ("concurrency", "singleton") => {
                            concurrency = Some(JobConcurrency::Singleton)
                        }
                        ("retry", "next_schedule") => retry = Some(JobRetry::NextSchedule),
                        _ => self.error_at("SYN_JOB_CLAUSE_VALUE_INVALID", value.range),
                    }
                }
                "run" => {
                    self.expect(TokenKind::Colon, "expected `:` after job run")?;
                    match self.parse_named_expression() {
                        Expression::Invocation(value) => run = Some(value),
                        other => self.error_at("SYN_JOB_RUN_INVOCATION_REQUIRED", other.range()),
                    }
                }
                "delivery" => {
                    delivery = self.parse_job_delivery(item.range.start);
                }
                _ => {
                    self.error_at("SYN_JOB_ITEM_UNKNOWN", item.range);
                    self.recover_job_item();
                }
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = match self.expect(TokenKind::RightBrace, "expected `}` after job clauses") {
            Some(token) => token.range.end,
            None if seen.contains("delivery") && self.delivery_declaration_ahead() => {
                self.previous_significant_end()
            }
            None => return None,
        };
        if concurrency.is_none() || run.is_none() || retry.is_none() {
            self.error_at("SYN_JOB_CLAUSE_REQUIRED", TextRange::new(start, end));
        }
        Some(JobDeclaration {
            name,
            every,
            concurrency,
            run,
            retry,
            delivery,
            range: TextRange::new(start, end),
        })
    }

    fn recover_job_item(&mut self) {
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            if depth == 0
                && (self.at(TokenKind::RightBrace)
                    || self.at_contextual("concurrency")
                    || self.at_contextual("retry")
                    || self.at_contextual("delivery")
                    || self.at(TokenKind::Run))
            {
                break;
            }
            match self.bump().kind {
                TokenKind::LeftBrace | TokenKind::LeftParen | TokenKind::LeftBracket => depth += 1,
                TokenKind::RightBrace | TokenKind::RightParen | TokenKind::RightBracket => {
                    depth = depth.saturating_sub(1)
                }
                _ => {}
            }
        }
    }

    fn parse_service_declaration(&mut self) -> Option<ServiceDeclaration> {
        let start = self.bump().range.start;
        let name = self.expect_contextual_name("expected service name")?;
        let open = self.expect(TokenKind::LeftBrace, "expected `{` after service name")?;
        let body_start = open.range.end;
        let mut depth = 1usize;
        let mut close = None;

        while !self.at(TokenKind::Eof) {
            let token = self.bump();
            match token.kind {
                TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = Some(token);
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(close) = close else {
            let eof = self.current().range;
            let diagnostic = Diagnostic::error("SYN_UNEXPECTED_TOKEN")
                .with_fact(DiagnosticFact::Expected(
                    "`}` to close the service declaration".to_owned(),
                ))
                .with_fact(DiagnosticFact::FoundValue("end of file".to_owned()));
            self.diagnostic_at(diagnostic, TextRange::new(start, eof.start));
            return None;
        };

        Some(ServiceDeclaration {
            name,
            items: parse_service_items(self.source, TextRange::new(body_start, close.range.start)),
            range: TextRange::new(start, close.range.end),
        })
    }

    fn parse_locales_declaration(&mut self) -> Option<LocalesDeclaration> {
        let start = self.bump().range.start;
        self.expect(TokenKind::LeftBrace, "expected `{` after `locales`")?;

        let default_name = self.expect_contextual_name("expected `default`")?;
        if default_name.text != "default" {
            self.error_at("SYN_LOCALES_DEFAULT_REQUIRED", default_name.range);
        }
        self.expect(TokenKind::Colon, "expected `:` after `default`")?;
        let default = self.parse_literal_of(TokenKind::StringLiteral)?;

        let supported_name = self.expect_contextual_name("expected `supported`")?;
        if supported_name.text != "supported" {
            self.error_at("SYN_LOCALES_SUPPORTED_REQUIRED", supported_name.range);
        }
        self.expect(TokenKind::Colon, "expected `:` after `supported`")?;
        self.expect(
            TokenKind::LeftBracket,
            "expected `[` before supported locales",
        )?;
        let mut supported = Vec::new();
        while !self.at(TokenKind::RightBracket) && !self.at(TokenKind::Eof) {
            supported.push(self.parse_literal_of(TokenKind::StringLiteral)?);
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(
            TokenKind::RightBracket,
            "expected `]` after supported locales",
        )?;

        let unsupported_name = self.expect_contextual_name("expected `unsupported`")?;
        if unsupported_name.text != "unsupported" {
            self.error_at("SYN_LOCALES_UNSUPPORTED_REQUIRED", unsupported_name.range);
        }
        self.expect(TokenKind::Colon, "expected `:` after `unsupported`")?;
        let unsupported_value = self.expect_contextual_name("expected locale fallback policy")?;
        let unsupported = match unsupported_value.text.as_str() {
            "fallback_to_default" => LocaleUnsupported::FallbackToDefault,
            "reject" => LocaleUnsupported::Reject,
            _ => {
                self.error_at("SYN_LOCALES_UNSUPPORTED_INVALID", unsupported_value.range);
                LocaleUnsupported::Reject
            }
        };
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after locales")?
            .range
            .end;
        Some(LocalesDeclaration {
            default,
            supported,
            unsupported,
            range: TextRange::new(start, end),
        })
    }

    fn parse_application_declaration(&mut self) -> Option<ApplicationDeclaration> {
        let start = self.bump().range.start;
        let name = self.expect_name("expected application declaration name")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before application settings",
        )?;
        let authentication_name = self.expect_contextual_name("expected `authentication`")?;
        if authentication_name.text != "authentication" {
            self.error_at("SYN_UNEXPECTED_TOKEN", authentication_name.range);
            return None;
        }
        let authentication =
            self.parse_application_authentication(authentication_name.range.start)?;
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after application settings",
            )?
            .range
            .end;
        Some(ApplicationDeclaration {
            name,
            authentication,
            range: TextRange::new(start, end),
        })
    }

    fn parse_application_authentication(
        &mut self,
        start: usize,
    ) -> Option<ApplicationAuthentication> {
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before authentication settings",
        )?;
        let principal_item = self.expect_contextual_name("expected `principal`")?;
        self.expect(TokenKind::Colon, "expected `:` after `principal`")?;
        let principal = self.parse_type_reference()?;
        if principal_item.text != "principal" {
            self.error_at("SYN_UNEXPECTED_TOKEN", principal_item.range);
        }
        let revocation_item = self.expect_contextual_name("expected `revocation`")?;
        if revocation_item.text != "revocation" {
            self.error_at("SYN_UNEXPECTED_TOKEN", revocation_item.range);
        }
        let revocation = self.parse_revocation_declaration(revocation_item.range.start)?;
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after authentication settings",
            )?
            .range
            .end;
        Some(ApplicationAuthentication {
            principal,
            revocation,
            range: TextRange::new(start, end),
        })
    }

    fn parse_revocation_declaration(&mut self, start: usize) -> Option<RevocationDeclaration> {
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before revocation settings",
        )?;
        let mode_item = self.expect_contextual_name("expected `mode`")?;
        self.expect(TokenKind::Colon, "expected `:` after `mode`")?;
        if mode_item.text != "mode" {
            self.error_at("SYN_UNEXPECTED_TOKEN", mode_item.range);
        }
        let mode_name = self.expect_contextual_name("expected `immediate` or `bounded`")?;
        let mode = match mode_name.text.as_str() {
            "immediate" => RevocationMode::Immediate,
            "bounded" => RevocationMode::Bounded,
            _ => {
                self.error_at("SYN_UNEXPECTED_TOKEN", mode_name.range);
                RevocationMode::Bounded
            }
        };
        let maximum_delay = if self.at_contextual("maximum_delay") {
            self.bump();
            self.expect(TokenKind::Colon, "expected `:` after `maximum_delay`")?;
            let value = self.parse_config_default();
            if !value
                .as_ref()
                .is_some_and(|value| value.kind == ConfigDefaultKind::Duration)
            {
                self.error_current("SYN_UNEXPECTED_TOKEN");
            }
            value
        } else {
            None
        };
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after revocation settings",
            )?
            .range
            .end;
        Some(RevocationDeclaration {
            mode,
            maximum_delay,
            range: TextRange::new(start, end),
        })
    }

    fn parse_principal_declaration(&mut self) -> Option<PrincipalDeclaration> {
        let start = self.bump().range.start;
        let name = self.expect_name("expected principal declaration name")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before principal variants",
        )?;
        let mut variants = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            let variant_name = self.expect_contextual_name("expected `user` or `service`")?;
            let kind = match variant_name.text.as_str() {
                "user" => PrincipalVariantKind::User,
                "service" => PrincipalVariantKind::Service,
                _ => {
                    self.error_at("SYN_UNEXPECTED_TOKEN", variant_name.range);
                    self.recover_until(&[TokenKind::RightBrace]);
                    if self.cursor == before {
                        self.bump();
                    }
                    continue;
                }
            };
            let variant_start = variant_name.range.start;
            self.expect(TokenKind::LeftBrace, "expected `{` before principal fields")?;
            let mut fields = Vec::new();
            while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                let field_before = self.cursor;
                if let Some(field) = self.parse_field_declaration(false) {
                    fields.push(field);
                }
                if self.cursor == field_before {
                    self.bump();
                }
            }
            let variant_end = self
                .expect(TokenKind::RightBrace, "expected `}` after principal fields")?
                .range
                .end;
            variants.push(PrincipalVariantDeclaration {
                kind,
                name: variant_name,
                fields,
                range: TextRange::new(variant_start, variant_end),
            });
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after principal variants",
            )?
            .range
            .end;
        Some(PrincipalDeclaration {
            name,
            variants,
            range: TextRange::new(start, end),
        })
    }

    fn parse_authentication_strategy_declaration(
        &mut self,
    ) -> Option<AuthenticationStrategyDeclaration> {
        let start = self.bump().range.start;
        let name = self.expect_name("expected authentication strategy name")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before authentication strategy settings",
        )?;
        let mut transport = None;
        let mut exchange = None;
        let mut validators = None;
        let mut claims = Vec::new();
        let mut resolutions = Vec::new();
        let mut claims_seen = false;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let item = self.expect_contextual_name("expected authentication strategy item")?;
            match item.text.as_str() {
                "transport" if transport.is_none() => {
                    transport = self.parse_authentication_transport(item.range.start);
                }
                "validators" if validators.is_none() => {
                    validators = self.parse_authentication_validators();
                }
                "exchange" if exchange.is_none() => {
                    exchange = self.parse_authentication_exchange(item.range.start);
                }
                "claims" if !claims_seen => {
                    claims_seen = true;
                    claims = self.parse_authentication_mappings("claims")?;
                }
                "resolution" => {
                    resolutions.push(self.parse_authentication_resolution(item.range.start)?);
                }
                _ => {
                    self.error_at("SYN_UNEXPECTED_TOKEN", item.range);
                    self.recover_until(&[TokenKind::RightBrace]);
                }
            }
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after authentication strategy settings",
            )?
            .range
            .end;
        let Some(transport) = transport else {
            self.error_at("SYN_UNEXPECTED_TOKEN", TextRange::new(start, end));
            return None;
        };
        let Some(validators) = validators else {
            self.error_at("SYN_UNEXPECTED_TOKEN", TextRange::new(start, end));
            return None;
        };
        Some(AuthenticationStrategyDeclaration {
            name,
            transport,
            exchange,
            validators,
            claims,
            resolutions,
            range: TextRange::new(start, end),
        })
    }

    fn parse_authentication_exchange(
        &mut self,
        start: usize,
    ) -> Option<AuthenticationExchangeDeclaration> {
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before exchange settings",
        )?;
        let path_name = self.expect_contextual_name("expected `path` in exchange")?;
        if path_name.text != "path" {
            self.error_at("SYN_UNEXPECTED_TOKEN", path_name.range);
            return None;
        }
        self.expect(TokenKind::Colon, "expected `:` after exchange path")?;
        let path = self.parse_literal_of(TokenKind::StringLiteral)?;
        let key_name = self.expect_contextual_name("expected `key` in exchange")?;
        if key_name.text != "key" {
            self.error_at("SYN_UNEXPECTED_TOKEN", key_name.range);
            return None;
        }
        self.expect(TokenKind::Colon, "expected `:` after exchange key")?;
        let key = self.expect_name("expected service key validator name")?;
        let signed_name = self.expect_contextual_name("expected `signed` in exchange")?;
        if signed_name.text != "signed" {
            self.error_at("SYN_UNEXPECTED_TOKEN", signed_name.range);
            return None;
        }
        self.expect(
            TokenKind::Colon,
            "expected `:` after exchange signed validator",
        )?;
        let signed = self.expect_name("expected service signed validator name")?;
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after exchange settings",
            )?
            .range
            .end;
        Some(AuthenticationExchangeDeclaration {
            path,
            key,
            signed,
            range: TextRange::new(start, end),
        })
    }

    fn parse_authentication_mappings(
        &mut self,
        owner: &str,
    ) -> Option<Vec<AuthenticationMappingDeclaration>> {
        self.expect(
            TokenKind::LeftBrace,
            &format!("expected `{{` before authentication {owner}"),
        )?;
        let mut mappings = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let source = self.expect_contextual_name("expected mapping source field")?;
            self.expect(TokenKind::Arrow, "expected `->` in authentication mapping")?;
            let target = self.parse_name_expression_reference()?;
            mappings.push(AuthenticationMappingDeclaration {
                range: TextRange::new(source.range.start, target.range.end),
                source,
                target,
            });
        }
        self.expect(
            TokenKind::RightBrace,
            &format!("expected `}}` after authentication {owner}"),
        )?;
        Some(mappings)
    }

    fn parse_authentication_resolution(
        &mut self,
        start: usize,
    ) -> Option<AuthenticationResolutionDeclaration> {
        let principal = self.expect_contextual_name("expected resolution principal variant")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before authentication resolution",
        )?;
        let authority_item = self.expect_contextual_name("expected `authority`")?;
        self.expect(TokenKind::Colon, "expected `:` after `authority`")?;
        if authority_item.text != "authority" {
            self.error_at("SYN_UNEXPECTED_TOKEN", authority_item.range);
        }
        let authority = self.parse_name_expression_reference()?;
        let active_item = self.expect_contextual_name("expected `active`")?;
        self.expect(TokenKind::Colon, "expected `:` after `active`")?;
        if active_item.text != "active" {
            self.error_at("SYN_UNEXPECTED_TOKEN", active_item.range);
        }
        let active = self.parse_expression();
        let mappings_item = self.expect_contextual_name("expected `mappings`")?;
        if mappings_item.text != "mappings" {
            self.error_at("SYN_UNEXPECTED_TOKEN", mappings_item.range);
        }
        let mappings = self.parse_authentication_mappings("resolution mappings")?;
        let inactive_item = self.expect_contextual_name("expected `inactive`")?;
        self.expect(TokenKind::Colon, "expected `:` after `inactive`")?;
        if inactive_item.text != "inactive" {
            self.error_at("SYN_UNEXPECTED_TOKEN", inactive_item.range);
        }
        let inactive = self.expect_contextual_name("expected inactive-principal failure")?;
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after authentication resolution",
            )?
            .range
            .end;
        Some(AuthenticationResolutionDeclaration {
            principal,
            authority,
            active,
            mappings,
            inactive,
            range: TextRange::new(start, end),
        })
    }

    fn parse_name_expression_reference(&mut self) -> Option<NameExpression> {
        let path = self.parse_qualified_name()?;
        Some(NameExpression {
            range: TextRange::new(path.first()?.range.start, path.last()?.range.end),
            path,
        })
    }

    fn parse_authentication_validators(
        &mut self,
    ) -> Option<Vec<AuthenticationValidatorDeclaration>> {
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before authentication validators",
        )?;
        let mut validators = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let name = self.expect_name("expected authentication validator name")?;
            let start = name.range.start;
            self.expect(
                TokenKind::LeftBrace,
                "expected `{` before authentication validator settings",
            )?;
            let mode_item = self.expect_contextual_name("expected `mode`")?;
            self.expect(TokenKind::Colon, "expected `:` after `mode`")?;
            if mode_item.text != "mode" {
                self.error_at("SYN_UNEXPECTED_TOKEN", mode_item.range);
            }
            let mode = self.expect_contextual_name("expected validation mode")?;
            let principal_item = self.expect_contextual_name("expected `principal`")?;
            self.expect(TokenKind::Colon, "expected `:` after `principal`")?;
            if principal_item.text != "principal" {
                self.error_at("SYN_UNEXPECTED_TOKEN", principal_item.range);
            }
            let principal = self.expect_contextual_name("expected principal variant")?;
            let mut settings = Vec::new();
            let mut credentials = None;
            while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                let name = self.expect_contextual_name("expected validator setting")?;
                if name.text == "credentials" {
                    if credentials.is_some() {
                        self.error_at("SYN_UNEXPECTED_TOKEN", name.range);
                    }
                    credentials = Some(self.parse_authentication_credentials(name.range.start)?);
                    continue;
                }
                self.expect(TokenKind::Colon, "expected `:` after validator setting")?;
                let value = self.parse_expression();
                let range = TextRange::new(name.range.start, value.range().end);
                settings.push(FieldInitialiser { name, value, range });
            }
            let end = self
                .expect(
                    TokenKind::RightBrace,
                    "expected `}` after authentication validator settings",
                )?
                .range
                .end;
            validators.push(AuthenticationValidatorDeclaration {
                name,
                mode,
                principal,
                settings,
                credentials,
                range: TextRange::new(start, end),
            });
        }
        self.expect(
            TokenKind::RightBrace,
            "expected `}` after authentication validators",
        )?;
        Some(validators)
    }

    fn parse_authentication_credentials(
        &mut self,
        start: usize,
    ) -> Option<AuthenticationCredentialBinding> {
        self.expect(TokenKind::LeftBrace, "expected `{` after credentials")?;
        let mut fields = std::collections::BTreeMap::new();
        let mut active = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let key = self.expect_contextual_name("expected credential binding field")?;
            self.expect(
                TokenKind::Colon,
                "expected `:` after credential binding field",
            )?;
            if key.text == "active" {
                if active.is_some() {
                    self.error_at("SYN_UNEXPECTED_TOKEN", key.range);
                }
                active = Some(self.parse_expression());
            } else {
                if !matches!(
                    key.text.as_str(),
                    "identity" | "principal" | "verifier" | "expires" | "revoked"
                ) {
                    self.error_at("SYN_UNEXPECTED_TOKEN", key.range);
                }
                let value = self.parse_name_expression_reference()?;
                if fields.insert(key.text, value).is_some() {
                    self.error_at("SYN_UNEXPECTED_TOKEN", key.range);
                }
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after credentials")?
            .range
            .end;
        if active.is_none()
            || ["identity", "principal", "verifier", "expires", "revoked"]
                .iter()
                .any(|key| !fields.contains_key(*key))
        {
            self.error_at("SYN_UNEXPECTED_TOKEN", TextRange::new(start, end));
            return None;
        }
        Some(AuthenticationCredentialBinding {
            identity: fields.remove("identity")?,
            principal: fields.remove("principal")?,
            verifier: fields.remove("verifier")?,
            active: active?,
            expires: fields.remove("expires")?,
            revoked: fields.remove("revoked")?,
            range: TextRange::new(start, end),
        })
    }

    fn parse_authentication_transport(&mut self, start: usize) -> Option<AuthenticationTransport> {
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before authentication transport",
        )?;
        let kind = self.expect_contextual_name("expected `cookie` or `bearer`")?;
        self.expect(TokenKind::Colon, "expected `:` after credential transport")?;
        let location = match kind.text.as_str() {
            "cookie" => {
                CredentialLocation::Cookie(self.parse_literal_of(TokenKind::StringLiteral)?)
            }
            "bearer" => {
                let token = self.current();
                if !matches!(
                    token.text(self.source),
                    "authorization_header" | "path" | "query"
                ) {
                    self.error_at("SYN_UNEXPECTED_TOKEN", token.range);
                    return None;
                }
                self.bump();
                CredentialLocation::Bearer(Name {
                    text: token.text(self.source).to_owned(),
                    range: token.range,
                })
            }
            _ => {
                self.error_at("SYN_UNEXPECTED_TOKEN", kind.range);
                return None;
            }
        };
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after authentication transport",
            )?
            .range
            .end;
        Some(AuthenticationTransport {
            location,
            range: TextRange::new(start, end),
        })
    }

    fn parse_config_declaration(&mut self) -> Option<ConfigDeclaration> {
        let start = self
            .expect(TokenKind::Config, "expected `config`")?
            .range
            .start;
        let name = self.expect_name("expected configuration declaration name")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before configuration fields",
        )?;
        let mut fields = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(field) = self.parse_config_field() {
                fields.push(field);
            } else {
                self.error_current("CONFIG_EXPECTED_FIELD");
                self.recover_until(&[TokenKind::RightBrace]);
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after configuration fields",
            )?
            .range
            .end;
        Some(ConfigDeclaration {
            name,
            fields,
            range: TextRange::new(start, end),
        })
    }

    fn parse_config_field(&mut self) -> Option<ConfigFieldDeclaration> {
        let start = self.current().range.start;
        let name = self.expect_contextual_name("expected configuration field name")?;
        self.expect(
            TokenKind::Colon,
            "expected `:` after configuration field name",
        )?;
        let field_type = self.parse_type_reference()?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before configuration field options",
        )?;

        let mut binding = None;
        let mut secret = false;
        let mut secret_seen = false;
        let mut default = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let option = self.expect_contextual_name("expected configuration option")?;
            self.expect(TokenKind::Colon, "expected `:` after configuration option")?;
            match option.text.as_str() {
                "binding" => {
                    if binding.is_some() {
                        self.error_at("CONFIG_DUPLICATE_OPTION", option.range);
                    }
                    let value = self.parse_literal_of(TokenKind::StringLiteral)?;
                    binding = Some(value);
                }
                "secret" => {
                    if secret_seen {
                        self.error_at("CONFIG_DUPLICATE_OPTION", option.range);
                    }
                    secret_seen = true;
                    let value = self.parse_literal_of(TokenKind::BooleanLiteral)?;
                    secret = value.text == "true";
                }
                "default" => {
                    if default.is_some() {
                        self.error_at("CONFIG_DUPLICATE_OPTION", option.range);
                    }
                    default = self.parse_config_default();
                    if default.is_none() {
                        self.error_current("CONFIG_DEFAULT_LITERAL_REQUIRED");
                    }
                }
                _ => {
                    self.error_at("CONFIG_UNKNOWN_OPTION", option.range);
                    if !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                        self.bump();
                    }
                }
            }
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after configuration field options",
            )?
            .range
            .end;
        Some(ConfigFieldDeclaration {
            name,
            field_type,
            binding,
            secret,
            default,
            range: TextRange::new(start, end),
        })
    }

    fn parse_config_default(&mut self) -> Option<ConfigDefault> {
        let token = self.current();
        let (kind, mut text, mut range) = match token.kind {
            TokenKind::StringLiteral => (
                ConfigDefaultKind::String,
                token.text(self.source).to_owned(),
                token.range,
            ),
            TokenKind::IntegerLiteral => (
                ConfigDefaultKind::Integer,
                token.text(self.source).to_owned(),
                token.range,
            ),
            TokenKind::DecimalLiteral => (
                ConfigDefaultKind::Decimal,
                token.text(self.source).to_owned(),
                token.range,
            ),
            TokenKind::BooleanLiteral => (
                ConfigDefaultKind::Boolean,
                token.text(self.source).to_owned(),
                token.range,
            ),
            _ => return None,
        };
        self.bump();
        let mut kind = kind;
        if matches!(
            kind,
            ConfigDefaultKind::Integer | ConfigDefaultKind::Decimal
        ) && self.current_kind() == TokenKind::Identifier
            && range.end == self.current().range.start
        {
            let suffix = self.current().text(self.source);
            if matches!(suffix, "ms" | "s" | "m" | "h" | "d") {
                text.push_str(suffix);
                range.end = self.current().range.end;
                kind = ConfigDefaultKind::Duration;
                self.bump();
            }
        }
        Some(ConfigDefault { kind, text, range })
    }

    fn parse_persistence_declaration(&mut self) -> Option<PersistenceDeclaration> {
        let start = self
            .expect(TokenKind::Persist, "expected `persist`")?
            .range
            .start;
        let target = self.expect_name("expected a type name after `persist`")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before persistence settings",
        )?;
        let mut identities = Vec::new();
        let mut uniques = Vec::new();
        let mut indexes = Vec::new();
        let mut constraints = Vec::new();
        let mut references = Vec::new();
        let mut inverses = Vec::new();

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            match self.current_kind() {
                TokenKind::Identity | TokenKind::Index => {
                    let setting = self.bump().kind;
                    self.expect(TokenKind::Colon, "expected `:` after persistence setting")?;
                    let field = self.expect_contextual_name("expected persisted field name")?;
                    if setting == TokenKind::Identity {
                        identities.push(field);
                    } else {
                        indexes.push(field);
                    }
                }
                TokenKind::Unique => {
                    self.bump();
                    self.expect(TokenKind::Colon, "expected `:` after `unique`")?;
                    uniques.push(self.expect_contextual_name("expected unique field name")?);
                }
                TokenKind::Constraint => {
                    constraints.push(self.parse_persistence_constraint()?);
                }
                TokenKind::References => {
                    let reference_start = self.bump().range.start;
                    let field = self.expect_contextual_name("expected referencing field name")?;
                    self.expect(TokenKind::Colon, "expected `:` after referencing field")?;
                    let target = self.parse_type_reference()?;
                    let relationship = if self.at(TokenKind::As) {
                        self.bump();
                        self.expect(TokenKind::Colon, "expected `:` after `as`")?;
                        Some(self.expect_contextual_name("expected relationship name")?)
                    } else {
                        None
                    };
                    self.expect(TokenKind::OnDelete, "expected `on_delete` for reference")?;
                    self.expect(TokenKind::Colon, "expected `:` after `on_delete`")?;
                    let on_delete = self.parse_reference_delete_action()?;
                    let end = self.previous_significant_end();
                    references.push(PersistenceReferenceDeclaration {
                        field,
                        reference: ReferenceDeclaration {
                            target,
                            relationship,
                            on_delete,
                            range: TextRange::new(reference_start, end),
                        },
                        range: TextRange::new(reference_start, end),
                    });
                }
                TokenKind::Inverse => {
                    inverses.push(self.parse_persisted_inverse_declaration()?);
                }
                _ => {
                    self.error_current("SYN_UNEXPECTED_TOKEN");
                    self.recover_until(&[TokenKind::RightBrace]);
                }
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after persistence settings",
            )?
            .range
            .end;
        Some(PersistenceDeclaration {
            target,
            identities,
            uniques,
            indexes,
            constraints,
            references,
            inverses,
            range: TextRange::new(start, end),
        })
    }

    fn parse_persisted_inverse_declaration(&mut self) -> Option<InverseDeclaration> {
        let start = self
            .expect(TokenKind::Inverse, "expected `inverse`")?
            .range
            .start;
        let name = self.expect_contextual_name("expected inverse relationship name")?;
        self.expect(
            TokenKind::Colon,
            "expected `:` after inverse relationship name",
        )?;
        let cardinality = match self.current_kind() {
            TokenKind::Many => {
                self.bump();
                InverseCardinality::Many
            }
            TokenKind::Optional => {
                self.bump();
                InverseCardinality::Optional
            }
            _ => {
                self.error_current("SYN_EXPECTED_INVERSE_CARDINALITY");
                return None;
            }
        };
        let target = self.expect_name("expected child type after inverse cardinality")?;
        self.expect(TokenKind::Via, "expected `via` after inverse target")?;
        self.expect(TokenKind::Colon, "expected `:` after `via`")?;
        let via = self.parse_type_reference()?;
        Some(InverseDeclaration {
            name,
            cardinality,
            target,
            range: TextRange::new(start, via.range.end),
            via,
        })
    }

    fn parse_test_declaration(&mut self) -> Option<TestDeclaration> {
        let start = self.expect(TokenKind::Test, "expected `test`")?.range.start;
        let name = self.parse_literal_of(TokenKind::StringLiteral)?;
        let fixture = if self.at_contextual("using") {
            self.bump();
            Some(self.expect_name("expected fixture name after `using`")?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Some(TestDeclaration {
            range: TextRange::new(start, body.range.end),
            name,
            fixture,
            body,
        })
    }

    fn parse_fixture_declaration(&mut self) -> Option<FixtureDeclaration> {
        let start = self
            .expect(TokenKind::Fixture, "expected `fixture`")?
            .range
            .start;
        let name = self.expect_name("expected fixture name")?;
        self.expect(TokenKind::LeftBrace, "expected `{` after fixture name")?;
        let mut clock = None;
        let mut configuration = None;
        let mut service_fakes = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            if self.at_contextual("clock") {
                let item = self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `clock`")?;
                let fixed =
                    self.expect_contextual_name("expected `fixed` before fixture instant")?;
                if fixed.text != "fixed" {
                    self.error_at("SYN_UNEXPECTED_TOKEN", fixed.range);
                }
                let value = self.parse_expression();
                if clock.replace(value).is_some() {
                    self.error_at("SYN_UNEXPECTED_TOKEN", item.range);
                }
            } else if self.at(TokenKind::Config) {
                let config_token = self.bump();
                self.expect(TokenKind::LeftBrace, "expected `{` after fixture `config`")?;
                let mut values = Vec::new();
                while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                    let item_start = self.current().range.start;
                    let item_name =
                        self.expect_contextual_name("expected configuration field name")?;
                    self.expect(
                        TokenKind::Colon,
                        "expected `:` after configuration field name",
                    )?;
                    let secret = self.at_contextual("secret")
                        && self.next_significant_kind() == Some(TokenKind::LeftParen);
                    let value = if secret {
                        self.bump();
                        self.expect(TokenKind::LeftParen, "expected `(` after `secret`")?;
                        let value = self.parse_expression();
                        self.expect(
                            TokenKind::RightParen,
                            "expected `)` after secret fixture value",
                        )?;
                        value
                    } else {
                        self.parse_expression()
                    };
                    let range = TextRange::new(item_start, value.range().end);
                    values.push(FixtureConfigValue {
                        name: item_name,
                        value,
                        secret,
                        range,
                    });
                }
                self.expect(TokenKind::RightBrace, "expected `}` after fixture config")?;
                if configuration.replace(values).is_some() {
                    self.error_at("SYN_UNEXPECTED_TOKEN", config_token.range);
                }
            } else if self.at_contextual("service") {
                if let Some(fake) = self.parse_fixture_service_fake() {
                    service_fakes.push(fake);
                }
            } else {
                let unexpected = self.bump();
                self.error_at("SYN_UNEXPECTED_TOKEN", unexpected.range);
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after fixture")?
            .range
            .end;
        Some(FixtureDeclaration {
            name,
            clock,
            configuration,
            service_fakes,
            range: TextRange::new(start, end),
        })
    }

    fn parse_fixture_service_fake(&mut self) -> Option<FixtureServiceFake> {
        let start = self.bump().range.start;
        let service = self.expect_name("expected service name in fixture fake")?;
        self.expect(TokenKind::Colon, "expected `:` after fixture service name")?;
        let fake = self.expect_contextual_name("expected `fake` after fixture service name")?;
        if fake.text != "fake" {
            self.error_at("SYN_UNEXPECTED_TOKEN", fake.range);
        }
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before service fake outcomes",
        )?;
        let mut outcomes = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let operation = self.expect_name("expected service operation in fixture fake")?;
            self.expect(
                TokenKind::FatArrow,
                "expected `=>` after fixture service operation",
            )?;
            let value = if self.at_contextual("accept") {
                self.bump();
                FixtureServiceFakeValue::Accepted(self.parse_expression())
            } else {
                FixtureServiceFakeValue::Declared(self.parse_name_expression_reference()?)
            };
            let end = match &value {
                FixtureServiceFakeValue::Accepted(expression) => expression.range().end,
                FixtureServiceFakeValue::Declared(name) => name.range.end,
            };
            outcomes.push(FixtureServiceFakeOutcome {
                range: TextRange::new(operation.range.start, end),
                operation,
                value,
            });
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after service fake outcomes",
            )?
            .range
            .end;
        Some(FixtureServiceFake {
            service,
            outcomes,
            range: TextRange::new(start, end),
        })
    }

    fn parse_enum_declaration(&mut self) -> Option<EnumDeclaration> {
        let start = self.expect(TokenKind::Enum, "expected `enum`")?.range.start;
        let name = self.expect_name("expected an enum name")?;
        self.parse_enum_body(start, name)
    }

    fn parse_enum_body(&mut self, start: usize, name: Name) -> Option<EnumDeclaration> {
        self.expect(TokenKind::LeftBrace, "expected `{` before enum variants")?;
        let mut variants = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(name) = self.expect_name("expected an enum variant") {
                let (fields, end) = if self.at(TokenKind::LeftBrace) {
                    self.parse_field_block()?
                } else {
                    (Vec::new(), name.range.end)
                };
                variants.push(EnumVariantDeclaration {
                    range: TextRange::new(name.range.start, end),
                    name,
                    fields,
                });
            }
            if self.at(TokenKind::Comma) {
                self.bump();
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after enum variants")?
            .range
            .end;
        if variants.is_empty() {
            self.error_at("SYN_ENUM_EMPTY", name.range);
        }
        Some(EnumDeclaration {
            name,
            variants,
            range: TextRange::new(start, end),
        })
    }

    fn parse_type_declaration(&mut self) -> Option<Declaration> {
        let start = self.expect(TokenKind::Type, "expected `type`")?.range.start;
        let name = self.expect_name("expected a type name")?;
        self.expect(TokenKind::Equal, "expected `=` after type name")?;
        if self.current().text(self.source) == "Object" {
            self.bump();
            let body = self.parse_record_body(RecordKind::Value, &name.text)?;
            return Some(Declaration::Record(RecordDeclaration {
                kind: RecordKind::Value,
                name,
                fields: body.fields,
                inverses: body.inverses,
                persistence_constraints: body.persistence_constraints,
                dossier: body.dossier,
                membership: body.membership,
                policy: body.policy,
                range: TextRange::new(start, body.end),
            }));
        }
        if self.current().text(self.source) == "Enum" {
            self.bump();
            return self.parse_enum_body(start, name).map(Declaration::Enum);
        }
        if self.at(TokenKind::LeftBrace) {
            let diagnostic = Diagnostic::error("SYN_TYPE_PARENT_REQUIRED").with_note(
                "use `type User = Text { ... }` for a scalar or `type User = Object { ... }` for fields",
            );
            self.diagnostic_at(diagnostic, self.current().range);
            return None;
        }
        let parent = self.parse_type_reference()?;
        if self.is_object_keyword_typo(&parent) {
            let misspelled = &parent.path[0];
            let mut diagnostic = Diagnostic::error("SEM_UNKNOWN_NAME")
                .with_fact(DiagnosticFact::Name(misspelled.text.clone()))
                .with_fact(DiagnosticFact::Expected("type".to_owned()))
                .with_fact(DiagnosticFact::SuggestedName("Object".to_owned()))
                .with_fact(DiagnosticFact::Usage("type definition".to_owned()))
                .with_edit(TextEdit {
                    source: self.source_name.clone(),
                    start: misspelled.range.start,
                    end: misspelled.range.end,
                    replacement: "Object".to_owned(),
                });
            diagnostic.recommended_next_step.title =
                format!("Replace `{}` with `Object`", misspelled.text);
            self.diagnostic_at(diagnostic, misspelled.range);
            let body = self.parse_record_body(RecordKind::Value, &name.text)?;
            return Some(Declaration::Record(RecordDeclaration {
                kind: RecordKind::Value,
                name,
                fields: body.fields,
                inverses: body.inverses,
                persistence_constraints: body.persistence_constraints,
                dossier: body.dossier,
                membership: body.membership,
                policy: body.policy,
                range: TextRange::new(start, body.end),
            }));
        }
        let (constraints, end) = if self.at(TokenKind::LeftBrace) {
            self.parse_constraint_block()?
        } else {
            (Vec::new(), parent.range.end)
        };

        Some(Declaration::Type(TypeDeclaration {
            name,
            parent,
            constraints,
            range: TextRange::new(start, end),
        }))
    }

    fn parse_record_declaration(&mut self, kind: RecordKind) -> Option<RecordDeclaration> {
        let keyword = match kind {
            RecordKind::Entity => TokenKind::Entity,
            RecordKind::Value => TokenKind::Value,
            RecordKind::Input => TokenKind::Input,
            RecordKind::Output => TokenKind::Output,
        };
        let start = self
            .expect(keyword, "expected record declaration")?
            .range
            .start;
        let name = self.expect_name("expected a record name")?;
        let body = self.parse_record_body(kind, &name.text)?;

        Some(RecordDeclaration {
            kind,
            name,
            fields: body.fields,
            inverses: body.inverses,
            persistence_constraints: body.persistence_constraints,
            dossier: body.dossier,
            membership: body.membership,
            policy: body.policy,
            range: TextRange::new(start, body.end),
        })
    }

    fn parse_record_body(&mut self, kind: RecordKind, owner: &str) -> Option<ParsedRecordBody> {
        self.expect(TokenKind::LeftBrace, "expected `{` before fields")?;
        self.field_owners.push(owner.to_owned());
        let mut fields = Vec::new();
        let mut inverses = Vec::new();
        let mut persistence_constraints = Vec::new();
        let mut identity = None;
        let mut persistence = None;
        let mut representations = Vec::new();
        let mut lifecycle = None;
        let mut membership = None;
        let mut policy = None;
        let owner_name = Name {
            text: owner.to_owned(),
            range: TextRange::new(0, 0),
        };

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if kind == RecordKind::Entity && self.at(TokenKind::Identity) {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `identity`")?;
                identity = self.expect_contextual_name("expected entity identity field");
            } else if kind == RecordKind::Entity && self.at_contextual("persistence") {
                let parsed = self.parse_entity_persistence()?;
                for field_name in &parsed.identities {
                    add_persistence_modifier(
                        &mut fields,
                        field_name,
                        PersistenceModifier::Identity,
                    );
                }
                for field_name in &parsed.uniques {
                    add_persistence_modifier(&mut fields, field_name, PersistenceModifier::Unique);
                }
                for field_name in &parsed.indexes {
                    add_persistence_modifier(&mut fields, field_name, PersistenceModifier::Index);
                }
                for reference in parsed.references {
                    if let Some(field) = fields
                        .iter_mut()
                        .find(|field| field.name.text == reference.field.text)
                    {
                        field.reference = Some(reference.reference);
                    }
                }
                persistence_constraints.extend(parsed.constraints);
                inverses.extend(parsed.inverses);
                persistence = Some(parsed.declaration);
            } else if kind == RecordKind::Entity
                && (self.at_contextual("cache") || self.at_contextual("projection"))
            {
                if let Some(representation) = self.parse_derived_representation() {
                    representations.push(representation);
                }
            } else if self.at_contextual("lifecycle") {
                if kind != RecordKind::Entity {
                    self.error_current("SYN_LIFECYCLE_NON_ENTITY");
                }
                let parsed = self.parse_entity_lifecycle()?;
                let parsed_range = parsed.range;
                if lifecycle.replace(parsed).is_some() {
                    self.error_at("SYN_LIFECYCLE_DUPLICATE", parsed_range);
                }
            } else if kind == RecordKind::Entity
                && matches!(
                    self.current_kind(),
                    TokenKind::Function | TokenKind::Action | TokenKind::Query
                )
            {
                let callable_kind = match self.current_kind() {
                    TokenKind::Function => CallableKind::Function,
                    TokenKind::Action => CallableKind::Action,
                    TokenKind::Query => CallableKind::Query,
                    _ => unreachable!(),
                };
                if let Some(callable) =
                    self.parse_owned_callable_declaration(callable_kind, &owner_name)
                {
                    self.synthetic_declarations
                        .push(Declaration::Callable(callable));
                }
            } else if kind == RecordKind::Entity && self.at_contextual("membership") {
                let parsed = self.parse_membership_declaration()?;
                if membership.replace(parsed).is_some() {
                    self.error_at("POLICY_MEMBERSHIP_DUPLICATE", self.current().range);
                }
            } else if kind == RecordKind::Entity && self.at_contextual("policy") {
                let parsed = self.parse_policy_declaration()?;
                if policy.replace(parsed).is_some() {
                    self.error_at("POLICY_DECLARATION_DUPLICATE", self.current().range);
                }
            } else if self.at(TokenKind::Inverse) {
                if kind != RecordKind::Entity {
                    self.error_current("SYN_INVERSE_NON_ENTITY");
                }
                if let Some(inverse) = self.parse_inverse_declaration() {
                    inverses.push(inverse);
                }
            } else if self.at(TokenKind::Constraint) {
                if kind != RecordKind::Entity {
                    self.error_current("SYN_CONSTRAINT_NON_ENTITY");
                }
                if let Some(constraint) = self.parse_persistence_constraint() {
                    persistence_constraints.push(constraint);
                }
            } else if let Some(field) =
                self.parse_field_declaration(matches!(kind, RecordKind::Input | RecordKind::Value))
            {
                fields.push(field);
            } else {
                self.recover_until(&[TokenKind::RightBrace]);
            }
            if self.cursor == before {
                self.bump();
            }
        }

        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after fields")?
            .range
            .end;
        self.field_owners.pop();
        if let Some(identity_field) = &identity {
            add_persistence_modifier(&mut fields, identity_field, PersistenceModifier::Identity);
        }
        let dossier = match (kind == RecordKind::Entity, identity) {
            (true, Some(identity)) => Some(EntityDossier {
                identity,
                persistence,
                representations,
                lifecycle,
            }),
            _ => None,
        };
        Some(ParsedRecordBody {
            fields,
            inverses,
            persistence_constraints,
            dossier,
            membership,
            policy,
            end,
        })
    }

    fn parse_entity_lifecycle(&mut self) -> Option<EntityLifecycle> {
        let start = self.bump().range.start;
        self.expect(TokenKind::LeftBrace, "expected `{` after `lifecycle`")?;
        let mut initial = None;
        let mut visible = None;
        let mut transitions = Vec::new();
        let mut purge = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if self.at_contextual("initial") {
                let declaration = self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `initial`")?;
                let (fields, _) = self.parse_object_body()?;
                if initial.replace(fields).is_some() {
                    self.error_at("SYN_LIFECYCLE_INITIAL_DUPLICATE", declaration.range);
                }
            } else if self.at_contextual("visible") {
                let declaration = self.bump();
                if self
                    .expect_contextual_name("expected `when` after `visible`")?
                    .text
                    != "when"
                {
                    self.error_at("SYN_LIFECYCLE_VISIBLE_WHEN_REQUIRED", declaration.range);
                }
                let predicate = self.parse_expression();
                if visible.replace(predicate).is_some() {
                    self.error_at("SYN_LIFECYCLE_VISIBLE_DUPLICATE", declaration.range);
                }
            } else if self.at_contextual("transition") {
                let transition_start = self.bump().range.start;
                let name = self.expect_contextual_name("expected transition name")?;
                self.expect(TokenKind::LeftBrace, "expected `{` after transition name")?;
                let mut from = None;
                let mut set = None;
                while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                    let item = self.expect_contextual_name("expected transition setting")?;
                    self.expect(TokenKind::Colon, "expected `:` after transition setting")?;
                    match item.text.as_str() {
                        "from" => {
                            let predicate = self.parse_expression();
                            if from.replace(predicate).is_some() {
                                self.error_at("SYN_LIFECYCLE_TRANSITION_DUPLICATE", item.range);
                            }
                        }
                        "set" => {
                            let (fields, _) = self.parse_object_body()?;
                            if set.replace(fields).is_some() {
                                self.error_at("SYN_LIFECYCLE_TRANSITION_DUPLICATE", item.range);
                            }
                        }
                        _ => {
                            self.error_at("SYN_LIFECYCLE_TRANSITION_SETTING_INVALID", item.range);
                            self.recover_until(&[TokenKind::RightBrace]);
                        }
                    }
                }
                let end = self
                    .expect(TokenKind::RightBrace, "expected `}` after transition")?
                    .range
                    .end;
                let Some(from) = from else {
                    self.error_at("SYN_LIFECYCLE_TRANSITION_FROM_REQUIRED", name.range);
                    if self.cursor == before {
                        self.bump();
                    }
                    continue;
                };
                let Some(set) = set else {
                    self.error_at("SYN_LIFECYCLE_TRANSITION_SET_REQUIRED", name.range);
                    if self.cursor == before {
                        self.bump();
                    }
                    continue;
                };
                if transitions
                    .iter()
                    .any(|transition: &LifecycleTransition| transition.name.text == name.text)
                {
                    self.error_at("SYN_LIFECYCLE_TRANSITION_DUPLICATE", name.range);
                }
                transitions.push(LifecycleTransition {
                    name,
                    from,
                    set,
                    range: TextRange::new(transition_start, end),
                });
            } else if self.at_contextual("purge") {
                let purge_start = self.bump().range.start;
                let after_keyword =
                    self.expect_contextual_name("expected `after` after `purge`")?;
                if after_keyword.text != "after" {
                    self.error_at("SYN_LIFECYCLE_PURGE_AFTER_REQUIRED", after_keyword.range);
                }
                let after = self.parse_expression();
                let from_keyword =
                    self.expect_contextual_name("expected `from` after purge duration")?;
                if from_keyword.text != "from" {
                    self.error_at("SYN_LIFECYCLE_PURGE_FROM_REQUIRED", from_keyword.range);
                }
                let from = self.expect_contextual_name("expected lifecycle timestamp field")?;
                let value = LifecyclePurge {
                    after,
                    from,
                    range: TextRange::new(purge_start, self.previous_significant_end()),
                };
                let value_range = value.range;
                if purge.replace(value).is_some() {
                    self.error_at("SYN_LIFECYCLE_PURGE_DUPLICATE", value_range);
                }
            } else {
                self.error_current("SYN_LIFECYCLE_SETTING_INVALID");
                self.recover_until(&[TokenKind::RightBrace]);
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after lifecycle")?
            .range
            .end;
        Some(EntityLifecycle {
            initial,
            visible,
            transitions,
            purge,
            range: TextRange::new(start, end),
        })
    }

    fn parse_membership_declaration(&mut self) -> Option<MembershipDeclaration> {
        let start = self.bump().range.start;
        self.expect(TokenKind::LeftBrace, "expected `{` after `membership`")?;
        let mut scope = None;
        let mut member = None;
        let mut role = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let item = self.expect_contextual_name("expected membership setting")?;
            self.expect(TokenKind::Colon, "expected `:` after membership setting")?;
            let value = self.expect_contextual_name("expected membership field")?;
            match item.text.as_str() {
                "scope" => scope = Some(value),
                "member" => member = Some(value),
                "role" => role = Some(value),
                _ => self.error_at("SYN_UNEXPECTED_TOKEN", item.range),
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after membership")?
            .range
            .end;
        let missing = Name {
            text: "missing".to_owned(),
            range: TextRange::new(start, start),
        };
        Some(MembershipDeclaration {
            scope: scope.unwrap_or_else(|| {
                self.error_at(
                    "POLICY_MEMBERSHIP_SCOPE_REQUIRED",
                    TextRange::new(start, end),
                );
                missing.clone()
            }),
            member: member.unwrap_or_else(|| {
                self.error_at(
                    "POLICY_MEMBERSHIP_MEMBER_REQUIRED",
                    TextRange::new(start, end),
                );
                missing.clone()
            }),
            role: role.unwrap_or_else(|| {
                self.error_at(
                    "POLICY_MEMBERSHIP_ROLE_REQUIRED",
                    TextRange::new(start, end),
                );
                missing
            }),
            range: TextRange::new(start, end),
        })
    }

    fn parse_policy_declaration(&mut self) -> Option<PolicyDeclaration> {
        let start = self.bump().range.start;
        self.expect(TokenKind::LeftBrace, "expected `{` after `policy`")?;
        let mut scope = None;
        let mut rules = Vec::new();
        let mut operations = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            if self.at_contextual("scope") {
                let setting = self.bump();
                self.expect(TokenKind::Colon, "expected `:` after policy scope")?;
                let value = self.expect_contextual_name("expected policy scope field")?;
                if scope.replace(value).is_some() {
                    self.error_at("POLICY_SCOPE_DUPLICATE", setting.range);
                }
            } else if self.at_contextual("operations") {
                self.bump();
                self.expect(TokenKind::LeftBrace, "expected `{` after policy operations")?;
                while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                    let operation_start = self.current().range.start;
                    let name = self.expect_contextual_name("expected entity operation name")?;
                    self.expect(TokenKind::LeftBrace, "expected `{` after operation name")?;
                    let mut operation_rules = Vec::new();
                    while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                        operation_rules.push(self.parse_policy_rule()?);
                    }
                    let operation_end = self
                        .expect(TokenKind::RightBrace, "expected `}` after operation policy")?
                        .range
                        .end;
                    operations.push(PolicyOperation {
                        name,
                        rules: operation_rules,
                        range: TextRange::new(operation_start, operation_end),
                    });
                }
                self.expect(
                    TokenKind::RightBrace,
                    "expected `}` after policy operations",
                )?;
            } else {
                rules.push(self.parse_policy_rule()?);
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after policy")?
            .range
            .end;
        Some(PolicyDeclaration {
            scope,
            rules,
            operations,
            range: TextRange::new(start, end),
        })
    }

    fn parse_policy_rule(&mut self) -> Option<PolicyRule> {
        let start = self.current().range.start;
        let path = self.parse_qualified_name()?;
        let subject_range = TextRange::new(path.first()?.range.start, path.last()?.range.end);
        let subject = NameExpression {
            path,
            range: subject_range,
        };
        self.expect(TokenKind::Colon, "expected `:` after policy subject")?;
        self.expect(TokenKind::LeftBracket, "expected `[` before policy effects")?;
        let mut effects = Vec::new();
        while !self.at(TokenKind::RightBracket) && !self.at(TokenKind::Eof) {
            let effect = self.expect_contextual_name("expected policy effect")?;
            let parsed = match effect.text.as_str() {
                "create" => Some(PolicyEffect::Create),
                "read" => Some(PolicyEffect::Read),
                "update" => Some(PolicyEffect::Update),
                "delete" => Some(PolicyEffect::Delete),
                "invoke" => Some(PolicyEffect::Invoke),
                _ => None,
            };
            if let Some(parsed) = parsed {
                if effects.contains(&parsed) {
                    self.error_at("POLICY_EFFECT_DUPLICATE", effect.range);
                } else {
                    effects.push(parsed);
                }
            } else {
                self.error_at("POLICY_EFFECT_UNKNOWN", effect.range);
            }
            if self.at(TokenKind::Comma) {
                self.bump();
            } else if !self.at(TokenKind::RightBracket) {
                self.error_current("SYN_UNEXPECTED_TOKEN");
                return None;
            }
        }
        let end = self
            .expect(TokenKind::RightBracket, "expected `]` after policy effects")?
            .range
            .end;
        Some(PolicyRule {
            subject,
            effects,
            range: TextRange::new(start, end),
        })
    }

    fn parse_persistence_constraint(&mut self) -> Option<PersistenceConstraintDeclaration> {
        let start = self
            .expect(TokenKind::Constraint, "expected `constraint`")?
            .range
            .start;
        let name = self.expect_contextual_name("expected constraint name")?;
        self.expect(TokenKind::Colon, "expected `:` after constraint name")?;
        self.expect(TokenKind::Unique, "expected `unique` constraint kind")?;
        self.expect(
            TokenKind::LeftParen,
            "expected `(` before constraint fields",
        )?;
        let mut fields = Vec::new();
        while !self.at(TokenKind::RightParen) && !self.at(TokenKind::Eof) {
            fields.push(self.expect_contextual_name("expected constrained field name")?);
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        let end = self
            .expect(
                TokenKind::RightParen,
                "expected `)` after constraint fields",
            )?
            .range
            .end;
        Some(PersistenceConstraintDeclaration {
            name,
            kind: PersistenceConstraintKind::Unique,
            fields,
            range: TextRange::new(start, end),
        })
    }

    fn parse_entity_persistence(&mut self) -> Option<ParsedEntityPersistence> {
        let start = self.bump().range.start;
        self.expect(TokenKind::LeftBrace, "expected `{` after `persistence`")?;
        let mut store = None;
        let mut role = None;
        let mut identities = Vec::new();
        let mut uniques = Vec::new();
        let mut indexes = Vec::new();
        let mut constraints = Vec::new();
        let mut references = Vec::new();
        let mut inverses = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if self.at_contextual("store") {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `store`")?;
                store = self.expect_contextual_name("expected authority store name");
            } else if self.at_contextual("role") {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `role`")?;
                let value = self.expect_contextual_name("expected persistence role")?;
                if value.text == "authority" {
                    role = Some(PersistenceRole::Authority);
                } else {
                    self.error_at("DATA_PERSISTENCE_ROLE_INVALID", value.range);
                }
            } else if self.at(TokenKind::Identity) {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `identity`")?;
                identities.push(self.expect_contextual_name("expected identity field")?);
            } else if self.at(TokenKind::Unique) {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `unique`")?;
                uniques.push(self.expect_contextual_name("expected unique field")?);
            } else if self.at(TokenKind::Index) {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `index`")?;
                indexes.push(self.expect_contextual_name("expected indexed field")?);
            } else if self.at(TokenKind::Constraint) {
                constraints.push(self.parse_persistence_constraint()?);
            } else if self.at(TokenKind::References) {
                let reference_start = self.bump().range.start;
                let field = self.expect_contextual_name("expected referencing field name")?;
                self.expect(TokenKind::Colon, "expected `:` after referencing field")?;
                let target = self.parse_type_reference()?;
                let relationship = if self.at(TokenKind::As) {
                    self.bump();
                    self.expect(TokenKind::Colon, "expected `:` after `as`")?;
                    Some(self.expect_contextual_name("expected relationship name")?)
                } else {
                    None
                };
                self.expect(TokenKind::OnDelete, "expected `on_delete` for reference")?;
                self.expect(TokenKind::Colon, "expected `:` after `on_delete`")?;
                let on_delete = self.parse_reference_delete_action()?;
                let end = self.previous_significant_end();
                references.push(PersistenceReferenceDeclaration {
                    field,
                    reference: ReferenceDeclaration {
                        target,
                        relationship,
                        on_delete,
                        range: TextRange::new(reference_start, end),
                    },
                    range: TextRange::new(reference_start, end),
                });
            } else if self.at(TokenKind::Inverse) {
                inverses.push(self.parse_persisted_inverse_declaration()?);
            } else {
                self.error_current("SYN_UNEXPECTED_TOKEN");
                self.recover_until(&[TokenKind::RightBrace]);
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after entity persistence",
            )?
            .range
            .end;
        let store = store.unwrap_or_else(|| {
            self.error_at(
                "DATA_PERSISTENCE_STORE_REQUIRED",
                TextRange::new(start, end),
            );
            Name {
                text: "missing".to_owned(),
                range: TextRange::new(start, start),
            }
        });
        let role = role.unwrap_or_else(|| {
            self.error_at("DATA_PERSISTENCE_ROLE_REQUIRED", TextRange::new(start, end));
            PersistenceRole::Authority
        });
        Some(ParsedEntityPersistence {
            declaration: EntityPersistence {
                store,
                role,
                range: TextRange::new(start, end),
            },
            identities,
            uniques,
            indexes,
            constraints,
            references,
            inverses,
        })
    }

    fn parse_derived_representation(&mut self) -> Option<DerivedRepresentation> {
        let token = self.bump();
        let kind = if token.text(self.source) == "cache" {
            DerivedRepresentationKind::Cache
        } else {
            DerivedRepresentationKind::Projection
        };
        let name = self.expect_contextual_name("expected representation name")?;
        self.expect(
            TokenKind::LeftBrace,
            "expected `{` before representation settings",
        )?;
        let mut store = None;
        let mut from = None;
        let mut strategy = None;
        let mut delivery = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let setting = self.expect_contextual_name("expected representation setting")?;
            self.expect(
                TokenKind::Colon,
                "expected `:` after representation setting",
            )?;
            let value = self.expect_contextual_name("expected representation setting value")?;
            match setting.text.as_str() {
                "store" => store = Some(value),
                "from" => from = Some(value),
                "strategy" => strategy = Some(value),
                "delivery" if value.text == "durable" => delivery = Some(DeliveryMode::Durable),
                "delivery" => self.error_at("DATA_DELIVERY_MODE_INVALID", value.range),
                _ => self.error_at("DATA_REPRESENTATION_SETTING_INVALID", setting.range),
            }
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after representation settings",
            )?
            .range
            .end;
        let missing = |parser: &mut Self, setting: &str| {
            parser.error_at(
                "DATA_REPRESENTATION_SETTING_REQUIRED",
                TextRange::new(token.range.start, end),
            );
            Name {
                text: format!("missing_{setting}"),
                range: TextRange::new(token.range.start, token.range.start),
            }
        };
        Some(DerivedRepresentation {
            kind,
            name,
            store: store.unwrap_or_else(|| missing(self, "store")),
            from: from.unwrap_or_else(|| missing(self, "from")),
            strategy,
            delivery: delivery.unwrap_or_else(|| {
                self.error_at(
                    "DATA_REPRESENTATION_SETTING_REQUIRED",
                    TextRange::new(token.range.start, end),
                );
                DeliveryMode::Durable
            }),
            range: TextRange::new(token.range.start, end),
        })
    }

    fn parse_inverse_declaration(&mut self) -> Option<InverseDeclaration> {
        let start = self
            .expect(TokenKind::Inverse, "expected `inverse`")?
            .range
            .start;
        let name = self.expect_contextual_name("expected inverse relationship name")?;
        self.expect(
            TokenKind::Colon,
            "expected `:` after inverse relationship name",
        )?;
        let cardinality = match self.current_kind() {
            TokenKind::Many => {
                self.bump();
                InverseCardinality::Many
            }
            TokenKind::Optional => {
                self.bump();
                InverseCardinality::Optional
            }
            _ => {
                self.error_current("SYN_EXPECTED_INVERSE_CARDINALITY");
                return None;
            }
        };
        let target = self.expect_name("expected child entity after inverse cardinality")?;
        self.expect(TokenKind::Via, "expected `via` after inverse target")?;
        let via = self.parse_type_reference()?;
        let end = via.range.end;
        Some(InverseDeclaration {
            name,
            cardinality,
            target,
            via,
            range: TextRange::new(start, end),
        })
    }

    fn parse_failure_declaration(&mut self) -> Option<FailureDeclaration> {
        let start = self
            .expect(TokenKind::Failure, "expected `failure`")?
            .range
            .start;
        let name = self.expect_name("expected a failure name")?;
        self.expect(TokenKind::LeftBrace, "expected `{` after failure name")?;

        let mut kind = None;
        let mut code = None;
        let mut message = None;
        let mut public_fields = Vec::new();
        let mut internal_fields = Vec::new();

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            match self.current_kind() {
                TokenKind::Kind => {
                    let item = self.bump();
                    self.expect_failure_item_colon(item);
                    kind = self.expect_name("expected a standard failure kind after `kind`");
                }
                TokenKind::Code => {
                    let item = self.bump();
                    self.expect_failure_item_colon(item);
                    code = self.parse_literal_of(TokenKind::StringLiteral);
                }
                TokenKind::Message => {
                    let item = self.bump();
                    self.expect_failure_item_colon(item);
                    message = self.parse_literal_of(TokenKind::StringLiteral);
                }
                TokenKind::Public => {
                    self.bump();
                    if let Some((fields, _)) = self.parse_field_block() {
                        public_fields = fields;
                    }
                }
                TokenKind::Internal => {
                    self.bump();
                    if let Some((fields, _)) = self.parse_field_block() {
                        internal_fields = fields;
                    }
                }
                _ => {
                    self.error_current("SYN_EXPECTED_FAILURE_ITEM");
                    self.recover_until(&[
                        TokenKind::Kind,
                        TokenKind::Code,
                        TokenKind::Message,
                        TokenKind::Public,
                        TokenKind::Internal,
                        TokenKind::RightBrace,
                    ]);
                }
            }
            if self.cursor == before {
                self.bump();
            }
        }

        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after failure declaration",
            )?
            .range
            .end;

        if code.is_none() {
            self.error_at("SYN_FAILURE_CODE_REQUIRED", name.range);
        }
        if kind.is_none() {
            self.error_at("SYN_FAILURE_KIND_REQUIRED", name.range);
        }

        Some(FailureDeclaration {
            name,
            kind: kind.unwrap_or_else(|| Name {
                text: "InternalFault".to_owned(),
                range: TextRange::new(start, start),
            }),
            code,
            message,
            public_fields,
            internal_fields,
            range: TextRange::new(start, end),
        })
    }

    fn parse_callable_declaration(&mut self, kind: CallableKind) -> Option<CallableDeclaration> {
        self.parse_callable_declaration_with_owner(kind, None)
    }

    fn parse_owned_callable_declaration(
        &mut self,
        kind: CallableKind,
        owner: &Name,
    ) -> Option<CallableDeclaration> {
        self.parse_callable_declaration_with_owner(kind, Some(owner))
    }

    fn parse_callable_declaration_with_owner(
        &mut self,
        kind: CallableKind,
        owner: Option<&Name>,
    ) -> Option<CallableDeclaration> {
        let keyword = match kind {
            CallableKind::Function => TokenKind::Function,
            CallableKind::Action => TokenKind::Action,
            CallableKind::Query => TokenKind::Query,
        };
        let start = self
            .expect(keyword, "expected callable declaration")?
            .range
            .start;
        let simple_name = self.expect_name("expected a callable name")?;
        let name = owner.map_or_else(
            || simple_name.clone(),
            |owner| Name {
                text: format!("{}.{}", owner.text, simple_name.text),
                range: simple_name.range,
            },
        );
        let mut parameters = self.parse_parameters()?;
        let mut receiver = None;
        if let Some(owner) = owner {
            if parameters
                .first()
                .is_some_and(|parameter| parameter.name.text == "self")
            {
                let parameter = &mut parameters[0];
                let received = parameter
                    .parameter_type
                    .path
                    .first()
                    .map(|name| name.text.as_str())
                    .unwrap_or_default();
                receiver = match received {
                    "ref" => Some(ReceiverKind::Reference),
                    "value" => Some(ReceiverKind::Value),
                    _ => {
                        self.error_at("DATA_RECEIVER_KIND_INVALID", parameter.parameter_type.range);
                        None
                    }
                };
                parameter.parameter_type.path = match receiver {
                    Some(ReceiverKind::Reference) => vec![
                        owner.clone(),
                        Name {
                            text: "Ref".to_owned(),
                            range: parameter.parameter_type.range,
                        },
                    ],
                    Some(ReceiverKind::Value) | None => vec![owner.clone()],
                };
            }
        }

        let mut consistency = None;
        let mut freshness = None;
        let mut mutation_guard = None;
        loop {
            if self.at_contextual("consistency") {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `consistency`")?;
                let value = self.expect_contextual_name("expected consistency disposition")?;
                consistency = match value.text.as_str() {
                    "atomic" => Some(ConsistencyDisposition::Atomic),
                    "durable_workflow" => Some(ConsistencyDisposition::DurableWorkflow),
                    _ => {
                        self.error_at("DATA_CONSISTENCY_INVALID", value.range);
                        None
                    }
                };
            } else if self.at_contextual("freshness") {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `freshness`")?;
                let value = self.expect_contextual_name("expected query freshness")?;
                freshness = match value.text.as_str() {
                    "authoritative" => Some(QueryFreshness::Authoritative),
                    "read_your_writes" => Some(QueryFreshness::ReadYourWrites),
                    "bounded_staleness" => Some(QueryFreshness::BoundedStaleness),
                    "eventual" => Some(QueryFreshness::Eventual),
                    _ => {
                        self.error_at("DATA_FRESHNESS_INVALID", value.range);
                        None
                    }
                };
            } else if self.at_contextual("guard") {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `guard`")?;
                let value = self.expect_contextual_name("expected mutation guard")?;
                mutation_guard = match value.text.as_str() {
                    "reload" => Some(MutationGuard::Reload),
                    "revision" => Some(MutationGuard::Revision),
                    _ => {
                        self.error_at("DATA_MUTATION_GUARD_INVALID", value.range);
                        None
                    }
                };
            } else {
                break;
            }
        }

        let (mut failures, mut failures_range) = self.parse_callable_failures();
        let return_start = self
            .expect(TokenKind::Arrow, "expected `->` and a return type")?
            .range
            .start;
        let return_type = self.parse_type_reference()?;
        let return_annotation_range = TextRange::new(return_start, return_type.range.end);

        // Keep reading the former position while the repository fixtures are
        // migrated. The formatter and documentation only emit the canonical
        // pre-arrow form.
        if failures.is_empty() {
            (failures, failures_range) = self.parse_callable_failures();
        }

        let (policy, body) = self.parse_callable_block()?;
        let end = body.range.end;
        Some(CallableDeclaration {
            kind,
            name,
            owner: owner.cloned(),
            receiver,
            consistency,
            freshness,
            mutation_guard,
            parameters,
            return_type,
            return_annotation_range,
            failures,
            failures_range,
            policy,
            body,
            range: TextRange::new(start, end),
        })
    }

    fn parse_callable_block(&mut self) -> Option<(Option<PolicyDeclaration>, Block)> {
        let start = self
            .expect(TokenKind::LeftBrace, "expected `{` to start callable body")?
            .range
            .start;
        let policy = if self.at_contextual("policy") {
            Some(self.parse_policy_declaration()?)
        } else {
            None
        };
        let mut statements = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            } else {
                self.error_current("SYN_EXPECTED_STATEMENT");
                self.recover_statement();
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` to close callable body")?
            .range
            .end;
        Some((
            policy,
            Block {
                statements,
                range: TextRange::new(start, end),
            },
        ))
    }

    fn parse_callable_failures(&mut self) -> (Vec<Name>, Option<TextRange>) {
        let mut failures = Vec::new();
        let mut failures_range = None;
        if self.at(TokenKind::Fails) {
            let failures_start = self.bump().range.start;
            loop {
                if let Some(failure) = self.expect_name("expected failure name after `fails`") {
                    failures.push(failure);
                }
                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump();
            }
            failures_range = failures
                .last()
                .map(|failure| TextRange::new(failures_start, failure.range.end));
        }
        (failures, failures_range)
    }

    fn parse_parameters(&mut self) -> Option<Vec<Parameter>> {
        self.expect(TokenKind::LeftParen, "expected `(` before parameters")?;
        let mut parameters = Vec::new();

        while !self.at(TokenKind::RightParen) && !self.at(TokenKind::Eof) {
            let start = self.current().range.start;
            let Some(name) = self.expect_contextual_name("expected parameter name") else {
                self.recover_until(&[TokenKind::Comma, TokenKind::RightParen]);
                if self.at(TokenKind::Comma) {
                    self.bump();
                }
                continue;
            };
            self.expect(TokenKind::Colon, "expected `:` after parameter name")?;
            let parameter_type = self.parse_type_reference()?;
            let end = parameter_type.range.end;
            parameters.push(Parameter {
                name,
                parameter_type,
                range: TextRange::new(start, end),
            });

            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }

        self.expect(TokenKind::RightParen, "expected `)` after parameters")?;
        Some(parameters)
    }

    fn parse_block(&mut self) -> Option<Block> {
        let start = self
            .expect(TokenKind::LeftBrace, "expected `{` to start block")?
            .range
            .start;
        let mut statements = Vec::new();

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            } else {
                self.error_current("SYN_EXPECTED_STATEMENT");
                self.recover_statement();
            }
            if self.cursor == before {
                self.bump();
            }
        }

        let end = self
            .expect(TokenKind::RightBrace, "expected `}` to close block")?
            .range
            .end;
        Some(Block {
            statements,
            range: TextRange::new(start, end),
        })
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match self.current_kind() {
            TokenKind::Var => self.parse_binding_statement().map(Statement::Binding),
            TokenKind::Identifier | TokenKind::Input | TokenKind::Output | TokenKind::Value
                if self.next_significant_kind() == Some(TokenKind::Equal) =>
            {
                self.parse_assignment_statement().map(Statement::Assignment)
            }
            TokenKind::Return => self.parse_return_statement().map(Statement::Return),
            TokenKind::Reject => self.parse_reject_statement().map(Statement::Reject),
            TokenKind::If => self.parse_if_statement().map(Statement::If),
            TokenKind::Match => self.parse_match_statement().map(Statement::Match),
            TokenKind::Assert => self.parse_assert_statement().map(Statement::Assert),
            TokenKind::Advance => self
                .parse_advance_clock_statement()
                .map(Statement::AdvanceClock),
            TokenKind::Throw => self.parse_unsupported_throw().map(Statement::Unsupported),
            _ => None,
        }
    }

    fn parse_assert_statement(&mut self) -> Option<AssertStatement> {
        let start = self
            .expect(TokenKind::Assert, "expected `assert`")?
            .range
            .start;
        let condition = self.parse_expression();
        Some(AssertStatement {
            range: TextRange::new(start, condition.range().end),
            condition,
        })
    }

    fn parse_advance_clock_statement(&mut self) -> Option<AdvanceClockStatement> {
        let start = self
            .expect(TokenKind::Advance, "expected `advance`")?
            .range
            .start;
        let clock = self.expect_contextual_name("expected `clock` after `advance`")?;
        if clock.text != "clock" {
            self.error_at("SYN_UNEXPECTED_TOKEN", clock.range);
        }
        let by = self.expect_contextual_name("expected `by` after `advance clock`")?;
        if by.text != "by" {
            self.error_at("SYN_UNEXPECTED_TOKEN", by.range);
        }
        let duration = self.parse_expression();
        Some(AdvanceClockStatement {
            range: TextRange::new(start, duration.range().end),
            duration,
        })
    }

    fn parse_assignment_statement(&mut self) -> Option<AssignmentStatement> {
        let target = self.expect_contextual_name("expected assignment target")?;
        let start = target.range.start;
        self.expect(TokenKind::Equal, "expected `=` in assignment")?;
        let value = self.parse_expression();
        let end = value.range().end;
        Some(AssignmentStatement {
            target,
            value,
            range: TextRange::new(start, end),
        })
    }

    fn parse_binding_statement(&mut self) -> Option<BindingStatement> {
        let start = self.expect(TokenKind::Var, "expected `var`")?.range.start;
        let mutable = if self.at(TokenKind::Mut) {
            self.bump();
            true
        } else {
            false
        };
        let name = self.expect_contextual_name("expected binding name")?;
        let annotation = if self.at(TokenKind::Colon) {
            self.bump();
            Some(self.parse_type_reference()?)
        } else {
            None
        };
        self.expect(TokenKind::Equal, "expected `=` in binding")?;
        let value = self.parse_expression();
        let end = value.range().end;
        Some(BindingStatement {
            mutable,
            name,
            annotation,
            value,
            range: TextRange::new(start, end),
        })
    }

    fn parse_return_statement(&mut self) -> Option<ReturnStatement> {
        let start = self
            .expect(TokenKind::Return, "expected `return`")?
            .range
            .start;
        let value = self.parse_expression();
        let end = value.range().end;
        Some(ReturnStatement {
            value,
            range: TextRange::new(start, end),
        })
    }

    fn parse_reject_statement(&mut self) -> Option<RejectStatement> {
        let start = self
            .expect(TokenKind::Reject, "expected `reject`")?
            .range
            .start;
        let failure = self.expect_name("expected failure name after `reject`")?;
        let mut values = Vec::new();
        let mut end = failure.range.end;

        if self.at(TokenKind::LeftBrace) {
            let parsed = self.parse_object_body()?;
            values = parsed.0;
            end = parsed.1;
        }

        Some(RejectStatement {
            failure,
            values,
            range: TextRange::new(start, end),
        })
    }

    fn parse_if_statement(&mut self) -> Option<IfStatement> {
        let start = self.expect(TokenKind::If, "expected `if`")?.range.start;
        let condition = self.parse_expression();
        let then_block = self.parse_block()?;
        let else_block = if self.at(TokenKind::Else) {
            self.bump();
            Some(self.parse_block()?)
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map_or(then_block.range.end, |block| block.range.end);

        Some(IfStatement {
            condition,
            then_block,
            else_block,
            range: TextRange::new(start, end),
        })
    }

    fn parse_match_statement(&mut self) -> Option<MatchStatement> {
        let start = self
            .expect(TokenKind::Match, "expected `match`")?
            .range
            .start;
        let subject = self.parse_match_expression();
        self.expect(TokenKind::LeftBrace, "expected `{` before match arms")?;
        let mut arms = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let arm_start = self.current().range.start;
            let pattern = self.parse_match_pattern()?;
            self.expect(TokenKind::FatArrow, "expected `=>` after match pattern")?;
            let body = self.parse_block()?;
            arms.push(MatchArm {
                range: TextRange::new(arm_start, body.range.end),
                pattern,
                body,
            });
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after match arms")?
            .range
            .end;
        Some(MatchStatement {
            subject,
            arms,
            range: TextRange::new(start, end),
        })
    }

    fn parse_match_expression(&mut self) -> Expression {
        self.parse_binary_expression(0, false)
    }

    fn parse_outcome_match_expression(&mut self) -> Expression {
        let start = self.bump().range.start;
        let subject = self.parse_match_expression();
        if self
            .expect(
                TokenKind::LeftBrace,
                "expected `{` before outcome match arms",
            )
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, subject.range().end));
        }
        let mut arms = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let arm_start = self.current().range.start;
            let pattern = if self.at(TokenKind::Success) {
                self.bump();
                if self
                    .expect(TokenKind::LeftParen, "expected `(` after `success`")
                    .is_none()
                {
                    break;
                }
                let Some(binding) =
                    self.expect_contextual_name("expected value binding in `success(...)`")
                else {
                    break;
                };
                if self
                    .expect(
                        TokenKind::RightParen,
                        "expected `)` after success value binding",
                    )
                    .is_none()
                {
                    break;
                }
                OutcomeMatchPattern::Success(binding)
            } else if self.at(TokenKind::Failure) {
                self.bump();
                let Some(failure) = self.expect_name("expected failure name after `failure`")
                else {
                    break;
                };
                OutcomeMatchPattern::Failure(failure)
            } else {
                self.error_current("FAIL_OUTCOME_PATTERN_INVALID");
                self.recover_until(&[
                    TokenKind::Success,
                    TokenKind::Failure,
                    TokenKind::RightBrace,
                ]);
                continue;
            };
            if self
                .expect(TokenKind::FatArrow, "expected `=>` after outcome pattern")
                .is_none()
            {
                break;
            }
            let body = if self.at(TokenKind::Propagate) {
                OutcomeMatchArmBody::Propagate(self.bump().range)
            } else if self.at(TokenKind::Reject) {
                let Some(rejection) = self.parse_reject_statement() else {
                    break;
                };
                OutcomeMatchArmBody::Reject(rejection)
            } else {
                OutcomeMatchArmBody::Value(self.parse_expression())
            };
            arms.push(OutcomeMatchArm {
                range: TextRange::new(arm_start, body.range().end),
                pattern,
                body,
            });
        }
        let end = self
            .expect(
                TokenKind::RightBrace,
                "expected `}` after outcome match arms",
            )
            .map_or(subject.range().end, |token| token.range.end);
        Expression::OutcomeMatch(OutcomeMatchExpression {
            subject: Box::new(subject),
            arms,
            range: TextRange::new(start, end),
        })
    }

    fn parse_match_pattern(&mut self) -> Option<MatchPattern> {
        match self.current_kind() {
            TokenKind::StringLiteral
            | TokenKind::IntegerLiteral
            | TokenKind::DecimalLiteral
            | TokenKind::BooleanLiteral
            | TokenKind::NoneLiteral => self.parse_any_literal().map(MatchPattern::Literal),
            TokenKind::Identifier if self.current().text(self.source) == "_" => {
                let token = self.bump();
                Some(MatchPattern::Wildcard(Name {
                    text: "_".to_owned(),
                    range: token.range,
                }))
            }
            TokenKind::Identifier
            | TokenKind::Config
            | TokenKind::Input
            | TokenKind::Output
            | TokenKind::Value
            | TokenKind::Path => {
                let path = self.parse_qualified_name()?;
                let range = TextRange::new(path.first()?.range.start, path.last()?.range.end);
                let target = NameExpression { path, range };
                if target.path.len() == 1
                    && target.path[0].text == "some"
                    && self.at(TokenKind::LeftParen)
                {
                    self.bump();
                    let binding =
                        self.expect_contextual_name("expected binding in `some(...)` pattern")?;
                    let end = self
                        .expect(TokenKind::RightParen, "expected `)` after optional binding")?
                        .range
                        .end;
                    Some(MatchPattern::OptionalSome(OptionalSomePattern {
                        binding,
                        range: TextRange::new(range.start, end),
                    }))
                } else if self.at(TokenKind::LeftBrace) {
                    self.bump();
                    let mut bindings = Vec::new();
                    while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                        bindings
                            .push(self.expect_contextual_name("expected variant field binding")?);
                        if self.at(TokenKind::Comma) {
                            self.bump();
                        }
                    }
                    let end = self
                        .expect(
                            TokenKind::RightBrace,
                            "expected `}` after variant field bindings",
                        )?
                        .range
                        .end;
                    Some(MatchPattern::Variant(VariantPattern {
                        target,
                        bindings,
                        range: TextRange::new(range.start, end),
                    }))
                } else {
                    Some(MatchPattern::Name(target))
                }
            }
            _ => {
                self.error_current("SYN_MATCH_PATTERN");
                None
            }
        }
    }

    fn parse_unsupported_throw(&mut self) -> Option<UnsupportedStatement> {
        let start = self
            .expect(TokenKind::Throw, "expected `throw`")?
            .range
            .start;
        let expression = self.parse_expression();
        let range = TextRange::new(start, expression.range().end);
        self.error_at("SYN_UNSUPPORTED_THROW", range);
        Some(UnsupportedStatement {
            keyword: "throw".to_owned(),
            range,
        })
    }

    fn parse_expression(&mut self) -> Expression {
        self.parse_binary_expression(0, true)
    }

    fn parse_binary_expression(
        &mut self,
        minimum_precedence: u8,
        allow_construction: bool,
    ) -> Expression {
        let mut expression = self.parse_prefix_expression(allow_construction);
        while let Some((operator, precedence)) = self.current_binary_operator() {
            if precedence < minimum_precedence {
                break;
            }
            self.bump();
            let right = self.parse_binary_expression(precedence + 1, allow_construction);
            let range = TextRange::new(expression.range().start, right.range().end);
            expression = Expression::Binary(BinaryExpression {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
                range,
            });
        }

        expression
    }

    fn parse_prefix_expression(&mut self, allow_construction: bool) -> Expression {
        if self.at(TokenKind::Await) {
            let unsupported = self.bump();
            self.error_at("EFFECT_AUTHORED_AWAIT", unsupported.range);
            return self.parse_prefix_expression(allow_construction);
        }
        if self.at(TokenKind::Attempt) {
            let start = self.bump().range.start;
            let value = self.parse_prefix_expression(allow_construction);
            return Expression::Attempt(AttemptExpression {
                range: TextRange::new(start, value.range().end),
                value: Box::new(value),
            });
        }
        if self.at(TokenKind::Call) {
            let start = self.bump().range.start;
            let expression = self.parse_named_expression_with_construction(false);
            let Expression::Invocation(invocation) = expression else {
                self.error_at("SYN_EXPECTED_INVOCATION", expression.range());
                return Expression::Missing(TextRange::new(start, expression.range().end));
            };
            let end = invocation.range.end;
            return Expression::TestCall(TestCallExpression {
                invocation,
                range: TextRange::new(start, end),
            });
        }
        if self.at(TokenKind::Not) || self.at(TokenKind::Minus) {
            let token = self.bump();
            let operator = if token.kind == TokenKind::Not {
                UnaryOperator::Not
            } else {
                UnaryOperator::Negate
            };
            let value = self.parse_prefix_expression(allow_construction);
            return Expression::Unary(UnaryExpression {
                range: TextRange::new(token.range.start, value.range().end),
                operator,
                value: Box::new(value),
            });
        }
        match self.current_kind() {
            TokenKind::Match => self.parse_outcome_match_expression(),
            TokenKind::Create => self.parse_create_expression(),
            TokenKind::Query
                if matches!(
                    self.next_significant_kind(),
                    Some(TokenKind::Optional | TokenKind::Required | TokenKind::Many)
                ) || self.next_significant_is_contextual("page") =>
            {
                self.parse_query_expression()
            }
            TokenKind::Update => self.parse_update_expression(),
            TokenKind::Delete => self.parse_delete_expression(),
            TokenKind::StringLiteral
            | TokenKind::IntegerLiteral
            | TokenKind::DecimalLiteral
            | TokenKind::BooleanLiteral
            | TokenKind::NoneLiteral => self
                .parse_any_literal()
                .map(Expression::Literal)
                .unwrap_or_else(|| Expression::Missing(self.current().range)),
            TokenKind::Identifier
            | TokenKind::Config
            | TokenKind::Input
            | TokenKind::Output
            | TokenKind::Value
            | TokenKind::Path
            | TokenKind::Query
            | TokenKind::Headers => {
                self.parse_named_expression_with_construction(allow_construction)
            }
            TokenKind::LeftParen => {
                let start = self.bump().range.start;
                let value = self.parse_expression();
                let end = self
                    .expect(TokenKind::RightParen, "expected `)` after expression")
                    .map_or(value.range().end, |token| token.range.end);
                Expression::Grouped(GroupedExpression {
                    value: Box::new(value),
                    range: TextRange::new(start, end),
                })
            }
            TokenKind::LeftBrace => {
                let start = self.current().range.start;
                let Some((fields, end)) = self.parse_object_body() else {
                    let end = self.previous_significant_end().max(start);
                    return Expression::Missing(TextRange::new(start, end));
                };
                Expression::Object(ObjectExpression {
                    fields,
                    range: TextRange::new(start, end),
                })
            }
            _ => {
                let range = self.current().range;
                self.error_current("SYN_EXPECTED_EXPRESSION");
                if !self.at(TokenKind::Eof) {
                    self.bump();
                }
                Expression::Missing(range)
            }
        }
    }

    fn current_binary_operator(&self) -> Option<(BinaryOperator, u8)> {
        Some(match self.current_kind() {
            TokenKind::Or => (BinaryOperator::Or, 1),
            TokenKind::And => (BinaryOperator::And, 2),
            TokenKind::EqualEqual => (BinaryOperator::Equal, 3),
            TokenKind::BangEqual => (BinaryOperator::NotEqual, 3),
            TokenKind::LeftAngle => (BinaryOperator::Less, 4),
            TokenKind::LessEqual => (BinaryOperator::LessEqual, 4),
            TokenKind::RightAngle => (BinaryOperator::Greater, 4),
            TokenKind::GreaterEqual => (BinaryOperator::GreaterEqual, 4),
            TokenKind::Plus => (BinaryOperator::Add, 5),
            TokenKind::Minus => (BinaryOperator::Subtract, 5),
            TokenKind::Star => (BinaryOperator::Multiply, 6),
            TokenKind::Slash => (BinaryOperator::Divide, 6),
            TokenKind::Percent => (BinaryOperator::Remainder, 6),
            _ => return None,
        })
    }

    fn parse_create_expression(&mut self) -> Expression {
        let start = self.bump().range.start;
        let Some(path) = self.parse_qualified_name() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let target_range = TextRange::new(
            path.first().map_or(start, |name| name.range.start),
            path.last().map_or(start, |name| name.range.end),
        );
        let target = NameExpression {
            path,
            range: target_range,
        };
        let Some((fields, fields_end)) = self.parse_object_body() else {
            let end = self.previous_significant_end().max(start);
            return Expression::Missing(TextRange::new(start, end));
        };
        let conflicts = self.parse_conflict_bindings();
        let end = conflicts
            .last()
            .map_or(fields_end, |binding| binding.range.end);
        Expression::Create(CreateExpression {
            target,
            fields,
            conflicts,
            range: TextRange::new(start, end),
        })
    }

    fn parse_query_expression(&mut self) -> Expression {
        let start = self.bump().range.start;
        if self.at_contextual("page") {
            self.bump();
            return self.parse_query_page_expression(start);
        }
        let cardinality = match self.current_kind() {
            TokenKind::Optional => {
                self.bump();
                QueryCardinality::Optional
            }
            TokenKind::Required => {
                self.bump();
                QueryCardinality::Required
            }
            TokenKind::Many => {
                self.bump();
                QueryCardinality::Many
            }
            _ => {
                let range = self.current().range;
                let diagnostic = self.expected_diagnostic(
                    "SYN_UNEXPECTED_TOKEN",
                    "one of `optional`, `required`, or `many` after `query`",
                );
                self.diagnostic_at(diagnostic, range);
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
        };
        let Some(path) = self.parse_qualified_name() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let target_range = TextRange::new(
            path.first().map_or(start, |name| name.range.start),
            path.last().map_or(start, |name| name.range.end),
        );
        let target = NameExpression {
            path,
            range: target_range,
        };
        if self
            .expect(TokenKind::LeftBrace, "expected `{` after query target")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        if self
            .expect(TokenKind::Where, "expected `where` in query")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        if self
            .expect(TokenKind::Colon, "expected `:` after `where`")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let Some(field) = self.expect_contextual_name("expected entity field after `where`") else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        if self
            .expect(TokenKind::EqualEqual, "expected `==` in query predicate")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let value = Box::new(self.parse_expression());
        let order = if cardinality == QueryCardinality::Many {
            let Some(order_token) = self.expect(
                TokenKind::OrderBy,
                "expected `order_by` in many-result query",
            ) else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let order_start = order_token.range.start;
            if self
                .expect(TokenKind::Colon, "expected `:` after `order_by`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some(field) = self.expect_contextual_name("expected entity field after `order_by`")
            else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let direction = match self.current_kind() {
                TokenKind::Asc => QueryOrderDirection::Ascending,
                TokenKind::Desc => QueryOrderDirection::Descending,
                _ => {
                    self.error_current("SYN_EXPECTED_ORDER_DIRECTION");
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
            };
            let order_end = self.bump().range.end;
            Some(QueryOrder {
                field,
                direction,
                range: TextRange::new(order_start, order_end),
            })
        } else {
            None
        };
        let pagination = if cardinality == QueryCardinality::Many && self.at(TokenKind::Limit) {
            let Some(pagination) = self.parse_query_pagination() else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            Some(pagination)
        } else {
            None
        };
        let mut includes = Vec::new();
        while self.at(TokenKind::Include) {
            let include_start = self.bump().range.start;
            if self
                .expect(TokenKind::Colon, "expected `:` after `include`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some(relationship) =
                self.expect_contextual_name("expected inverse relationship after `include`")
            else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let nested_relationship = if self.at(TokenKind::Dot) {
                self.bump();
                let Some(nested) =
                    self.expect_contextual_name("expected nested relationship name after `.`")
                else {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                Some(nested)
            } else {
                None
            };
            let include_cardinality = match self.current_kind() {
                TokenKind::Required => {
                    self.bump();
                    QueryIncludeCardinality::Required
                }
                TokenKind::Optional => {
                    self.bump();
                    QueryIncludeCardinality::Optional
                }
                _ => QueryIncludeCardinality::Many,
            };
            if self
                .expect(
                    TokenKind::Into,
                    "expected `into` and a named output shape after included relationship",
                )
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            if self
                .expect(TokenKind::Colon, "expected `:` after `into`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some(result) = self.parse_type_reference() else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let (order, pagination, include_end) = if include_cardinality
                == QueryIncludeCardinality::Many
            {
                if self
                    .expect(
                        TokenKind::OrderBy,
                        "expected `order_by` for included to-many relationship",
                    )
                    .is_none()
                {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
                if self
                    .expect(TokenKind::Colon, "expected `:` after `order_by`")
                    .is_none()
                {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
                let Some(order_field) =
                    self.expect_contextual_name("expected child field after `order_by`")
                else {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                let direction = match self.current_kind() {
                    TokenKind::Asc => QueryOrderDirection::Ascending,
                    TokenKind::Desc => QueryOrderDirection::Descending,
                    _ => {
                        self.error_current("SYN_EXPECTED_ORDER_DIRECTION");
                        return Expression::Missing(TextRange::new(
                            start,
                            self.current().range.end,
                        ));
                    }
                };
                let order_end = self.bump().range.end;
                let Some(pagination) = self.parse_query_pagination() else {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                (
                    QueryOrder {
                        field: order_field,
                        direction,
                        range: TextRange::new(include_start, order_end),
                    },
                    pagination.clone(),
                    pagination.range.end,
                )
            } else {
                let synthetic = TextRange::new(result.range.end, result.range.end);
                (
                    QueryOrder {
                        field: relationship.clone(),
                        direction: QueryOrderDirection::Ascending,
                        range: synthetic,
                    },
                    QueryPagination {
                        limit: Box::new(Expression::Literal(Literal {
                            kind: LiteralKind::Integer,
                            text: "1".to_owned(),
                            range: synthetic,
                        })),
                        offset: Box::new(Expression::Literal(Literal {
                            kind: LiteralKind::Integer,
                            text: "0".to_owned(),
                            range: synthetic,
                        })),
                        range: synthetic,
                    },
                    result.range.end,
                )
            };
            includes.push(QueryInclude {
                relationship,
                nested_relationship,
                cardinality: include_cardinality,
                result,
                order,
                pagination,
                range: TextRange::new(include_start, include_end),
            });
        }
        let missing = if cardinality == QueryCardinality::Required {
            if self
                .expect(TokenKind::Missing, "expected `missing` in required query")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            if self
                .expect(TokenKind::Colon, "expected `:` after `missing`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            self.parse_failure_binding()
        } else {
            None
        };
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after query predicate")
            .map_or(value.range().end, |token| token.range.end);
        Expression::Query(QueryExpression {
            cardinality,
            target,
            field,
            value,
            order,
            pagination,
            page: None,
            includes,
            missing,
            range: TextRange::new(start, end),
        })
    }

    fn parse_query_page_expression(&mut self, start: usize) -> Expression {
        let Some(path) = self.parse_qualified_name() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let target_end = path.last().map_or(start, |name| name.range.end);
        let target = NameExpression {
            range: TextRange::new(
                path.first().map_or(start, |name| name.range.start),
                target_end,
            ),
            path,
        };
        if self
            .expect(
                TokenKind::Arrow,
                "expected `->` and a page result after query target",
            )
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let Some(result) = self.parse_type_reference() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        if self
            .expect(TokenKind::LeftBrace, "expected `{` after page result")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        if self
            .expect(TokenKind::Where, "expected `where` in page query")
            .is_none()
            || self
                .expect(TokenKind::Colon, "expected `:` after `where`")
                .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let Some(first) = self.parse_query_page_predicate() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let mut predicates = vec![first];
        while self.at(TokenKind::And) {
            self.bump();
            if self
                .expect(TokenKind::Colon, "expected `:` after page predicate `and`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some(predicate) = self.parse_query_page_predicate() else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            predicates.push(predicate);
        }

        if self
            .expect(TokenKind::OrderBy, "expected `order_by` in page query")
            .is_none()
            || self
                .expect(TokenKind::Colon, "expected `:` after `order_by`")
                .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let mut order = Vec::new();
        loop {
            let order_start = self.current().range.start;
            let Some(field) = self.expect_contextual_name("expected entity field after `order_by`")
            else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let direction = match self.current_kind() {
                TokenKind::Asc => QueryOrderDirection::Ascending,
                TokenKind::Desc => QueryOrderDirection::Descending,
                _ => {
                    self.error_current("SYN_EXPECTED_ORDER_DIRECTION");
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
            };
            let order_end = self.bump().range.end;
            order.push(QueryOrder {
                field,
                direction,
                range: TextRange::new(order_start, order_end),
            });
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump();
        }

        if !self.at_contextual("after") {
            self.error_current("SYN_QUERY_PAGE_AFTER_REQUIRED");
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        self.bump();
        if self
            .expect(TokenKind::Colon, "expected `:` after `after`")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let after_optional = if self.at(TokenKind::Optional) {
            self.bump();
            true
        } else {
            false
        };
        let after = Box::new(self.parse_expression());

        if self
            .expect(TokenKind::Limit, "expected `limit` in page query")
            .is_none()
            || self
                .expect(TokenKind::Colon, "expected `:` after `limit`")
                .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let limit = Box::new(self.parse_expression());
        if !self.at_contextual("project") {
            self.error_current("SYN_QUERY_PAGE_PROJECT_REQUIRED");
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        self.bump();
        if self
            .expect(TokenKind::Colon, "expected `:` after `project`")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let Some(projection) = self.parse_type_reference() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        if !self.at_contextual("cursor") {
            self.error_current("SYN_QUERY_PAGE_CURSOR_REQUIRED");
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        self.bump();
        if self
            .expect(TokenKind::Colon, "expected `:` after `cursor`")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let Some(cursor_path) = self.parse_qualified_name() else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let cursor_start = cursor_path.first().map_or(start, |name| name.range.start);
        let cursor_end = cursor_path.last().map_or(start, |name| name.range.end);
        let cursor = TypeReference {
            path: cursor_path,
            arguments: Vec::new(),
            nullable: false,
            range: TextRange::new(cursor_start, cursor_end),
        };
        if self
            .expect(
                TokenKind::LeftParen,
                "expected cursor fields in parentheses",
            )
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let mut cursor_fields = Vec::new();
        loop {
            let Some(field) = self.expect_contextual_name("expected cursor field") else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            cursor_fields.push(field);
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump();
        }
        if self
            .expect(TokenKind::RightParen, "expected `)` after cursor fields")
            .is_none()
        {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after page query")
            .map_or(self.previous_significant_end(), |token| token.range.end);
        let page = QueryPage {
            result: result.clone(),
            predicates: predicates.clone(),
            order: order.clone(),
            after,
            after_optional,
            limit: limit.clone(),
            projection,
            cursor,
            cursor_fields,
            range: TextRange::new(start, end),
        };
        let first = &predicates[0];
        Expression::Query(QueryExpression {
            cardinality: QueryCardinality::Many,
            target,
            field: first.field.clone(),
            value: first.value.clone(),
            order: order.first().cloned(),
            pagination: None,
            page: Some(page),
            includes: Vec::new(),
            missing: None,
            range: TextRange::new(start, end),
        })
    }

    fn parse_query_page_predicate(&mut self) -> Option<QueryPagePredicate> {
        let start = self.current().range.start;
        let field = self.expect_contextual_name("expected entity field in page predicate")?;
        let (operator, value) = if self.at(TokenKind::EqualEqual) {
            self.bump();
            (
                QueryPagePredicateOperator::Equal,
                self.parse_binary_expression(3, true),
            )
        } else if self.at(TokenKind::GreaterEqual) {
            self.bump();
            (
                QueryPagePredicateOperator::RequiredGreaterEqual,
                self.parse_binary_expression(3, true),
            )
        } else if self.at(TokenKind::LeftAngle) {
            self.bump();
            (
                QueryPagePredicateOperator::RequiredLess,
                self.parse_binary_expression(3, true),
            )
        } else if self.at(TokenKind::LessEqual) {
            self.bump();
            self.expect(
                TokenKind::Optional,
                "expected `optional` after `<=` in page predicate",
            )?;
            (
                QueryPagePredicateOperator::OptionalLessEqual,
                self.parse_binary_expression(3, true),
            )
        } else if self.at_contextual("matches") {
            self.bump();
            self.expect(TokenKind::Optional, "expected `optional` after `matches`")?;
            (
                QueryPagePredicateOperator::OptionalEqual,
                self.parse_binary_expression(3, true),
            )
        } else {
            self.error_current("SYN_QUERY_PAGE_PREDICATE_OPERATOR");
            return None;
        };
        Some(QueryPagePredicate {
            field,
            operator,
            range: TextRange::new(start, value.range().end),
            value: Box::new(value),
        })
    }

    fn parse_query_pagination(&mut self) -> Option<QueryPagination> {
        let start = self
            .expect(
                TokenKind::Limit,
                "expected explicit `limit` for a to-many load",
            )?
            .range
            .start;
        self.expect(TokenKind::Colon, "expected `:` after `limit`")?;
        let limit = Box::new(self.parse_expression());
        self.expect(TokenKind::Offset, "expected `offset` after query limit")?;
        self.expect(TokenKind::Colon, "expected `:` after `offset`")?;
        let offset = Box::new(self.parse_expression());
        let end = offset.range().end;
        Some(QueryPagination {
            limit,
            offset,
            range: TextRange::new(start, end),
        })
    }

    fn parse_failure_binding(&mut self) -> Option<RejectStatement> {
        let failure = self.expect_name("expected failure name after `missing:`")?;
        let start = failure.range.start;
        let mut values = Vec::new();
        let mut end = failure.range.end;

        if self.at(TokenKind::LeftBrace) {
            let parsed = self.parse_object_body()?;
            values = parsed.0;
            end = parsed.1;
        }

        Some(RejectStatement {
            failure,
            values,
            range: TextRange::new(start, end),
        })
    }

    fn parse_update_expression(&mut self) -> Expression {
        let start = self.bump().range.start;
        let Some(target) = self.parse_required_mutation_target(start, "update") else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let Some((field, value)) = self.parse_mutation_predicate(start, "update") else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let (transition, changes, conditional_changes, patch, empty) = if self
            .at_contextual("transition")
        {
            self.bump();
            if self
                .expect(TokenKind::Colon, "expected `:` after `transition`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some(transition) =
                self.expect_contextual_name("expected lifecycle transition name")
            else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let (changes, conditional_changes, patch, empty) = if self.at(TokenKind::Set) {
                self.bump();
                if self
                    .expect(TokenKind::Colon, "expected `:` after `set`")
                    .is_none()
                {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
                let Some((changes, _)) = self.parse_object_body() else {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                (changes, Vec::new(), None, None)
            } else if self.at(TokenKind::Patch) {
                self.bump();
                if self
                    .expect(TokenKind::Colon, "expected `:` after `patch`")
                    .is_none()
                {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
                let Some(path) = self.parse_qualified_name() else {
                    self.error_current("SYN_EXPECTED_PATCH_INPUT");
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                let patch = NameExpression {
                    range: TextRange::new(
                        path.first().map_or(start, |name| name.range.start),
                        path.last().map_or(start, |name| name.range.end),
                    ),
                    path,
                };
                let Some(empty) = self.parse_named_failure_binding("empty", TokenKind::Empty)
                else {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                let (changes, conditional_changes) = if self.at(TokenKind::Set) {
                    self.bump();
                    if self
                        .expect(TokenKind::Colon, "expected `:` after `set`")
                        .is_none()
                    {
                        return Expression::Missing(TextRange::new(
                            start,
                            self.current().range.end,
                        ));
                    }
                    let Some(changes) = self.parse_patch_set_body() else {
                        return Expression::Missing(TextRange::new(
                            start,
                            self.current().range.end,
                        ));
                    };
                    changes
                } else {
                    (Vec::new(), Vec::new())
                };
                (changes, conditional_changes, Some(patch), Some(empty))
            } else {
                (Vec::new(), Vec::new(), None, None)
            };
            (Some(transition), changes, conditional_changes, patch, empty)
        } else if self.at(TokenKind::Set) {
            self.bump();
            if self
                .expect(TokenKind::Colon, "expected `:` after `set`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some((changes, _)) = self.parse_object_body() else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            (None, changes, Vec::new(), None, None)
        } else if self.at(TokenKind::Patch) {
            self.bump();
            if self
                .expect(TokenKind::Colon, "expected `:` after `patch`")
                .is_none()
            {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            }
            let Some(path) = self.parse_qualified_name() else {
                self.error_current("SYN_EXPECTED_PATCH_INPUT");
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let patch = NameExpression {
                range: TextRange::new(
                    path.first().map_or(start, |name| name.range.start),
                    path.last().map_or(start, |name| name.range.end),
                ),
                path,
            };
            let Some(empty) = self.parse_named_failure_binding("empty", TokenKind::Empty) else {
                return Expression::Missing(TextRange::new(start, self.current().range.end));
            };
            let (changes, conditional_changes) = if self.at(TokenKind::Set) {
                self.bump();
                if self
                    .expect(TokenKind::Colon, "expected `:` after `set`")
                    .is_none()
                {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                }
                let Some(changes) = self.parse_patch_set_body() else {
                    return Expression::Missing(TextRange::new(start, self.current().range.end));
                };
                changes
            } else {
                (Vec::new(), Vec::new())
            };
            (None, changes, conditional_changes, Some(patch), Some(empty))
        } else {
            self.error_current("SYN_EXPECTED_UPDATE_BODY");
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let Some(missing) = self.parse_named_failure_binding("missing", TokenKind::Missing) else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let conflicts = self.parse_conflict_bindings();
        if conflicts.is_empty() {
            self.error_current("SYN_MUTATION_CONFLICT_REQUIRED");
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after update")
            .map_or_else(
                || conflicts.last().unwrap().range.end,
                |token| token.range.end,
            );
        Expression::Update(UpdateExpression {
            target,
            field,
            value,
            transition,
            changes,
            conditional_changes,
            patch,
            empty,
            missing,
            conflicts,
            range: TextRange::new(start, end),
        })
    }

    fn parse_patch_set_body(
        &mut self,
    ) -> Option<(Vec<FieldInitialiser>, Vec<PatchConditionalChange>)> {
        self.expect(TokenKind::LeftBrace, "expected `{`")?;
        let mut changes = Vec::new();
        let mut conditional_changes = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            let Some(change) = self.parse_field_initialiser() else {
                self.error_current("SYN_EXPECTED_FIELD_INITIALISER");
                self.recover_until(&[TokenKind::RightBrace]);
                continue;
            };
            if self.current().text(self.source) == "when" {
                self.bump();
                let Some(path) = self.parse_qualified_name() else {
                    self.error_current("SYN_EXPECTED_SUPPLIED_FIELD");
                    return None;
                };
                if self.current().text(self.source) != "supplied" {
                    self.error_current("SYN_EXPECTED_SUPPLIED");
                    return None;
                }
                let supplied_end = self.bump().range.end;
                let supplied = NameExpression {
                    range: TextRange::new(path[0].range.start, path.last()?.range.end),
                    path,
                };
                conditional_changes.push(PatchConditionalChange {
                    range: TextRange::new(change.range.start, supplied_end),
                    change,
                    supplied,
                });
            } else {
                changes.push(change);
            }
            if self.cursor == before {
                self.bump();
            }
        }
        self.expect(
            TokenKind::RightBrace,
            "expected `}` after patch-derived values",
        )?;
        Some((changes, conditional_changes))
    }

    fn parse_delete_expression(&mut self) -> Expression {
        let start = self.bump().range.start;
        let Some(target) = self.parse_required_mutation_target(start, "delete") else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let Some((field, value)) = self.parse_mutation_predicate(start, "delete") else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let Some(missing) = self.parse_named_failure_binding("missing", TokenKind::Missing) else {
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        };
        let conflicts = self.parse_conflict_bindings();
        if conflicts.is_empty() {
            self.error_current("SYN_MUTATION_CONFLICT_REQUIRED");
            return Expression::Missing(TextRange::new(start, self.current().range.end));
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after delete")
            .map_or_else(
                || conflicts.last().unwrap().range.end,
                |token| token.range.end,
            );
        Expression::Delete(DeleteExpression {
            target,
            field,
            value,
            missing,
            conflicts,
            range: TextRange::new(start, end),
        })
    }

    fn parse_required_mutation_target(
        &mut self,
        start: usize,
        operation: &str,
    ) -> Option<NameExpression> {
        self.expect(
            TokenKind::Required,
            &format!("expected `required` after `{operation}`"),
        )?;
        let path = self.parse_qualified_name()?;
        let range = TextRange::new(
            path.first().map_or(start, |name| name.range.start),
            path.last().map_or(start, |name| name.range.end),
        );
        self.expect(
            TokenKind::LeftBrace,
            &format!("expected `{{` after {operation} target"),
        )?;
        Some(NameExpression { path, range })
    }

    fn parse_mutation_predicate(
        &mut self,
        _start: usize,
        operation: &str,
    ) -> Option<(Name, Box<Expression>)> {
        self.expect(
            TokenKind::Where,
            &format!("expected `where` in {operation}"),
        )?;
        self.expect(TokenKind::Colon, "expected `:` after `where`")?;
        let field = self.expect_contextual_name("expected entity field after `where`")?;
        self.expect(TokenKind::EqualEqual, "expected `==` in mutation predicate")?;
        Some((field, Box::new(self.parse_expression())))
    }

    fn parse_named_failure_binding(
        &mut self,
        label: &str,
        kind: TokenKind,
    ) -> Option<RejectStatement> {
        self.expect(kind, &format!("expected `{label}` failure binding"))?;
        self.expect(TokenKind::Colon, &format!("expected `:` after `{label}`"))?;
        self.parse_failure_binding()
    }

    fn parse_conflict_bindings(&mut self) -> Vec<ConflictBinding> {
        let mut bindings = Vec::new();
        while self.at(TokenKind::Conflict) {
            let start = self.bump().range.start;
            let constraint = if self.at(TokenKind::Colon) {
                None
            } else {
                let path = self.parse_qualified_name();
                path.map(|path| {
                    let range = TextRange::new(
                        path.first().unwrap().range.start,
                        path.last().unwrap().range.end,
                    );
                    NameExpression { path, range }
                })
            };
            if self
                .expect(TokenKind::Colon, "expected `:` after conflict binding")
                .is_none()
            {
                break;
            }
            let Some(rejection) = self.parse_failure_binding() else {
                break;
            };
            let end = rejection.range.end;
            bindings.push(ConflictBinding {
                constraint,
                rejection,
                range: TextRange::new(start, end),
            });
        }
        bindings
    }

    fn parse_named_expression(&mut self) -> Expression {
        self.parse_named_expression_with_construction(true)
    }

    fn parse_named_expression_with_construction(&mut self, allow_construction: bool) -> Expression {
        let Some(path) = self.parse_qualified_name() else {
            return Expression::Missing(self.current().range);
        };
        let name_range = TextRange::new(
            path.first().map_or(0, |name| name.range.start),
            path.last().map_or(0, |name| name.range.end),
        );
        let target = NameExpression {
            path,
            range: name_range,
        };

        if self.at(TokenKind::LeftParen) {
            let Some((arguments, named_arguments, end)) = self.parse_arguments() else {
                let end = self.previous_significant_end().max(name_range.end);
                return Expression::Missing(TextRange::new(name_range.start, end));
            };
            Expression::Invocation(InvocationExpression {
                callee: target,
                arguments,
                named_arguments,
                range: TextRange::new(name_range.start, end),
            })
        } else if allow_construction && self.at(TokenKind::LeftBrace) {
            let Some((fields, end)) = self.parse_object_body() else {
                let end = self.previous_significant_end().max(name_range.end);
                return Expression::Missing(TextRange::new(name_range.start, end));
            };
            Expression::Construction(ConstructionExpression {
                target,
                fields,
                range: TextRange::new(name_range.start, end),
            })
        } else {
            Expression::Name(target)
        }
    }

    fn parse_arguments(&mut self) -> Option<(Vec<Expression>, Vec<FieldInitialiser>, usize)> {
        self.expect(TokenKind::LeftParen, "expected `(`")?;
        let mut arguments = Vec::new();
        let mut named_arguments = Vec::new();
        let mut named_seen = false;
        let mut named_names = BTreeSet::new();
        while !self.at(TokenKind::RightParen) && !self.at(TokenKind::Eof) {
            let is_named = matches!(
                self.current_kind(),
                TokenKind::Identifier
                    | TokenKind::Config
                    | TokenKind::Input
                    | TokenKind::Output
                    | TokenKind::Value
                    | TokenKind::Path
            ) && self.next_significant_kind() == Some(TokenKind::Colon);
            if is_named {
                named_seen = true;
                let start = self.current().range.start;
                let name = self.expect_contextual_name("expected named argument")?;
                self.expect(TokenKind::Colon, "expected `:` after named argument")?;
                let value = self.parse_expression();
                let range = TextRange::new(start, value.range().end);
                if !named_names.insert(name.text.clone()) {
                    self.error_at("SYN_NAMED_ARGUMENT_DUPLICATE", name.range);
                }
                named_arguments.push(FieldInitialiser { name, value, range });
            } else {
                let argument = self.parse_expression();
                if named_seen {
                    self.error_at("SYN_POSITIONAL_AFTER_NAMED", argument.range());
                }
                arguments.push(argument);
            }
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        let end = self
            .expect(TokenKind::RightParen, "expected `)` after arguments")?
            .range
            .end;
        Some((arguments, named_arguments, end))
    }

    fn parse_object_body(&mut self) -> Option<(Vec<FieldInitialiser>, usize)> {
        self.expect(TokenKind::LeftBrace, "expected `{`")?;
        let mut fields = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(field) = self.parse_field_initialiser() {
                fields.push(field);
            } else {
                self.error_current("SYN_EXPECTED_FIELD_INITIALISER");
                self.recover_until(&[TokenKind::RightBrace]);
            }
            if self.cursor == before {
                self.bump();
            }
        }
        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after object values")?
            .range
            .end;
        Some((fields, end))
    }

    fn parse_field_initialiser(&mut self) -> Option<FieldInitialiser> {
        let start = self.current().range.start;
        let name = self.expect_contextual_name("expected field name")?;
        self.expect(TokenKind::Colon, "expected `:` after field name")?;
        let value = self.parse_expression();
        let end = value.range().end;
        Some(FieldInitialiser {
            name,
            value,
            range: TextRange::new(start, end),
        })
    }

    fn parse_type_reference(&mut self) -> Option<TypeReference> {
        let path = self.parse_qualified_name()?;
        let start = path.first()?.range.start;
        if path.len() == 1 && path[0].text == "Object" && self.at(TokenKind::LeftBrace) {
            self.inline_type_counter += 1;
            let owner = self
                .field_owners
                .last()
                .map(String::as_str)
                .unwrap_or("Object");
            let synthetic_name = format!(
                "__jadpo_{}_{}",
                owner
                    .chars()
                    .map(|character| if character.is_ascii_alphanumeric() {
                        character
                    } else {
                        '_'
                    })
                    .collect::<String>(),
                self.inline_type_counter
            );
            let body = self.parse_record_body(RecordKind::Value, &synthetic_name)?;
            let end = body.end;
            self.synthetic_declarations
                .push(Declaration::Record(RecordDeclaration {
                    kind: RecordKind::Value,
                    name: Name {
                        text: synthetic_name.clone(),
                        range: path[0].range,
                    },
                    fields: body.fields,
                    inverses: body.inverses,
                    persistence_constraints: body.persistence_constraints,
                    dossier: body.dossier,
                    membership: body.membership,
                    policy: body.policy,
                    range: TextRange::new(start, end),
                }));
            let nullable = if self.at(TokenKind::Question) {
                self.bump();
                true
            } else {
                false
            };
            return Some(TypeReference {
                path: vec![Name {
                    text: synthetic_name,
                    range: path[0].range,
                }],
                arguments: Vec::new(),
                nullable,
                range: TextRange::new(start, self.previous_significant_end()),
            });
        }
        let mut arguments = Vec::new();

        if self.at(TokenKind::LeftAngle) {
            self.bump();
            while !self.at(TokenKind::RightAngle) && !self.at(TokenKind::Eof) {
                arguments.push(self.parse_type_reference()?);
                if self.at(TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RightAngle, "expected `>` after type arguments")?;
        }

        let nullable = if self.at(TokenKind::Question) {
            self.bump();
            true
        } else {
            false
        };
        let end = self.previous_significant_end();

        Some(TypeReference {
            path,
            arguments,
            nullable,
            range: TextRange::new(start, end),
        })
    }

    fn parse_qualified_name(&mut self) -> Option<Vec<Name>> {
        let mut path = vec![self.expect_contextual_name("expected name")?];
        while self.at(TokenKind::Dot) {
            self.bump();
            path.push(self.expect_contextual_name("expected name after `.`")?);
        }
        Some(path)
    }

    fn parse_field_block(&mut self) -> Option<(Vec<FieldDeclaration>, usize)> {
        self.expect(TokenKind::LeftBrace, "expected `{` before fields")?;
        let mut fields = Vec::new();

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(field) = self.parse_field_declaration(false) {
                fields.push(field);
            } else {
                self.recover_until(&[TokenKind::RightBrace]);
            }
            if self.cursor == before {
                self.bump();
            }
        }

        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after fields")?
            .range
            .end;
        Some((fields, end))
    }

    fn parse_field_declaration(&mut self, allow_default: bool) -> Option<FieldDeclaration> {
        let start = self.current().range.start;
        let name = self.expect_contextual_name("expected field name")?;
        self.expect(TokenKind::Colon, "expected `:` after field name")?;
        let owner = self.field_owners.last().map_or_else(
            || name.text.clone(),
            |owner| format!("{owner}_{}", name.text),
        );
        self.field_owners.push(owner);
        let field_type = self.parse_type_reference()?;
        self.field_owners.pop();
        let options = if self.at(TokenKind::LeftBrace) {
            self.parse_field_options()?
        } else {
            ParsedFieldOptions {
                constraints: Vec::new(),
                generated: None,
                role: None,
                immutable: false,
                policy: None,
            }
        };
        let default = if allow_default
            && self.current_is_on_same_line(self.previous_significant_end())
            && self.at_contextual("default")
        {
            self.bump();
            Some(self.parse_any_literal().or_else(|| {
                self.error_current("SYN_EXPECTED_LITERAL");
                None
            })?)
        } else {
            None
        };
        let generated_on = if self.at_contextual("generated") {
            self.bump();
            self.expect(TokenKind::LeftBrace, "expected `{` after `generated`")?;
            let on = self.expect_contextual_name("expected `on`")?;
            if on.text != "on" {
                self.error_at("SYN_GENERATED_ON_REQUIRED", on.range);
            }
            self.expect(TokenKind::Colon, "expected `:` after `on`")?;
            let role = self.expect_contextual_name("expected generated lifecycle role")?;
            let role = match role.text.as_str() {
                "create" => GeneratedFieldRole::Create,
                "create_or_change" => GeneratedFieldRole::CreateOrChange,
                _ => {
                    self.error_at("SYN_GENERATED_ROLE_INVALID", role.range);
                    GeneratedFieldRole::Create
                }
            };
            self.expect(
                TokenKind::RightBrace,
                "expected `}` after generated field role",
            )?;
            Some(role)
        } else {
            None
        };
        let generated = match (options.generated, generated_on) {
            (Some(identity), Some(_)) => {
                self.error_at("SYN_DUPLICATE_FIELD_MODIFIER", name.range);
                Some(identity)
            }
            (Some(identity), None) => Some(identity),
            (None, generated_on) => generated_on,
        };
        let postfix_anchor = self.previous_significant_end();
        let mut persistence = Vec::new();
        while self.current_is_on_same_line(postfix_anchor) {
            if self.next_significant_kind() == Some(TokenKind::Colon) {
                break;
            }
            let Some(modifier) = (match self.current_kind() {
                TokenKind::Identity => Some(PersistenceModifier::Identity),
                TokenKind::Unique => Some(PersistenceModifier::Unique),
                TokenKind::Index => Some(PersistenceModifier::Index),
                _ => None,
            }) else {
                break;
            };
            let range = self.current().range;
            self.bump();
            if persistence.contains(&modifier) {
                self.error_at("SYN_DUPLICATE_FIELD_MODIFIER", range);
            } else {
                persistence.push(modifier);
            }
        }
        let reference =
            if self.current_is_on_same_line(postfix_anchor) && self.at(TokenKind::References) {
                let reference_start = self.bump().range.start;
                let target = self.parse_type_reference()?;
                let relationship = if self.at(TokenKind::As) {
                    self.bump();
                    Some(self.expect_contextual_name("expected relationship name after `as`")?)
                } else {
                    None
                };
                self.expect(
                    TokenKind::OnDelete,
                    "expected `on_delete` after relationship target",
                )?;
                let on_delete = self.parse_reference_delete_action()?;
                let reference_end = self.previous_significant_end();
                Some(ReferenceDeclaration {
                    target,
                    relationship,
                    on_delete,
                    range: TextRange::new(reference_start, reference_end),
                })
            } else {
                None
            };
        let optional =
            if self.current_is_on_same_line(postfix_anchor) && self.at(TokenKind::Optional) {
                self.bump();
                true
            } else {
                false
            };
        let end = self.previous_significant_end();

        Some(FieldDeclaration {
            name,
            field_type,
            constraints: options.constraints,
            default,
            persistence,
            generated,
            reference,
            role: options.role,
            immutable: options.immutable,
            policy: options.policy,
            optional,
            range: TextRange::new(start, end),
        })
    }

    fn parse_field_options(&mut self) -> Option<ParsedFieldOptions> {
        self.expect(TokenKind::LeftBrace, "expected `{` before field options")?;
        let mut constraints = Vec::new();
        let mut generated = None;
        let mut role = None;
        let mut immutable = false;
        let mut policy = None;
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            if self.at_contextual("role") {
                let start = self.bump().range.start;
                self.expect(TokenKind::Colon, "expected `:` after field role")?;
                let path = self.parse_qualified_name()?;
                let range = TextRange::new(start, path.last()?.range.end);
                let expression_range =
                    TextRange::new(path.first()?.range.start, path.last()?.range.end);
                let binding = RoleBinding {
                    role: NameExpression {
                        path,
                        range: expression_range,
                    },
                    range,
                };
                if role.replace(binding).is_some() {
                    self.error_at("POLICY_BINDING_DUPLICATE", range);
                }
            } else if self.at_contextual("generated") {
                let start = self.bump().range.start;
                self.expect(TokenKind::Colon, "expected `:` after `generated`")?;
                let identity = self.expect(
                    TokenKind::Identity,
                    "expected `identity` after `generated:`",
                )?;
                if generated.replace(GeneratedFieldRole::Identity).is_some() {
                    self.error_at(
                        "SYN_DUPLICATE_FIELD_MODIFIER",
                        TextRange::new(start, identity.range.end),
                    );
                }
            } else if self.at_contextual("immutable") {
                self.bump();
                self.expect(TokenKind::Colon, "expected `:` after `immutable`")?;
                let value = self.parse_literal_of(TokenKind::BooleanLiteral)?;
                if value.text == "true" {
                    immutable = true;
                } else {
                    self.error_at("POLICY_BINDING_MUTABLE", value.range);
                }
            } else if self.at_contextual("policy") {
                let parsed = self.parse_policy_declaration()?;
                if policy.replace(parsed).is_some() {
                    self.error_at("POLICY_DECLARATION_DUPLICATE", self.current().range);
                }
            } else if let Some(constraint) = self.parse_constraint() {
                constraints.push(constraint);
            } else {
                self.error_current("SYN_UNEXPECTED_TOKEN");
                self.bump();
            }
        }
        self.expect(TokenKind::RightBrace, "expected `}` after field options")?;
        Some(ParsedFieldOptions {
            constraints,
            generated,
            role,
            immutable,
            policy,
        })
    }

    fn parse_reference_delete_action(&mut self) -> Option<ReferenceDeleteAction> {
        let action = match self.current_kind() {
            TokenKind::Restrict => ReferenceDeleteAction::Restrict,
            TokenKind::Cascade => ReferenceDeleteAction::Cascade,
            TokenKind::SetNull => ReferenceDeleteAction::SetNull,
            _ => {
                self.error_current("SYN_EXPECTED_DELETE_ACTION");
                return None;
            }
        };
        self.bump();
        Some(action)
    }

    fn parse_constraint_block(&mut self) -> Option<(Vec<Constraint>, usize)> {
        self.expect(TokenKind::LeftBrace, "expected `{` before constraints")?;
        let mut constraints = Vec::new();

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            if let Some(constraint) = self.parse_constraint() {
                constraints.push(constraint);
            } else {
                self.error_current("SYN_EXPECTED_CONSTRAINT");
                self.recover_until(&[
                    TokenKind::Min,
                    TokenKind::Max,
                    TokenKind::MinLength,
                    TokenKind::MaxLength,
                    TokenKind::Pattern,
                    TokenKind::Format,
                    TokenKind::RightBrace,
                ]);
            }
            if self.cursor == before {
                self.bump();
            }
        }

        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after constraints")?
            .range
            .end;
        Some((constraints, end))
    }

    fn parse_constraint(&mut self) -> Option<Constraint> {
        let start = self.current().range.start;
        let kind = match self.current_kind() {
            TokenKind::Min => ConstraintKind::Min,
            TokenKind::Max => ConstraintKind::Max,
            TokenKind::MinLength => ConstraintKind::MinLength,
            TokenKind::MaxLength => ConstraintKind::MaxLength,
            TokenKind::Pattern => ConstraintKind::Pattern,
            TokenKind::Format => ConstraintKind::Format,
            _ => return None,
        };
        let name = self.bump();
        if self.at(TokenKind::Colon) {
            self.bump();
        } else {
            let constraint_name = name.text(self.source).to_owned();
            let mut diagnostic = Diagnostic::error("SYN_CONSTRAINT_COLON_REQUIRED")
                .with_fact(DiagnosticFact::Name(constraint_name.clone()))
                .with_edit(TextEdit {
                    source: self.source_name.clone(),
                    start: name.range.end,
                    end: name.range.end,
                    replacement: ":".to_owned(),
                });
            diagnostic.recommended_next_step.title =
                format!("Insert `:` after `{constraint_name}`");
            self.diagnostic_at(diagnostic, name.range);
        }

        let value = match kind {
            ConstraintKind::Min | ConstraintKind::Max => self.parse_number_literal(),
            ConstraintKind::MinLength | ConstraintKind::MaxLength => {
                self.parse_literal_of(TokenKind::IntegerLiteral)
            }
            ConstraintKind::Pattern => self.parse_literal_of(TokenKind::StringLiteral),
            ConstraintKind::Format => self.parse_name_literal(),
        }
        .unwrap_or_else(|| {
            self.error_current("SYN_EXPECTED_CONSTRAINT_VALUE");
            Literal {
                kind: LiteralKind::String,
                text: String::new(),
                range: self.current().range,
            }
        });
        let end = value.range.end;
        Some(Constraint {
            kind,
            value,
            range: TextRange::new(start, end),
        })
    }

    fn parse_number_literal(&mut self) -> Option<Literal> {
        match self.current_kind() {
            TokenKind::IntegerLiteral | TokenKind::DecimalLiteral => self.parse_any_literal(),
            _ => None,
        }
    }

    fn parse_name_literal(&mut self) -> Option<Literal> {
        let token = self.expect_name_token("expected constraint format name")?;
        Some(Literal {
            kind: LiteralKind::String,
            text: token.text(self.source).to_owned(),
            range: token.range,
        })
    }

    fn parse_literal_of(&mut self, expected: TokenKind) -> Option<Literal> {
        if !self.at(expected) {
            self.error_current("SYN_EXPECTED_LITERAL");
            return None;
        }
        self.parse_any_literal()
    }

    fn parse_any_literal(&mut self) -> Option<Literal> {
        let token = self.current();
        let kind = match token.kind {
            TokenKind::StringLiteral => LiteralKind::String,
            TokenKind::IntegerLiteral => LiteralKind::Integer,
            TokenKind::DecimalLiteral => LiteralKind::Decimal,
            TokenKind::BooleanLiteral => LiteralKind::Boolean,
            TokenKind::NoneLiteral => LiteralKind::None,
            _ => return None,
        };
        self.bump();
        Some(Literal {
            kind,
            text: token.text(self.source).to_owned(),
            range: token.range,
        })
    }

    fn parse_route_declaration(&mut self) -> Option<RouteDeclaration> {
        let start = self
            .expect(TokenKind::Route, "expected `route`")?
            .range
            .start;
        let method = match self.current_kind() {
            TokenKind::Get => HttpMethod::Get,
            TokenKind::Post => HttpMethod::Post,
            TokenKind::Put => HttpMethod::Put,
            TokenKind::Patch => HttpMethod::Patch,
            TokenKind::Delete => HttpMethod::Delete,
            _ => {
                self.error_current("SYN_EXPECTED_HTTP_METHOD");
                return None;
            }
        };
        self.bump();
        let path_token = self.expect(TokenKind::RoutePath, "expected route path")?;
        let path = path_token.text(self.source).to_owned();
        self.expect(TokenKind::LeftBrace, "expected `{` after route path")?;

        let mut public = false;
        let mut fresh_authority = false;
        let mut auth_seen = false;
        let mut path_fields = Vec::new();
        let mut path_seen = false;
        let mut query = None;
        let mut headers = Vec::new();
        let mut headers_seen = false;
        let mut input = None;
        let mut output = None;
        let mut success = RouteSuccess::default();
        let mut success_seen = false;
        let mut deadline: Option<DurationLiteral> = None;
        let mut deadline_seen = false;
        let mut run = None;
        let mut inline_action = None;

        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let before = self.cursor;
            match self.current_kind() {
                TokenKind::Auth => {
                    let item = self.bump();
                    if auth_seen {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    auth_seen = true;
                    let colon_end = self.expect_route_item_colon(item.range);
                    if self.at(TokenKind::NoneLiteral) {
                        self.bump();
                        public = true;
                    } else if self.at_contextual("fresh") {
                        self.bump();
                        fresh_authority = true;
                    } else {
                        let invalid = self.current();
                        let value_is_missing = matches!(
                            invalid.kind,
                            TokenKind::Auth
                                | TokenKind::Path
                                | TokenKind::Query
                                | TokenKind::Headers
                                | TokenKind::Input
                                | TokenKind::Output
                                | TokenKind::Success
                                | TokenKind::Deadline
                                | TokenKind::Run
                                | TokenKind::Action
                                | TokenKind::RightBrace
                                | TokenKind::Eof
                        );
                        // Recover the retired two-word spelling as one invalid value.
                        // Neither diagnostic choice is applied without a human decision.
                        let legacy_end = if invalid.kind == TokenKind::Public {
                            self.tokens[self.cursor + 1..]
                                .iter()
                                .find(|token| !token.kind.is_trivia())
                                .filter(|token| token.text(self.source) == "explicitly")
                                .map(|token| token.range.end)
                        } else {
                            None
                        };
                        let invalid_range = if value_is_missing {
                            TextRange::new(colon_end, colon_end)
                        } else {
                            TextRange::new(
                                invalid.range.start,
                                legacy_end.unwrap_or(invalid.range.end),
                            )
                        };
                        let found = if value_is_missing {
                            "missing value".to_owned()
                        } else if legacy_end.is_some() {
                            "public explicitly".to_owned()
                        } else {
                            diagnostic_token_label(invalid, self.source)
                        };
                        let route = format!("{} {}", http_method_name(method), path);
                        let (remove_start, remove_end) = route_item_removal_range(
                            self.source,
                            item.range.start,
                            if value_is_missing {
                                colon_end
                            } else {
                                invalid_range.end
                            },
                        );
                        let mut diagnostic = Diagnostic::error("ROUTE_AUTH_VALUE_INVALID")
                            .with_fact(DiagnosticFact::Route(route.clone()))
                            .with_fact(DiagnosticFact::FoundValue(found));
                        diagnostic.impact.affected.push(route);
                        diagnostic.alternatives[0].edits.push(TextEdit {
                            source: self.source_name.clone(),
                            start: remove_start,
                            end: remove_end,
                            replacement: String::new(),
                        });
                        diagnostic.alternatives[1].edits.push(TextEdit {
                            source: self.source_name.clone(),
                            start: invalid_range.start,
                            end: invalid_range.end,
                            replacement: if value_is_missing {
                                " none".to_owned()
                            } else {
                                "none".to_owned()
                            },
                        });
                        self.diagnostic_at(diagnostic, invalid_range);
                        if !value_is_missing {
                            self.bump();
                            if legacy_end.is_some() {
                                self.bump();
                            }
                        }
                    }
                }
                TokenKind::Path => {
                    let item = self.bump();
                    if path_seen {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    path_seen = true;
                    self.expect_route_item_colon(item.range);
                    if let Some((fields, _)) = self.parse_field_block() {
                        for field in &fields {
                            if field.field_type.nullable
                                || field.optional
                                || !field.constraints.is_empty()
                                || !field.persistence.is_empty()
                                || field.reference.is_some()
                            {
                                self.error_at("ROUTE_PATH_FIELD_MODIFIER_INVALID", field.range);
                            }
                        }
                        path_fields = fields;
                    }
                }
                TokenKind::Input => {
                    let item = self.bump();
                    if input.is_some() {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    self.expect_route_item_colon(item.range);
                    input = self.parse_type_reference();
                }
                TokenKind::Query => {
                    let item = self.bump();
                    if query.is_some() {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    self.expect_route_item_colon(item.range);
                    query = self.parse_type_reference();
                }
                TokenKind::Headers => {
                    let item = self.bump();
                    if headers_seen {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    headers_seen = true;
                    self.expect_route_item_colon(item.range);
                    self.expect(TokenKind::LeftBrace, "expected `{` after route headers")?;
                    while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                        let field_start = self.current().range.start;
                        let name =
                            self.expect_contextual_name("expected route header binding name")?;
                        self.expect(
                            TokenKind::Colon,
                            "expected `:` after route header binding name",
                        )?;
                        let field_type = self.parse_type_reference()?;
                        let from =
                            self.expect_contextual_name("expected `from` before HTTP header name")?;
                        if from.text != "from" {
                            self.error_at("ROUTE_HEADER_FROM_REQUIRED", from.range);
                        }
                        let wire_name = self.parse_literal_of(TokenKind::StringLiteral)?;
                        let optional = if self.at(TokenKind::Optional) {
                            self.bump();
                            true
                        } else {
                            false
                        };
                        headers.push(RouteHeaderBinding {
                            name,
                            field_type,
                            wire_name,
                            optional,
                            range: TextRange::new(field_start, self.previous_significant_end()),
                        });
                    }
                    self.expect(TokenKind::RightBrace, "expected `}` after route headers")?;
                }
                TokenKind::Output => {
                    let item = self.bump();
                    if output.is_some() {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    self.expect_route_item_colon(item.range);
                    output = self.parse_type_reference();
                }
                TokenKind::Success => {
                    let item = self.bump();
                    if success_seen {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    success_seen = true;
                    self.expect_route_item_colon(item.range);
                    if self.at_contextual("created") {
                        self.bump();
                        success = RouteSuccess::Created;
                    } else if self.at_contextual("no_content") {
                        self.bump();
                        success = RouteSuccess::NoContent;
                    } else {
                        self.error_current("SYN_UNEXPECTED_TOKEN");
                        if !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
                            self.bump();
                        }
                    }
                }
                TokenKind::Deadline => {
                    let item = self.bump();
                    if deadline_seen {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    deadline_seen = true;
                    self.expect_route_item_colon(item.range);
                    deadline = self.parse_config_default().map(|value| DurationLiteral {
                        text: value.text,
                        range: value.range,
                    });
                }
                TokenKind::Run => {
                    let item = self.bump();
                    if run.is_some() {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    self.expect_route_item_colon(item.range);
                    let expression = self.parse_named_expression();
                    match expression {
                        Expression::Invocation(invocation) => run = Some(invocation),
                        other => self.error_at("SYN_EXPECTED_INVOCATION", other.range()),
                    }
                }
                TokenKind::Action => {
                    let item = self.bump();
                    if inline_action.is_some() {
                        self.error_at("ROUTE_ITEM_DUPLICATE", item.range);
                    }
                    self.expect_route_item_colon(item.range);
                    let action_start = item.range.start;
                    let mut failures = Vec::new();
                    let mut failures_range = None;
                    if self.at(TokenKind::Fails) {
                        let failures_start = self.bump().range.start;
                        loop {
                            if let Some(failure) =
                                self.expect_name("expected problem name after `fails`")
                            {
                                failures.push(failure);
                            }
                            if !self.at(TokenKind::Comma) {
                                break;
                            }
                            self.bump();
                        }
                        failures_range = failures
                            .last()
                            .map(|failure| TextRange::new(failures_start, failure.range.end));
                    }
                    if let Some(body) = self.parse_block() {
                        inline_action = Some(InlineAction {
                            range: TextRange::new(action_start, body.range.end),
                            failures,
                            failures_range,
                            body,
                        });
                    }
                }
                _ => {
                    self.error_current("SYN_EXPECTED_ROUTE_ITEM");
                    self.recover_until(&[
                        TokenKind::Auth,
                        TokenKind::Path,
                        TokenKind::Query,
                        TokenKind::Headers,
                        TokenKind::Input,
                        TokenKind::Output,
                        TokenKind::Success,
                        TokenKind::Deadline,
                        TokenKind::Run,
                        TokenKind::Action,
                        TokenKind::RightBrace,
                    ]);
                }
            }
            if self.cursor == before {
                self.bump();
            }
        }

        let end = self
            .expect(TokenKind::RightBrace, "expected `}` after route")?
            .range
            .end;

        if run.is_some() == inline_action.is_some() {
            let (code, _message) = if run.is_some() {
                (
                    "ROUTE_BEHAVIOUR_CONFLICT",
                    "route must contain either `run:` or an inline `action`, not both",
                )
            } else {
                (
                    "ROUTE_BEHAVIOUR_REQUIRED",
                    "route must contain exactly one of `run:` or an inline `action`",
                )
            };
            self.error_at(code, TextRange::new(start, path_token.range.end));
        }

        let mut placeholders = Vec::new();
        let mut remaining = path.as_str();
        while let Some(open) = remaining.find('{') {
            let after_open = &remaining[open + 1..];
            let Some(close) = after_open.find('}') else {
                self.error_at("ROUTE_PATH_PLACEHOLDER_INVALID", path_token.range);
                break;
            };
            let name = &after_open[..close];
            if name.is_empty()
                || !name.chars().enumerate().all(|(index, character)| {
                    if index == 0 {
                        character.is_ascii_alphabetic() || character == '_'
                    } else {
                        character.is_ascii_alphanumeric() || character == '_'
                    }
                })
            {
                self.error_at("ROUTE_PATH_PLACEHOLDER_INVALID", path_token.range);
            } else {
                placeholders.push(name.to_owned());
            }
            remaining = &after_open[close + 1..];
        }
        let mut unique_placeholders = BTreeSet::new();
        for placeholder in &placeholders {
            if !unique_placeholders.insert(placeholder.clone()) {
                self.error_at("ROUTE_PATH_PLACEHOLDER_DUPLICATE", path_token.range);
            }
        }
        let declared = path_fields
            .iter()
            .map(|field| field.name.text.clone())
            .collect::<BTreeSet<_>>();
        for placeholder in &unique_placeholders {
            if !declared.contains(placeholder) {
                self.error_at("ROUTE_PATH_BINDING_MISSING", path_token.range);
            }
        }
        let mut seen_bindings = BTreeSet::new();
        for field in &path_fields {
            if !seen_bindings.insert(field.name.text.clone()) {
                self.error_at("ROUTE_PATH_BINDING_DUPLICATE", field.range);
            } else if !unique_placeholders.contains(&field.name.text) {
                self.error_at("ROUTE_PATH_BINDING_EXTRA", field.range);
            }
        }

        Some(RouteDeclaration {
            method,
            path,
            path_range: path_token.range,
            public,
            fresh_authority,
            path_fields,
            query,
            headers,
            input,
            output,
            success,
            deadline,
            run,
            inline_action,
            range: TextRange::new(start, end),
        })
    }

    fn expect_name(&mut self, message: &str) -> Option<Name> {
        let token = self.expect(TokenKind::Identifier, message)?;
        Some(Name {
            text: token.text(self.source).to_owned(),
            range: token.range,
        })
    }

    fn expect_contextual_name(&mut self, message: &str) -> Option<Name> {
        let token = self.expect_name_token(message)?;
        Some(Name {
            text: token.text(self.source).to_owned(),
            range: token.range,
        })
    }

    fn expect_name_token(&mut self, _message: &str) -> Option<Token> {
        let text = self.current().text(self.source);
        if text
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            && text
                .chars()
                .skip(1)
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            Some(self.bump())
        } else {
            self.error_current("SYN_EXPECTED_NAME");
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, message: &str) -> Option<Token> {
        if self.at(kind) {
            Some(self.bump())
        } else {
            let range = self.current().range;
            let diagnostic = self.expected_diagnostic("SYN_UNEXPECTED_TOKEN", message);
            self.diagnostic_at(diagnostic, range);
            None
        }
    }

    fn expected_diagnostic(&self, code: &'static str, message: &str) -> Diagnostic {
        let expected = message
            .strip_prefix("expected ")
            .unwrap_or(message)
            .to_owned();
        Diagnostic::error(code)
            .with_fact(DiagnosticFact::Expected(expected))
            .with_fact(DiagnosticFact::FoundValue(diagnostic_token_label(
                self.current(),
                self.source,
            )))
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    fn at_contextual(&self, value: &str) -> bool {
        self.current().text(self.source) == value
    }

    fn current_is_on_same_line(&self, previous_end: usize) -> bool {
        !self.source[previous_end..self.current().range.start].contains('\n')
    }

    fn current_kind(&self) -> TokenKind {
        self.current().kind
    }

    fn current(&self) -> Token {
        self.tokens[self.cursor]
    }

    fn next_significant_kind(&self) -> Option<TokenKind> {
        self.tokens[self.cursor + 1..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map(|token| token.kind)
    }

    fn next_significant_is_contextual(&self, value: &str) -> bool {
        self.tokens[self.cursor + 1..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .is_some_and(|token| token.text(self.source) == value)
    }

    fn is_object_keyword_typo(&self, parent: &TypeReference) -> bool {
        if parent.path.len() != 1
            || !parent.arguments.is_empty()
            || parent.nullable
            || !self.at(TokenKind::LeftBrace)
            || edit_distance(&parent.path[0].text.to_ascii_lowercase(), "object") > 2
        {
            return false;
        }

        let mut body_tokens = self.tokens[self.cursor + 1..]
            .iter()
            .filter(|token| !token.kind.is_trivia());
        match body_tokens.next().map(|token| token.kind) {
            Some(TokenKind::RightBrace) => true,
            Some(_) => body_tokens
                .next()
                .is_some_and(|token| token.kind == TokenKind::Colon),
            None => false,
        }
    }

    fn bump(&mut self) -> Token {
        let token = self.current();
        if token.kind != TokenKind::Eof {
            self.cursor += 1;
            self.skip_trivia();
        }
        token
    }

    fn skip_trivia(&mut self) {
        while self
            .tokens
            .get(self.cursor)
            .is_some_and(|token| token.kind.is_trivia())
        {
            self.cursor += 1;
        }
    }

    fn previous_significant_end(&self) -> usize {
        self.tokens[..self.cursor]
            .iter()
            .rev()
            .find(|token| !token.kind.is_trivia())
            .map_or(0, |token| token.range.end)
    }

    fn error_current(&mut self, code: &'static str) {
        self.error_at(code, self.current().range);
    }

    fn expect_route_item_colon(&mut self, item_range: TextRange) -> usize {
        if self.at(TokenKind::Colon) {
            return self.bump().range.end;
        }
        let mut diagnostic = Diagnostic::error("ROUTE_ITEM_COLON_REQUIRED").with_edit(TextEdit {
            source: self.source_name.clone(),
            start: item_range.end,
            end: item_range.end,
            replacement: ":".to_owned(),
        });
        diagnostic.primary = Some(SourceSpan {
            source: self.source_name.clone(),
            start: item_range.start,
            end: item_range.end,
        });
        self.diagnostics.push(diagnostic);
        item_range.end
    }

    fn expect_failure_item_colon(&mut self, item: Token) {
        if self.at(TokenKind::Colon) {
            self.bump();
            return;
        }
        let name = item.text(self.source).to_owned();
        let mut diagnostic = Diagnostic::error("SYN_FAILURE_ITEM_COLON_REQUIRED")
            .with_fact(DiagnosticFact::Name(name.clone()))
            .with_edit(TextEdit {
                source: self.source_name.clone(),
                start: item.range.end,
                end: item.range.end,
                replacement: ":".to_owned(),
            });
        diagnostic.recommended_next_step.title = format!("Insert `:` after `{name}`");
        self.diagnostic_at(diagnostic, item.range);
    }

    fn error_at(&mut self, code: &'static str, range: TextRange) {
        self.diagnostic_at(Diagnostic::error(code), range);
    }

    fn diagnostic_at(&mut self, mut diagnostic: Diagnostic, range: TextRange) {
        diagnostic.primary = Some(SourceSpan {
            source: self.source_name.clone(),
            start: range.start,
            end: range.end,
        });
        self.diagnostics.push(diagnostic);
    }

    fn recover_declaration(&mut self) {
        self.recover_until(&[
            TokenKind::Type,
            TokenKind::Enum,
            TokenKind::Entity,
            TokenKind::Value,
            TokenKind::Input,
            TokenKind::Output,
            TokenKind::Failure,
            TokenKind::Function,
            TokenKind::Action,
            TokenKind::Test,
            TokenKind::Route,
            TokenKind::Eof,
        ]);
    }

    fn recover_statement(&mut self) {
        self.recover_until(&[
            TokenKind::Var,
            TokenKind::Return,
            TokenKind::Reject,
            TokenKind::If,
            TokenKind::Match,
            TokenKind::Assert,
            TokenKind::Throw,
            TokenKind::RightBrace,
            TokenKind::Eof,
        ]);
    }

    fn recover_until(&mut self, kinds: &[TokenKind]) {
        while !kinds.contains(&self.current_kind()) && !self.at(TokenKind::Eof) {
            self.bump();
        }
    }
}

fn http_method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    for (left_index, left_character) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_character) in right.chars().enumerate() {
            current.push(
                (previous[right_index + 1] + 1).min(
                    (current[right_index] + 1).min(
                        previous[right_index] + usize::from(left_character != right_character),
                    ),
                ),
            );
        }
        previous = current;
    }
    previous.last().copied().unwrap_or_default()
}

fn parse_service_items(source: &str, range: TextRange) -> Vec<ServiceItem> {
    let mut items = Vec::new();
    let mut sections = Vec::new();
    let mut offset = range.start;
    for raw_line in source[range.start..range.end].split_inclusive('\n') {
        let line = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            offset += raw_line.len();
            continue;
        }
        let leading = line.len() - line.trim_start().len();
        let item_start = offset + leading;
        let item_end = offset + line.trim_end().len();
        if trimmed == "}" {
            sections.pop();
        } else if let Some(section) = trimmed.strip_suffix('{') {
            sections.push(section.trim().to_owned());
        } else {
            let (key, value) = trimmed
                .split_once(':')
                .or_else(|| {
                    trimmed
                        .find(char::is_whitespace)
                        .map(|index| (&trimmed[..index], trimmed[index..].trim()))
                })
                .unwrap_or((trimmed, ""));
            items.push(ServiceItem {
                path: sections.clone(),
                key: key.trim().to_owned(),
                value: value.trim().to_owned(),
                range: TextRange::new(item_start, item_end),
            });
        }
        offset += raw_line.len();
    }
    items
}

fn diagnostic_token_label(token: Token, source: &str) -> String {
    let text = token.text(source);
    if !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return text.to_owned();
    }
    match token.kind {
        TokenKind::StringLiteral => "string literal".to_owned(),
        TokenKind::IntegerLiteral | TokenKind::DecimalLiteral => "number literal".to_owned(),
        TokenKind::RoutePath => "route path".to_owned(),
        TokenKind::LeftBrace => "opening brace".to_owned(),
        TokenKind::RightBrace => "closing brace".to_owned(),
        TokenKind::Colon => "colon".to_owned(),
        TokenKind::Comma => "comma".to_owned(),
        TokenKind::LeftParen => "(".to_owned(),
        TokenKind::RightParen => ")".to_owned(),
        TokenKind::LeftAngle => "<".to_owned(),
        TokenKind::RightAngle => ">".to_owned(),
        TokenKind::Dot => ".".to_owned(),
        TokenKind::Question => "?".to_owned(),
        TokenKind::Equal => "=".to_owned(),
        TokenKind::EqualEqual => "==".to_owned(),
        TokenKind::BangEqual => "!=".to_owned(),
        TokenKind::LessEqual => "<=".to_owned(),
        TokenKind::GreaterEqual => ">=".to_owned(),
        TokenKind::Plus => "+".to_owned(),
        TokenKind::Minus => "-".to_owned(),
        TokenKind::Star => "*".to_owned(),
        TokenKind::Slash => "/".to_owned(),
        TokenKind::Percent => "%".to_owned(),
        TokenKind::Arrow => "->".to_owned(),
        TokenKind::FatArrow => "=>".to_owned(),
        TokenKind::Eof => "end of file".to_owned(),
        _ => "this token".to_owned(),
    }
}

fn route_item_removal_range(source: &str, item_start: usize, value_end: usize) -> (usize, usize) {
    let line_start = source[..item_start]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let line_end = source[value_end..]
        .find('\n')
        .map_or(source.len(), |offset| value_end + offset + 1);
    let value_line_end = line_end.saturating_sub(usize::from(
        line_end > 0 && source.as_bytes()[line_end - 1] == b'\n',
    ));
    if source[line_start..item_start].trim().is_empty()
        && source[value_end..value_line_end].trim().is_empty()
    {
        (line_start, line_end)
    } else {
        (item_start, value_end)
    }
}

fn add_persistence_modifier(
    fields: &mut [FieldDeclaration],
    field_name: &Name,
    modifier: PersistenceModifier,
) {
    if let Some(field) = fields
        .iter_mut()
        .find(|field| field.name.text == field_name.text)
    {
        if !field.persistence.contains(&modifier) {
            field.persistence.push(modifier);
        }
    }
}

fn declaration_name(declaration: &Declaration) -> Option<&Name> {
    match declaration {
        Declaration::Application(declaration) => Some(&declaration.name),
        Declaration::Locales(_) => None,
        Declaration::AuthenticationStrategy(declaration) => Some(&declaration.name),
        Declaration::Principal(declaration) => Some(&declaration.name),
        Declaration::Config(declaration) => Some(&declaration.name),
        Declaration::Type(declaration) => Some(&declaration.name),
        Declaration::Enum(declaration) => Some(&declaration.name),
        Declaration::Record(declaration) => Some(&declaration.name),
        Declaration::Failure(declaration) => Some(&declaration.name),
        Declaration::Callable(declaration) => Some(&declaration.name),
        Declaration::Fixture(declaration) => Some(&declaration.name),
        Declaration::Test(_) => None,
        Declaration::Route(_) => None,
        Declaration::Job(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::{
        CallableKind, CredentialLocation, Declaration, HttpMethod, PersistenceConstraintKind,
        PersistenceModifier, PrincipalVariantKind, QueryOrderDirection, RecordKind,
        ReferenceDeleteAction, RouteSuccess, Statement,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("syntax crate should be inside the repository")
    }

    #[test]
    fn captures_balanced_service_contract_without_confusing_nested_blocks() {
        let path = repository_root().join("tests/assurance/service-successor-v0.1.jadpo");
        let full_source = fs::read_to_string(&path).expect("service contract should be readable");
        let parsed = parse(&path, &full_source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        assert_eq!(parsed.file.services.len(), 1);
        assert_eq!(parsed.file.services[0].name.text, "ReminderMail");
        let items = &parsed.file.services[0].items;
        assert!(items.iter().any(|item| {
            item.path.len() == 1
                && item.path[0] == "import"
                && item.key == "file"
                && item.value == "\"tests/assurance/service-reference-mail-v0.1.json\""
        }));
        assert!(items
            .iter()
            .any(|item| item.key == "POST" && item.value == "/v1/messages"));
        assert!(items
            .iter()
            .any(|item| item.value.contains("OutcomeUnknown")));
    }

    #[test]
    fn parses_the_jadpo_seed_without_diagnostics() {
        let path = repository_root().join("examples/jadpo-seed/app.jadpo");
        let source = fs::read_to_string(&path).expect("seed should be readable");
        let parsed = parse(&path, &source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let outline = parsed
            .file
            .declarations
            .iter()
            .map(declaration_outline)
            .collect::<Vec<_>>();
        assert_eq!(
            outline,
            vec![
                "type InviteCode",
                "value Customer",
                "value RegisterCustomer",
                "value RegistrationAccepted",
                "failure InviteCodeRejected",
                "action register_customer",
                "route POST /registrations",
                "route GET /registrations/{email}",
            ]
        );
    }

    #[test]
    fn explains_when_record_fields_are_written_as_a_type_alias() {
        let source = "type User = {\n    id: UUID\n}\n";
        let parsed = parse(Path::new("inline.jadpo"), source);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].code, "SYN_TYPE_PARENT_REQUIRED");
        assert_eq!(parsed.diagnostics[0].primary.as_ref().unwrap().start, 12);
        assert!(parsed.diagnostics[0].notes[0].contains("type User = Object"));
    }

    #[test]
    fn parses_module_headers_selective_imports_and_public_declarations() {
        let source = r#"
module todo.api
import todo.domain { Todo, TodoTitle }

public input CreateTodo { title: TodoTitle }
output PrivateResult { todo: Todo }
"#;
        let parsed = parse(Path::new("api.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        assert_eq!(
            parsed
                .file
                .module
                .as_ref()
                .unwrap()
                .path
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>(),
            vec!["todo", "api"]
        );
        assert_eq!(parsed.file.imports.len(), 1);
        assert_eq!(
            parsed.file.imports[0]
                .names
                .iter()
                .map(|name| name.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Todo", "TodoTitle"]
        );
        assert_eq!(
            parsed
                .file
                .exports
                .iter()
                .map(|name| name.text.as_str())
                .collect::<Vec<_>>(),
            vec!["CreateTodo"]
        );
    }

    #[test]
    fn requires_colons_between_route_items_and_values() {
        let source = "route GET /health { auth public explicitly output Health run health() }";
        let parsed = parse(Path::new("inline.jadpo"), source);

        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "ROUTE_ITEM_COLON_REQUIRED"));
        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "ROUTE_AUTH_VALUE_INVALID"));
    }

    #[test]
    fn parses_typed_route_query_and_explicit_header_bindings() {
        let source = r#"
type PageSize = Int { min: 1 max: 100 }
type ListTodos = Object { page_size: PageSize default 25 }
route GET /todos {
    auth: none
    query: ListTodos
    headers: { trace_id: Text from "X-Trace" optional }
    action: { return true }
}
"#;
        let parsed = parse(Path::new("route-inputs.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let route = parsed
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Route(route) => Some(route),
                _ => None,
            })
            .expect("route");
        assert_eq!(route.query.as_ref().unwrap().path[0].text, "ListTodos");
        assert_eq!(route.headers.len(), 1);
        assert_eq!(route.headers[0].name.text, "trace_id");
        assert_eq!(route.headers[0].wire_name.text, "\"X-Trace\"");
        assert!(route.headers[0].optional);
        let list = parsed
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Record(record) if record.name.text == "ListTodos" => Some(record),
                _ => None,
            })
            .expect("query record");
        assert_eq!(list.fields[0].default.as_ref().unwrap().text, "25");
    }

    #[test]
    fn route_header_binding_requires_from_keyword() {
        let parsed = parse(
            Path::new("route-header-from.jadpo"),
            "route GET /items { auth: none headers: { trace: Text wire \"X-Trace\" } action: { return true } }",
        );
        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "ROUTE_HEADER_FROM_REQUIRED"));
    }

    #[test]
    fn parses_explicit_created_and_no_content_route_successes() {
        let source = r#"output CreatedItem { id: Uuid }
action make_item() -> CreatedItem { return CreatedItem { id: Uuid("00000000-0000-4000-8000-000000000001") } }
route POST /items { auth: none output: CreatedItem run: make_item() success: created }
route DELETE /items { auth: none action: {} success: no_content }
"#;
        let parsed = parse(Path::new("route-success.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let successes = parsed
            .file
            .declarations
            .iter()
            .filter_map(|declaration| match declaration {
                Declaration::Route(route) => Some(route.success),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(successes, [RouteSuccess::Created, RouteSuccess::NoContent]);
    }

    #[test]
    fn invalid_route_auth_value_is_one_human_owned_root_diagnostic() {
        let source = r#"input RegisterCustomer { email: Text }
output RegistrationAccepted { accepted: Bool }
action register_customer(input: RegisterCustomer) -> RegistrationAccepted {
    return RegistrationAccepted { accepted: true }
}
route POST /registrations {
    auth: nonke
    input: RegisterCustomer
    output: RegistrationAccepted
    run: register_customer(input)
}
"#;
        let parsed = parse(Path::new("auth-value.jadpo"), source);

        assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
        let diagnostic = &parsed.diagnostics[0];
        assert_eq!(diagnostic.code, "ROUTE_AUTH_VALUE_INVALID");
        assert_eq!(
            diagnostic.message,
            "Route authentication value `nonke` is not valid"
        );
        assert_eq!(diagnostic.decision_owner.as_str(), "human");
        assert_eq!(diagnostic.alternatives.len(), 2);
        assert!(diagnostic
            .alternatives
            .iter()
            .all(|choice| !choice.preferred && choice.edits.len() == 1));
        let primary = diagnostic.primary.as_ref().expect("primary range");
        assert_eq!(&source[primary.start..primary.end], "nonke");

        let route = parsed
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Route(route) => Some(route),
                _ => None,
            })
            .expect("the route should survive parser recovery");
        assert!(route.input.is_some());
        assert!(route.output.is_some());
        assert!(route.run.is_some());
        assert!(!route.public);
    }

    #[test]
    fn legacy_public_auth_has_one_complete_human_owned_repair() {
        let source = "route GET /identity { auth: public explicitly action: { return true } }";
        let parsed = parse(Path::new("legacy-public-auth.jadpo"), source);
        assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
        let diagnostic = &parsed.diagnostics[0];
        assert_eq!(diagnostic.code, "ROUTE_AUTH_VALUE_INVALID");
        assert_eq!(
            diagnostic.message,
            "Route authentication value `public explicitly` is not valid"
        );
        assert_eq!(diagnostic.decision_owner.as_str(), "human");
        let Declaration::Route(route) = &parsed.file.declarations[0] else {
            panic!("route")
        };
        assert!(!route.public);
        assert!(route.inline_action.is_some());
        for choice in &diagnostic.alternatives {
            let edit = &choice.edits[0];
            let repaired = format!(
                "{}{}{}",
                &source[..edit.start],
                edit.replacement,
                &source[edit.end..]
            );
            let checked = parse(Path::new("repaired-auth.jadpo"), &repaired);
            assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
            let Declaration::Route(route) = &checked.file.declarations[0] else {
                panic!("route")
            };
            assert_eq!(route.public, edit.replacement == "none");
        }
    }

    #[test]
    fn parses_fresh_authority_route_requirement() {
        let source = "route POST /sensitive { auth: fresh action: { return true } }";
        let parsed = parse(Path::new("fresh-auth.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let route = parsed
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Route(route) => Some(route),
                _ => None,
            })
            .expect("route");
        assert!(!route.public);
        assert!(route.fresh_authority);
    }

    #[test]
    fn requires_colons_for_block_valued_route_items_with_repairs() {
        let source = "output Health { ok: Bool } route GET /health/{id} { path { id: Uuid } output: Health action { return Health { ok: true } } }";
        let parsed = parse(Path::new("route-blocks.jadpo"), source);
        let diagnostics = parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "ROUTE_ITEM_COLON_REQUIRED")
            .collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            diagnostics[0].recommended_next_step.edits[0].replacement,
            ":"
        );
        assert_eq!(
            diagnostics[1].recommended_next_step.edits[0].replacement,
            ":"
        );
    }

    #[test]
    fn required_queries_require_a_colon_separated_missing_failure() {
        let source = "entity Customer { id: Uuid } action find(id: Customer.id) -> Customer { return query required Customer { where: id == id missing CustomerNotFound } }";
        let parsed = parse(Path::new("required-query.jadpo"), source);

        assert!(parsed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "SYN_UNEXPECTED_TOKEN"
                && diagnostic.message == "Expected `:` after `missing`"
        }));
    }

    #[test]
    fn queries_require_a_colon_after_where() {
        let source = "entity Customer { id: Uuid } action find(id: Customer.id) -> Customer? { return query optional Customer { where id == id } }";
        let parsed = parse(Path::new("query-where.jadpo"), source);

        assert!(parsed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "SYN_UNEXPECTED_TOKEN"
                && diagnostic.message == "Expected `:` after `where`"
        }));
    }

    #[test]
    fn parses_typed_keyset_page_queries() {
        let source = r#"
entity Todo { owner_id: Uuid id: Uuid identity created_at: Instant deleted_at: Instant? status: Text }
input ListTodos { owner_id: Uuid status: Text optional after: TodoCursor optional page_size: Int }
value TodoCursor { created_at: Todo.created_at id: Todo.id }
output TodoView { id: Todo.id created_at: Todo.created_at status: Todo.status }
output TodoPage { items: List<TodoView> next: TodoCursor? }
action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: owner_id == input.owner_id
        and: deleted_at == none
        and: status matches optional input.status
        order_by: created_at desc, id desc
        after: optional input.after
        limit: input.page_size
        project: TodoView
        cursor: TodoCursor(created_at, id)
    }
}
"#;
        let parsed = parse(Path::new("keyset-page.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let action = parsed
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Callable(callable) if callable.name.text == "list" => Some(callable),
                _ => None,
            })
            .expect("page action");
        let Statement::Return(returned) = &action.body.statements[0] else {
            panic!("expected page return");
        };
        let crate::Expression::Attempt(attempt) = &returned.value else {
            panic!("expected query attempt");
        };
        let crate::Expression::Query(query) = attempt.value.as_ref() else {
            panic!("expected query expression");
        };
        let page = query.page.as_ref().expect("page query AST");
        assert_eq!(page.predicates.len(), 3);
        assert_eq!(page.order.len(), 2);
        assert_eq!(
            page.cursor_fields
                .iter()
                .map(|field| field.text.as_str())
                .collect::<Vec<_>>(),
            ["created_at", "id"]
        );
        assert_eq!(page.order[0].direction, QueryOrderDirection::Descending);
    }

    #[test]
    fn reports_required_keyset_page_clauses_and_predicate_operators() {
        let cases = [
            (
                "where: id == input.id\n        order_by: id desc\n        limit: 10\n        project: TodoView\n        cursor: TodoCursor(id)",
                "SYN_QUERY_PAGE_AFTER_REQUIRED",
            ),
            (
                "where: id == input.id\n        order_by: id desc\n        after: optional input.after\n        limit: 10\n        cursor: TodoCursor(id)",
                "SYN_QUERY_PAGE_PROJECT_REQUIRED",
            ),
            (
                "where: id == input.id\n        order_by: id desc\n        after: optional input.after\n        limit: 10\n        project: TodoView",
                "SYN_QUERY_PAGE_CURSOR_REQUIRED",
            ),
            (
                "where: id != input.id\n        order_by: id desc\n        after: optional input.after\n        limit: 10\n        project: TodoView\n        cursor: TodoCursor(id)",
                "SYN_QUERY_PAGE_PREDICATE_OPERATOR",
            ),
        ];

        for (clauses, expected_code) in cases {
            let source = format!(
                r#"
entity Todo {{ id: Uuid identity }}
input ListTodos {{ id: Uuid optional after: TodoCursor optional }}
value TodoCursor {{ id: Todo.id }}
output TodoView {{ id: Todo.id }}
output TodoPage {{ items: List<TodoView> next: TodoCursor? }}
action list(input: ListTodos) -> TodoPage {{
    return attempt query page Todo -> TodoPage {{
        {clauses}
    }}
}}
"#
            );
            let parsed = parse(Path::new("keyset-page-diagnostic.jadpo"), &source);
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected_code),
                "expected {expected_code}, received {:#?}",
                parsed.diagnostics
            );
        }
    }

    #[test]
    fn query_clauses_require_colon_separated_values() {
        let cases = [
            (
                "query many Customer { where: id == id order_by id asc }",
                "expected `:` after `order_by`",
            ),
            (
                "query many Customer { where: id == id order_by: id asc limit 2 offset: 0 }",
                "expected `:` after `limit`",
            ),
            (
                "query many Customer { where: id == id order_by: id asc limit: 2 offset 0 }",
                "expected `:` after `offset`",
            ),
            (
                "query required Customer { where: id == id include todos into: CustomerTodos order_by: id asc limit: 2 offset: 0 missing: Missing }",
                "expected `:` after `include`",
            ),
            (
                "query required Customer { where: id == id include: todos into CustomerTodos order_by: id asc limit: 2 offset: 0 missing: Missing }",
                "expected `:` after `into`",
            ),
        ];

        for (query, expected_syntax) in cases {
            let source = format!(
                "entity Customer {{ id: Uuid }} action find(id: Customer.id) -> Customer? {{ return {query} }}"
            );
            let parsed = parse(Path::new("query-clause.jadpo"), &source);
            assert!(
                parsed.diagnostics.iter().any(|diagnostic| diagnostic.code
                    == "SYN_UNEXPECTED_TOKEN"
                    && diagnostic.message == expected_syntax.replacen("expected", "Expected", 1)),
                "missing diagnostic for {expected_syntax:?} in {:#?}",
                parsed.diagnostics
            );
        }
    }

    #[test]
    fn mutations_require_colon_separated_items() {
        let source = "entity Customer { id: Uuid } failure Missing { kind: NotFound code: \"missing\" } failure Clashed { kind: Conflict code: \"clashed\" } action change(id: Customer.id) -> Customer fails Missing, Clashed { return update required Customer { where: id == id set { id: id } missing: Missing conflict: Clashed } }";
        let parsed = parse(Path::new("mutation.jadpo"), source);

        assert!(parsed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "SYN_UNEXPECTED_TOKEN"
                && diagnostic.message == "Expected `:` after `set`"
        }));
    }

    #[test]
    fn parses_entity_persistence_modifiers_after_the_field_type() {
        let source = "entity Account { id: Uuid identity email: Text unique owner: Text index }";
        let parsed = parse(Path::new("entity-modifiers.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Record(account) = &parsed.file.declarations[0] else {
            panic!("expected entity declaration");
        };
        assert_eq!(
            account.fields[0].persistence,
            vec![PersistenceModifier::Identity]
        );
        assert_eq!(
            account.fields[1].persistence,
            vec![PersistenceModifier::Unique]
        );
        assert_eq!(
            account.fields[2].persistence,
            vec![PersistenceModifier::Index]
        );
    }

    #[test]
    fn keeps_same_line_entity_identity_out_of_legacy_field_modifiers() {
        let source = "entity Customer { id: Uuid identity: id persistence { store: secondary role: authority } }";
        let parsed = parse(Path::new("inline.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Record(customer) = &parsed.file.declarations[0] else {
            panic!("expected entity declaration");
        };
        let dossier = customer.dossier.as_ref().expect("entity dossier");
        assert_eq!(dossier.identity.text, "id");
        assert_eq!(
            dossier
                .persistence
                .as_ref()
                .expect("persistence")
                .store
                .text,
            "secondary"
        );
    }

    #[test]
    fn parses_named_compound_constraints_and_precise_conflicts() {
        let source = "entity Membership { id: Uuid identity tenant: Text email: Text constraint tenant_email: unique(tenant, email) } failure Exists { kind: Conflict code: \"exists\" } action add(id: Membership.id, tenant: Membership.tenant, email: Membership.email) -> Membership fails Exists { return create Membership { id: id tenant: tenant email: email } conflict Membership.tenant_email: Exists }";
        let parsed = parse(Path::new("compound-constraint.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Record(entity) = &parsed.file.declarations[0] else {
            panic!("expected entity declaration");
        };
        assert_eq!(entity.persistence_constraints.len(), 1);
        assert_eq!(
            entity.persistence_constraints[0].kind,
            PersistenceConstraintKind::Unique
        );
        assert_eq!(
            entity.persistence_constraints[0]
                .fields
                .iter()
                .map(|field| field.text.as_str())
                .collect::<Vec<_>>(),
            vec!["tenant", "email"]
        );
        let Declaration::Callable(callable) = &parsed.file.declarations[2] else {
            panic!("expected action declaration");
        };
        let Statement::Return(statement) = &callable.body.statements[0] else {
            panic!("expected return statement");
        };
        let crate::Expression::Create(create) = &statement.value else {
            panic!("expected create expression");
        };
        assert_eq!(
            create.conflicts[0]
                .constraint
                .as_ref()
                .map(|constraint| constraint.path[1].text.as_str()),
            Some("tenant_email")
        );
    }

    #[test]
    fn parses_a_colon_separated_create_conflict_binding() {
        let source = "entity Account { id: Uuid identity } failure AccountConflict { kind: Conflict code: \"account_conflict\" } action add(id: Account.id) -> Account fails AccountConflict { return create Account { id: id } conflict: AccountConflict }";
        let parsed = parse(Path::new("create-conflict.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Callable(callable) = &parsed.file.declarations[2] else {
            panic!("expected action declaration");
        };
        let Statement::Return(statement) = &callable.body.statements[0] else {
            panic!("expected return statement");
        };
        let crate::Expression::Create(create) = &statement.value else {
            panic!("expected create expression");
        };
        assert_eq!(
            create
                .conflicts
                .first()
                .map(|binding| binding.rejection.failure.text.as_str()),
            Some("AccountConflict")
        );
    }

    #[test]
    fn parses_an_explicit_entity_reference_and_delete_lifecycle() {
        let source = "entity User { id: Uuid identity } entity Todo { id: Uuid identity owner_id: User.id references User.id as owner on_delete cascade }";
        let parsed = parse(Path::new("relationship.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Record(todo) = &parsed.file.declarations[1] else {
            panic!("expected todo entity");
        };
        let reference = todo.fields[1]
            .reference
            .as_ref()
            .expect("owner field should carry relationship metadata");
        assert_eq!(
            reference
                .target
                .path
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>(),
            vec!["User", "id"]
        );
        assert_eq!(
            reference
                .relationship
                .as_ref()
                .map(|name| name.text.as_str()),
            Some("owner")
        );
        assert_eq!(reference.on_delete, ReferenceDeleteAction::Cascade);
    }

    #[test]
    fn parses_entity_lifecycle_initial_visibility_transition_and_purge() {
        let source = r#"
enum UserStatus { active disabled }
entity User {
    id: Uuid identity
    identity: id
    status: UserStatus
    disabled_at: Instant?
    lifecycle {
        initial: { status: User.status(UserStatus.active) disabled_at: none }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: { status: User.status(UserStatus.disabled) disabled_at: clock.now }
        }
    }
}
entity Todo {
    id: Uuid identity
    identity: id
    deleted_at: Instant?
    lifecycle {
        initial: { deleted_at: none }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: { deleted_at: clock.now }
        }
        purge after config.soft_delete_retention from deleted_at
    }
}
"#;
        let parsed = parse(Path::new("lifecycle.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let Declaration::Record(user) = &parsed.file.declarations[1] else {
            panic!("expected User entity");
        };
        let lifecycle = &user.dossier.as_ref().unwrap().lifecycle;
        let lifecycle = lifecycle.as_ref().expect("User lifecycle should parse");
        assert_eq!(lifecycle.initial.as_ref().unwrap().len(), 2);
        assert!(lifecycle.visible.is_some());
        assert_eq!(lifecycle.transitions.len(), 1);
        assert_eq!(lifecycle.transitions[0].name.text, "disable");
        assert_eq!(lifecycle.transitions[0].set.len(), 2);

        let Declaration::Record(todo) = &parsed.file.declarations[2] else {
            panic!("expected Todo entity");
        };
        let lifecycle = todo.dossier.as_ref().unwrap().lifecycle.as_ref().unwrap();
        assert_eq!(lifecycle.transitions[0].name.text, "delete");
        let purge = lifecycle.purge.as_ref().expect("purge clause should parse");
        assert_eq!(purge.from.text, "deleted_at");
        assert_eq!(
            purge.after.range().start,
            source.find("config.soft_delete_retention").unwrap()
        );
    }

    #[test]
    fn reports_specific_lifecycle_declaration_errors() {
        let cases = [
            (
                "SYN_LIFECYCLE_DUPLICATE",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} } lifecycle { initial: {} } }",
            ),
            (
                "SYN_LIFECYCLE_INITIAL_DUPLICATE",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} initial: {} } }",
            ),
            (
                "SYN_LIFECYCLE_NON_ENTITY",
                "input TodoFilter { lifecycle { initial: {} } }",
            ),
            (
                "SYN_LIFECYCLE_PURGE_AFTER_REQUIRED",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} purge before config.retention from deleted_at } }",
            ),
            (
                "SYN_LIFECYCLE_PURGE_DUPLICATE",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} purge after config.retention from deleted_at purge after config.retention from deleted_at } }",
            ),
            (
                "SYN_LIFECYCLE_PURGE_FROM_REQUIRED",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} purge after config.retention using deleted_at } }",
            ),
            (
                "SYN_LIFECYCLE_SETTING_INVALID",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} unsupported: true } }",
            ),
            (
                "SYN_LIFECYCLE_TRANSITION_DUPLICATE",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} transition delete { from: true from: false set: {} } } }",
            ),
            (
                "SYN_LIFECYCLE_TRANSITION_FROM_REQUIRED",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} transition delete { set: {} } } }",
            ),
            (
                "SYN_LIFECYCLE_TRANSITION_SETTING_INVALID",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} transition delete { when: true from: true set: {} } } }",
            ),
            (
                "SYN_LIFECYCLE_TRANSITION_SET_REQUIRED",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} transition delete { from: true } } }",
            ),
            (
                "SYN_LIFECYCLE_VISIBLE_DUPLICATE",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} visible when true visible when false } }",
            ),
            (
                "SYN_LIFECYCLE_VISIBLE_WHEN_REQUIRED",
                "entity Todo { id: Uuid identity: id lifecycle { initial: {} visible if true } }",
            ),
        ];

        for (expected, source) in cases {
            let parsed = parse(Path::new("lifecycle-invalid.jadpo"), source);
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected),
                "expected {expected} for source {source:?}, got {:#?}",
                parsed.diagnostics
            );
        }
    }

    #[test]
    fn parses_transition_as_the_only_update_write_marker() {
        let source = r#"
action disable(id: User.id) -> Bool {
    var user = attempt update required User {
        where: id == id
        transition: disable
        missing: UserMissing
        conflict: UserConflict
    }
    return true
}
"#;
        let parsed = parse(Path::new("lifecycle-transition.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let action = parsed
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Callable(callable) if callable.name.text == "disable" => {
                    Some(callable)
                }
                _ => None,
            })
            .expect("disable action");
        let Statement::Binding(binding) = &action.body.statements[0] else {
            panic!("expected result binding");
        };
        let crate::Expression::Attempt(attempt) = &binding.value else {
            panic!("expected attempt expression");
        };
        let crate::Expression::Update(update) = attempt.value.as_ref() else {
            panic!("expected update expression");
        };
        assert_eq!(update.transition.as_ref().unwrap().text, "disable");
        assert!(update.changes.is_empty());
        assert!(update.patch.is_none());
    }

    #[test]
    fn parses_a_deterministically_ordered_many_query() {
        let source = "entity User { id: Uuid identity } entity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade } action list(user_id: User.id) -> List<Todo> { return query many Todo { where: owner_id == user_id order_by: id asc } }";
        let parsed = parse(Path::new("many-query.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Callable(callable) = &parsed.file.declarations[2] else {
            panic!("expected action declaration");
        };
        let Statement::Return(statement) = &callable.body.statements[0] else {
            panic!("expected return statement");
        };
        let crate::Expression::Query(query) = &statement.value else {
            panic!("expected query expression");
        };
        assert_eq!(query.cardinality, crate::QueryCardinality::Many);
        assert_eq!(query.order.as_ref().unwrap().field.text, "id");
    }

    #[test]
    fn parses_an_inverse_and_explicit_batched_include() {
        let source = r#"
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse notes: many Note via Note.owner_id
}
entity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
entity Note { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
output UserActivity { parent: User todos: List<Todo> notes: List<Note> }
failure UserNotFound { kind: NotFound code: "user_not_found" }
action load(user_id: User.id) -> UserActivity fails UserNotFound {
    return query required User {
        where: id == user_id
        include: todos into: UserActivity order_by: id asc limit: 100 offset: 0
        include: notes into: UserActivity order_by: id desc limit: 50 offset: 10
        missing: UserNotFound
    }
}
"#;
        let parsed = parse(Path::new("inverse-query.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Record(user) = &parsed.file.declarations[0] else {
            panic!("expected user entity");
        };
        assert_eq!(user.inverses[0].name.text, "todos");
        assert_eq!(user.inverses[0].target.text, "Todo");
        let Declaration::Callable(callable) = &parsed.file.declarations[5] else {
            panic!("expected load action");
        };
        let Statement::Return(statement) = &callable.body.statements[0] else {
            panic!("expected return statement");
        };
        let crate::Expression::Query(query) = &statement.value else {
            panic!("expected query expression");
        };
        let include = query.includes.first().expect("include should be parsed");
        assert_eq!(include.relationship.text, "todos");
        assert_eq!(include.result.path[0].text, "UserActivity");
        assert_eq!(include.order.field.text, "id");
        let crate::Expression::Literal(limit) = include.pagination.limit.as_ref() else {
            panic!("expected literal limit");
        };
        assert_eq!(limit.text, "100");
        assert_eq!(query.includes.len(), 2);
        assert_eq!(query.includes[1].relationship.text, "notes");
        assert_eq!(
            query.includes[1].order.direction,
            QueryOrderDirection::Descending
        );
    }

    #[test]
    fn parses_explicit_local_reassignment() {
        let source = r#"
type Choice = Text {}
function choose(initial: Choice, replacement: Choice) -> Choice {
    var mut selected: Choice = initial
    selected = replacement
    return selected
}
"#;
        let parsed = parse(Path::new("assignment.jadpo"), source);

        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Callable(callable) = &parsed.file.declarations[1] else {
            panic!("expected callable declaration");
        };
        let Statement::Assignment(assignment) = &callable.body.statements[1] else {
            panic!("expected assignment statement");
        };
        assert_eq!(assignment.target.text, "selected");
        assert!(matches!(assignment.value, crate::Expression::Name(_)));
    }

    #[test]
    fn parses_passing_fixtures_and_preserves_selected_parser_diagnostics() {
        let root = repository_root().join("tests/compile");
        let mut paths = Vec::new();
        collect_sources(&root, &mut paths);
        paths.sort();

        for path in paths {
            let source = fs::read_to_string(&path).expect("fixture should be readable");
            let parsed = parse(&path, &source);
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");

            if file_name == "10_arbitrary_exception.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "SYN_UNSUPPORTED_THROW");
                let Declaration::Callable(callable) = &parsed.file.declarations[0] else {
                    panic!("expected function declaration");
                };
                assert!(matches!(
                    callable.body.statements[0],
                    Statement::Unsupported(_)
                ));
            } else if file_name == "59_route_path_binding_mismatch.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 2, "{:#?}", parsed.diagnostics);
                assert_eq!(
                    parsed
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.code)
                        .collect::<Vec<_>>(),
                    vec!["ROUTE_PATH_BINDING_MISSING", "ROUTE_PATH_BINDING_EXTRA"]
                );
            } else if file_name == "62_route_behaviour_conflict.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "ROUTE_BEHAVIOUR_CONFLICT");
            } else if file_name == "67_route_path_modifier_invalid.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(
                    parsed.diagnostics[0].code,
                    "ROUTE_PATH_FIELD_MODIFIER_INVALID"
                );
            } else if file_name == "69_duplicate_route_item.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "ROUTE_ITEM_DUPLICATE");
            } else if file_name == "70_route_block_colon_required.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 2, "{:#?}", parsed.diagnostics);
                assert!(parsed
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code == "ROUTE_ITEM_COLON_REQUIRED"));
            } else if file_name == "71_invalid_route_auth_value.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "ROUTE_AUTH_VALUE_INVALID");
            } else if file_name == "72_top_level_route_item.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "SYN_EXPECTED_DECLARATION");
                assert_eq!(
                    parsed.diagnostics[0].message,
                    "`path` cannot start a top-level declaration"
                );
            } else if file_name == "73_route_scalar_colon_required.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 4, "{:#?}", parsed.diagnostics);
                assert!(parsed
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code == "ROUTE_ITEM_COLON_REQUIRED"));
            } else if file_name == "74_record_field_colon_required.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "SYN_UNEXPECTED_TOKEN");
                assert_eq!(
                    parsed.diagnostics[0].message,
                    "Expected `:` after field name"
                );
            } else if file_name == "75_invalid_string_escape.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "SYN_INVALID_ESCAPE");
            } else if file_name == "76_unexpected_character.jadpo" {
                assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
                assert_eq!(parsed.diagnostics[0].code, "SYN_UNEXPECTED_CHARACTER");
            } else if path
                .components()
                .any(|component| component.as_os_str() == "pass")
            {
                assert!(
                    parsed.diagnostics.is_empty(),
                    "{}: {:#?}",
                    path.display(),
                    parsed.diagnostics
                );
            }
        }
    }

    #[test]
    fn recovers_and_reports_multiple_syntax_errors() {
        let source = r#"
type Email = Text {
    max_length
}

value Result {
    ok Bool
}
"#;
        let parsed = parse(Path::new("malformed.jadpo"), source);

        assert!(parsed.diagnostics.len() >= 2, "{:#?}", parsed.diagnostics);
        assert_eq!(parsed.file.declarations.len(), 2);
    }

    #[test]
    fn reports_each_specific_parser_expectation() {
        let cases = [
            ("input Bad { constraint named: unique(value) }", "SYN_CONSTRAINT_NON_ENTITY"),
            ("entity Bad { id: Uuid identity identity }", "SYN_DUPLICATE_FIELD_MODIFIER"),
            ("module one\nimport two { }\ntype Name = Text {}", "SYN_EMPTY_IMPORT"),
            ("enum Empty {}", "SYN_ENUM_EMPTY"),
            ("type Name = Text { unknown }", "SYN_EXPECTED_CONSTRAINT"),
            ("type Name = Text { max_length }", "SYN_EXPECTED_CONSTRAINT_VALUE"),
            ("entity User { id: Uuid identity } entity Bad { owner: User.id references User.id on_delete unknown }", "SYN_EXPECTED_DELETE_ACTION"),
            ("function bad() -> Bool { return }", "SYN_EXPECTED_EXPRESSION"),
            ("failure Bad { unknown }", "SYN_EXPECTED_FAILURE_ITEM"),
            ("output Result { ok: Bool } function bad() -> Result { return Result { 123 } }", "SYN_EXPECTED_FIELD_INITIALISER"),
            ("route UNKNOWN /bad { auth: none action: { return true } }", "SYN_EXPECTED_HTTP_METHOD"),
            ("entity User { id: Uuid identity inverse items: wrong Item via Item.user_id } entity Item { id: Uuid identity user_id: User.id references User.id on_delete cascade }", "SYN_EXPECTED_INVERSE_CARDINALITY"),
            ("route GET /bad { auth: none run: true }", "SYN_EXPECTED_INVOCATION"),
            ("test Bad { }", "SYN_EXPECTED_LITERAL"),
            ("type Name = Other.123 {}", "SYN_EXPECTED_NAME"),
            ("entity Item { id: Uuid identity } action list() -> List<Item> { return query many Item { where: id == id order_by: id sideways } }", "SYN_EXPECTED_ORDER_DIRECTION"),
            ("route GET /bad { auth: none unknown: true }", "SYN_EXPECTED_ROUTE_ITEM"),
            ("function bad() -> Bool { type }", "SYN_EXPECTED_STATEMENT"),
            ("failure MissingKind { code: \"missing_kind\" }", "SYN_FAILURE_KIND_REQUIRED"),
            ("failure MissingCode { kind: NotFound }", "SYN_FAILURE_CODE_REQUIRED"),
            ("import shared.names { Name }\ntype Local = Text {}", "SYN_IMPORT_REQUIRES_MODULE"),
            ("input Bad { inverse items: many Item via Item.bad }", "SYN_INVERSE_NON_ENTITY"),
            ("enum Choice { Yes No } function bad(value: Choice) -> Bool { match value { if => {} } return true }", "SYN_MATCH_PATTERN"),
            ("public route GET /bad { auth: none action: { return true } }", "SYN_ROUTE_EXPORT_INVALID"),
            ("entity Item { id: Uuid identity } input PatchItem { id: Item.id optional } failure Empty { kind: InvalidValue code: \"empty\" } failure Missing { kind: NotFound code: \"missing\" } action bad(id: Item.id, input: PatchItem) -> Item fails Empty, Missing { return update required Item { where: id == id patch: 123 empty: Empty missing: Missing } }", "SYN_EXPECTED_PATCH_INPUT"),
            ("entity Item { id: Uuid identity } input PatchItem { id: Item.id optional } failure Empty { kind: InvalidValue code: \"empty\" } failure Missing { kind: NotFound code: \"missing\" } action bad(id: Item.id, input: PatchItem) -> Item fails Empty, Missing { return update required Item { where: id == id patch: input empty: Empty set: { id: id when 123 } missing: Missing } }", "SYN_EXPECTED_SUPPLIED_FIELD"),
            ("entity Item { id: Uuid identity } input PatchItem { id: Item.id optional } failure Empty { kind: InvalidValue code: \"empty\" } failure Missing { kind: NotFound code: \"missing\" } action bad(id: Item.id, input: PatchItem) -> Item fails Empty, Missing { return update required Item { where: id == id patch: input empty: Empty set: { id: id when input.id wrong } missing: Missing } }", "SYN_EXPECTED_SUPPLIED"),
            ("entity Item { id: Uuid identity } failure Missing { kind: NotFound code: \"missing\" } action bad(id: Item.id) -> Item fails Missing { return update required Item { where: id == id missing: Missing } }", "SYN_EXPECTED_UPDATE_BODY"),
            ("entity Item { id: Uuid identity } failure Missing { kind: NotFound code: \"missing\" } action bad(id: Item.id) -> Item fails Missing { return delete required Item { where: id == id missing: Missing } }", "SYN_MUTATION_CONFLICT_REQUIRED"),
        ];

        for (source, expected) in cases {
            let parsed = parse(Path::new("specific-parser-error.jadpo"), source);
            let codes = parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>();
            assert!(
                codes.contains(&expected),
                "{expected}: {codes:#?}\n{source}"
            );
        }
    }

    #[test]
    fn recovers_an_object_keyword_typo_without_cascading() {
        let source = r#"type foo = Okbject {
    foo: Text {
        max_length: 100
    }
}"#;
        let parsed = parse(Path::new("object-keyword-typo.jadpo"), source);

        assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
        assert_eq!(parsed.diagnostics[0].code, "SEM_UNKNOWN_NAME");
        assert_eq!(
            parsed.diagnostics[0].recommended_next_step.title,
            "Replace `Okbject` with `Object`"
        );
        assert_eq!(
            parsed.diagnostics[0].recommended_next_step.edits[0].replacement,
            "Object"
        );
        assert_eq!(parsed.file.declarations.len(), 1);
        let Declaration::Record(declaration) = &parsed.file.declarations[0] else {
            panic!("object-shaped recovery should preserve the record body");
        };
        assert_eq!(declaration.fields.len(), 1);
        assert_eq!(declaration.fields[0].name.text, "foo");
    }

    #[test]
    fn requires_colons_after_every_type_constraint_name() {
        let source = r#"type Legacy = Text {
    min 1
    max 2
    min_length 1
    max_length 3
    pattern "[a-z]+"
    format email
}"#;
        let parsed = parse(Path::new("legacy-constraints.jadpo"), source);

        assert_eq!(parsed.diagnostics.len(), 6, "{:#?}", parsed.diagnostics);
        assert!(parsed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == "SYN_CONSTRAINT_COLON_REQUIRED"));
        assert!(parsed.diagnostics.iter().all(|diagnostic| {
            diagnostic.recommended_next_step.edits.len() == 1
                && diagnostic.recommended_next_step.edits[0].replacement == ":"
        }));
    }

    #[test]
    fn requires_colons_after_failure_member_names() {
        let source = r#"failure Missing {
    kind NotFound
    code "missing"
    message "Missing."
}"#;
        let parsed = parse(Path::new("legacy-failure-members.jadpo"), source);

        assert_eq!(parsed.diagnostics.len(), 3, "{:#?}", parsed.diagnostics);
        assert!(parsed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == "SYN_FAILURE_ITEM_COLON_REQUIRED"));
        assert!(parsed.diagnostics.iter().all(|diagnostic| {
            diagnostic.recommended_next_step.edits.len() == 1
                && diagnostic.recommended_next_step.edits[0].replacement == ":"
        }));
    }

    #[test]
    fn names_punctuation_instead_of_calling_it_this_token() {
        let parsed = parse(Path::new("punctuation.jadpo"), "input User { name. Text }");
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "SYN_UNEXPECTED_TOKEN")
            .expect("the dot should be rejected");

        assert!(diagnostic.reason.contains("Found `.`"), "{diagnostic:#?}");
        assert!(!diagnostic.reason.contains("this token"));
    }

    #[test]
    fn parses_structured_configuration_fields() {
        let source = r#"config AppConfiguration {
    mailer_api_key: Text {
        binding: "MAILER_API_KEY"
        secret: true
    }
    request_timeout: Duration {
        binding: "REQUEST_TIMEOUT"
        default: 5s
    }
}"#;
        let parsed = parse(Path::new("configuration.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Config(configuration) = &parsed.file.declarations[0] else {
            panic!("expected configuration declaration");
        };
        assert_eq!(configuration.fields.len(), 2);
        assert!(configuration.fields[0].secret);
        assert_eq!(
            configuration.fields[1]
                .default
                .as_ref()
                .expect("duration default")
                .text,
            "5s"
        );
    }

    #[test]
    fn parses_none_default_on_nullable_input_fields_only() {
        let source = "input CreateTodo { due_at: Instant? default none }";
        let parsed = parse(Path::new("input-default.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::Record(input) = &parsed.file.declarations[0] else {
            panic!("expected input declaration");
        };
        assert_eq!(input.kind, RecordKind::Input);
        assert_eq!(input.fields[0].default.as_ref().unwrap().text, "none");
        assert!(!input.fields[0].optional);

        let invalid = parse(
            Path::new("entity-default.jadpo"),
            "entity Todo { id: Uuid default none }",
        );
        assert!(!invalid.diagnostics.is_empty());
    }

    #[test]
    fn parses_application_authentication_and_closed_principal() {
        let source = r#"application TodoApplication {
    authentication {
        principal: Principal
        revocation {
            mode: bounded
            maximum_delay: 5m
        }
    }
}
principal Principal {
    user { subject: Text }
    service { subject: Text }
}"#;
        let parsed = parse(Path::new("authentication.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let Declaration::Application(application) = &parsed.file.declarations[0] else {
            panic!("expected application declaration");
        };
        assert_eq!(application.name.text, "TodoApplication");
        assert_eq!(
            application.authentication.principal.path[0].text,
            "Principal"
        );
        assert_eq!(
            application
                .authentication
                .revocation
                .maximum_delay
                .as_ref()
                .expect("bounded delay")
                .text,
            "5m"
        );

        let Declaration::Principal(principal) = &parsed.file.declarations[1] else {
            panic!("expected principal declaration");
        };
        assert_eq!(principal.variants.len(), 2);
        assert_eq!(principal.variants[0].kind, PrincipalVariantKind::User);
        assert_eq!(principal.variants[1].kind, PrincipalVariantKind::Service);
    }

    #[test]
    fn parses_authentication_strategy_slots_and_named_validators() {
        let source = r#"authentication api_bearer {
    transport { bearer: authorization_header }
    validators {
        opaque_user { mode: opaque principal: user }
        service_key { mode: api_key principal: service }
        service_signed { mode: signed principal: service }
    }
    exchange { path: "/auth/exchange" key: service_key signed: service_signed }
}"#;
        let parsed = parse(Path::new("authentication-strategy.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Declaration::AuthenticationStrategy(strategy) = &parsed.file.declarations[0] else {
            panic!("expected authentication strategy declaration");
        };
        assert_eq!(strategy.name.text, "api_bearer");
        assert!(matches!(
            &strategy.transport.location,
            CredentialLocation::Bearer(location) if location.text == "authorization_header"
        ));
        assert_eq!(strategy.validators.len(), 3);
        assert_eq!(strategy.validators[0].mode.text, "opaque");
        assert_eq!(strategy.validators[1].principal.text, "service");
        let exchange = strategy.exchange.as_ref().expect("exchange declaration");
        assert_eq!(exchange.path.text, "\"/auth/exchange\"");
        assert_eq!(exchange.key.text, "service_key");
        assert_eq!(exchange.signed.text, "service_signed");
    }

    #[test]
    fn rejects_invalid_configuration_field_options() {
        let duplicate_and_unknown = parse(
            Path::new("bad-options.jadpo"),
            r#"config App {
    value: Text {
        binding: "FIRST"
        binding: "SECOND"
        mystery: "value"
    }
}"#,
        );
        let codes = duplicate_and_unknown
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"CONFIG_DUPLICATE_OPTION"), "{codes:#?}");
        assert!(codes.contains(&"CONFIG_UNKNOWN_OPTION"), "{codes:#?}");

        let missing_default = parse(
            Path::new("bad-default.jadpo"),
            "config App { value: Text { binding: \"VALUE\" default: } }",
        );
        assert!(missing_default
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CONFIG_DEFAULT_LITERAL_REQUIRED"));

        let missing_field = parse(Path::new("bad-field.jadpo"), "config App { 123 }");
        assert!(missing_field
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CONFIG_EXPECTED_FIELD"));
    }

    fn collect_sources(path: &Path, output: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("fixture directory should be readable") {
            let entry = entry.expect("fixture entry should be readable");
            let entry_path = entry.path();
            if entry_path.is_dir() {
                collect_sources(&entry_path, output);
            } else if entry_path
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("jadpo")
            {
                output.push(entry_path);
            }
        }
    }

    fn declaration_outline(declaration: &Declaration) -> String {
        match declaration {
            Declaration::Application(declaration) => {
                format!("application {}", declaration.name.text)
            }
            Declaration::Locales(_) => "locales".to_owned(),
            Declaration::AuthenticationStrategy(declaration) => {
                format!("authentication {}", declaration.name.text)
            }
            Declaration::Principal(declaration) => {
                format!("principal {}", declaration.name.text)
            }
            Declaration::Config(declaration) => format!("config {}", declaration.name.text),
            Declaration::Type(declaration) => format!("type {}", declaration.name.text),
            Declaration::Enum(declaration) => format!("enum {}", declaration.name.text),
            Declaration::Record(declaration) => {
                let keyword = match declaration.kind {
                    RecordKind::Entity => "entity",
                    RecordKind::Value => "value",
                    RecordKind::Input => "input",
                    RecordKind::Output => "output",
                };
                format!("{keyword} {}", declaration.name.text)
            }
            Declaration::Failure(declaration) => {
                format!("failure {}", declaration.name.text)
            }
            Declaration::Callable(declaration) => {
                let keyword = match declaration.kind {
                    CallableKind::Function => "function",
                    CallableKind::Action => "action",
                    CallableKind::Query => "query",
                };
                format!("{keyword} {}", declaration.name.text)
            }
            Declaration::Fixture(declaration) => {
                format!("fixture {}", declaration.name.text)
            }
            Declaration::Test(declaration) => format!("test {}", declaration.name.text),
            Declaration::Job(declaration) => format!("job {}", declaration.name.text),
            Declaration::Route(declaration) => {
                let method = match declaration.method {
                    HttpMethod::Get => "GET",
                    HttpMethod::Post => "POST",
                    HttpMethod::Put => "PUT",
                    HttpMethod::Patch => "PATCH",
                    HttpMethod::Delete => "DELETE",
                };
                format!("route {method} {}", declaration.path)
            }
        }
    }
}
