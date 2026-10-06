use crate::{TextRange, Token};
use jadpo_diagnostics::Diagnostic;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedSyntax {
    pub source_name: String,
    pub source_text: String,
    pub file: SyntaxFile,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxFile {
    pub module: Option<ModuleDeclaration>,
    pub imports: Vec<ImportDeclaration>,
    pub exports: Vec<Name>,
    pub persistence: Vec<PersistenceDeclaration>,
    pub declarations: Vec<Declaration>,
    /// Parsed top-level service contracts. Kept separate while their public
    /// grammar is introduced in a deliberately narrow, checked slice.
    pub services: Vec<ServiceDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceDeclaration {
    pub name: Name,
    pub items: Vec<ServiceItem>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceItem {
    /// Names of enclosing service sections, such as `import` or `credential`.
    pub path: Vec<String>,
    pub key: String,
    pub value: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistenceDeclaration {
    pub target: Name,
    pub identities: Vec<Name>,
    pub uniques: Vec<Name>,
    pub indexes: Vec<Name>,
    pub constraints: Vec<PersistenceConstraintDeclaration>,
    pub references: Vec<PersistenceReferenceDeclaration>,
    pub inverses: Vec<InverseDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistenceReferenceDeclaration {
    pub field: Name,
    pub reference: ReferenceDeclaration,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleDeclaration {
    pub path: Vec<Name>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportDeclaration {
    pub module: Vec<Name>,
    pub names: Vec<Name>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Declaration {
    Application(ApplicationDeclaration),
    Locales(LocalesDeclaration),
    AuthenticationStrategy(AuthenticationStrategyDeclaration),
    Principal(PrincipalDeclaration),
    Config(ConfigDeclaration),
    Type(TypeDeclaration),
    Enum(EnumDeclaration),
    Record(RecordDeclaration),
    Failure(FailureDeclaration),
    Callable(CallableDeclaration),
    Fixture(FixtureDeclaration),
    Test(TestDeclaration),
    Route(RouteDeclaration),
    Job(JobDeclaration),
}

impl Declaration {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Application(declaration) => declaration.range,
            Self::Locales(declaration) => declaration.range,
            Self::AuthenticationStrategy(declaration) => declaration.range,
            Self::Principal(declaration) => declaration.range,
            Self::Config(declaration) => declaration.range,
            Self::Type(declaration) => declaration.range,
            Self::Enum(declaration) => declaration.range,
            Self::Record(declaration) => declaration.range,
            Self::Failure(declaration) => declaration.range,
            Self::Callable(declaration) => declaration.range,
            Self::Fixture(declaration) => declaration.range,
            Self::Test(declaration) => declaration.range,
            Self::Route(declaration) => declaration.range,
            Self::Job(declaration) => declaration.range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalesDeclaration {
    pub default: Literal,
    pub supported: Vec<Literal>,
    pub unsupported: LocaleUnsupported,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocaleUnsupported {
    FallbackToDefault,
    Reject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationStrategyDeclaration {
    pub name: Name,
    pub transport: AuthenticationTransport,
    pub exchange: Option<AuthenticationExchangeDeclaration>,
    pub validators: Vec<AuthenticationValidatorDeclaration>,
    pub claims: Vec<AuthenticationMappingDeclaration>,
    pub resolutions: Vec<AuthenticationResolutionDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationExchangeDeclaration {
    pub path: Literal,
    pub key: Name,
    pub signed: Name,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationValidatorDeclaration {
    pub name: Name,
    pub mode: Name,
    pub principal: Name,
    pub settings: Vec<FieldInitialiser>,
    pub credentials: Option<AuthenticationCredentialBinding>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationCredentialBinding {
    pub identity: NameExpression,
    pub principal: NameExpression,
    pub verifier: NameExpression,
    pub active: Expression,
    pub expires: NameExpression,
    pub revoked: NameExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationMappingDeclaration {
    pub source: Name,
    pub target: NameExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationResolutionDeclaration {
    pub principal: Name,
    pub authority: NameExpression,
    pub active: Expression,
    pub mappings: Vec<AuthenticationMappingDeclaration>,
    pub inactive: Name,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationTransport {
    pub location: CredentialLocation,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialLocation {
    Cookie(Literal),
    Bearer(Name),
}

impl CredentialLocation {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Cookie(literal) => literal.range,
            Self::Bearer(name) => name.range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationDeclaration {
    pub name: Name,
    pub authentication: ApplicationAuthentication,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationAuthentication {
    pub principal: TypeReference,
    pub revocation: RevocationDeclaration,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevocationDeclaration {
    pub mode: RevocationMode,
    pub maximum_delay: Option<ConfigDefault>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevocationMode {
    Immediate,
    Bounded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalDeclaration {
    pub name: Name,
    pub variants: Vec<PrincipalVariantDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrincipalVariantKind {
    User,
    Service,
}

impl PrincipalVariantKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Service => "service",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalVariantDeclaration {
    pub kind: PrincipalVariantKind,
    pub name: Name,
    pub fields: Vec<FieldDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigDeclaration {
    pub name: Name,
    pub fields: Vec<ConfigFieldDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigFieldDeclaration {
    pub name: Name,
    pub field_type: TypeReference,
    pub binding: Option<Literal>,
    pub secret: bool,
    pub default: Option<ConfigDefault>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigDefaultKind {
    String,
    Integer,
    Decimal,
    Boolean,
    Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigDefault {
    pub kind: ConfigDefaultKind,
    pub text: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurationLiteral {
    pub text: String,
    pub range: TextRange,
}

impl DurationLiteral {
    pub fn milliseconds(&self) -> Option<u64> {
        const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
        let (number, multiplier) = if let Some(number) = self.text.strip_suffix("ms") {
            (number, 1_u64)
        } else if let Some(number) = self.text.strip_suffix('s') {
            (number, 1_000)
        } else if let Some(number) = self.text.strip_suffix('m') {
            (number, 60_000)
        } else if let Some(number) = self.text.strip_suffix('h') {
            (number, 3_600_000)
        } else {
            return None;
        };
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        if whole.is_empty()
            || !whole.bytes().all(|digit| digit.is_ascii_digit())
            || !fraction.bytes().all(|digit| digit.is_ascii_digit())
            || (number.contains('.') && fraction.is_empty())
        {
            return None;
        }
        // Multiply the authored decimal coefficient exactly, least-significant
        // digit first. Carry stays below the bounded unit multiplier, so even
        // very long spellings cannot overflow or lose fractional information.
        let mut scaled_digits = Vec::with_capacity(number.len() + 7);
        let mut carry = 0_u64;
        for digit in whole.bytes().chain(fraction.bytes()).rev() {
            carry += u64::from(digit - b'0') * multiplier;
            scaled_digits.push((carry % 10) as u8);
            carry /= 10;
        }
        while carry > 0 {
            scaled_digits.push((carry % 10) as u8);
            carry /= 10;
        }
        if scaled_digits
            .iter()
            .take(fraction.len())
            .any(|digit| *digit != 0)
        {
            return None;
        }
        let milliseconds = scaled_digits
            .iter()
            .skip(fraction.len())
            .rev()
            .try_fold(0_u64, |value, digit| {
                value.checked_mul(10)?.checked_add(u64::from(*digit))
            })?;
        (milliseconds > 0 && milliseconds <= MAX_SAFE_INTEGER).then_some(milliseconds)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumDeclaration {
    pub name: Name,
    pub variants: Vec<EnumVariantDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumVariantDeclaration {
    pub name: Name,
    pub fields: Vec<FieldDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name {
    pub text: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeDeclaration {
    pub name: Name,
    pub parent: TypeReference,
    pub constraints: Vec<Constraint>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordKind {
    Entity,
    Value,
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordDeclaration {
    pub kind: RecordKind,
    pub name: Name,
    pub fields: Vec<FieldDeclaration>,
    pub inverses: Vec<InverseDeclaration>,
    pub persistence_constraints: Vec<PersistenceConstraintDeclaration>,
    pub dossier: Option<EntityDossier>,
    pub membership: Option<MembershipDeclaration>,
    pub policy: Option<PolicyDeclaration>,
    pub range: TextRange,
}

impl RecordDeclaration {
    pub fn is_persistent_entity(&self) -> bool {
        self.kind == RecordKind::Entity
            && self
                .dossier
                .as_ref()
                .map_or(true, |dossier| dossier.persistence.is_some())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityDossier {
    pub identity: Name,
    pub persistence: Option<EntityPersistence>,
    pub representations: Vec<DerivedRepresentation>,
    pub lifecycle: Option<EntityLifecycle>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityLifecycle {
    pub initial: Option<Vec<FieldInitialiser>>,
    pub visible: Option<Expression>,
    pub transitions: Vec<LifecycleTransition>,
    pub purge: Option<LifecyclePurge>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleTransition {
    pub name: Name,
    pub from: Expression,
    pub set: Vec<FieldInitialiser>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecyclePurge {
    pub after: Expression,
    pub from: Name,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityPersistence {
    pub store: Name,
    pub role: PersistenceRole,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceRole {
    Authority,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedRepresentation {
    pub kind: DerivedRepresentationKind,
    pub name: Name,
    pub store: Name,
    pub from: Name,
    pub strategy: Option<Name>,
    pub delivery: DeliveryMode,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DerivedRepresentationKind {
    Cache,
    Projection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryMode {
    Durable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceConstraintKind {
    Unique,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistenceConstraintDeclaration {
    pub name: Name,
    pub kind: PersistenceConstraintKind,
    pub fields: Vec<Name>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InverseDeclaration {
    pub name: Name,
    pub cardinality: InverseCardinality,
    pub target: Name,
    pub via: TypeReference,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InverseCardinality {
    Many,
    Optional,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDeclaration {
    pub name: Name,
    pub field_type: TypeReference,
    pub constraints: Vec<Constraint>,
    pub default: Option<Literal>,
    pub persistence: Vec<PersistenceModifier>,
    pub generated: Option<GeneratedFieldRole>,
    pub reference: Option<ReferenceDeclaration>,
    pub role: Option<RoleBinding>,
    pub immutable: bool,
    pub policy: Option<PolicyDeclaration>,
    pub optional: bool,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleBinding {
    pub role: NameExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MembershipDeclaration {
    pub scope: Name,
    pub member: Name,
    pub role: Name,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyDeclaration {
    pub scope: Option<Name>,
    pub rules: Vec<PolicyRule>,
    pub operations: Vec<PolicyOperation>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyOperation {
    pub name: Name,
    pub rules: Vec<PolicyRule>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRule {
    pub subject: NameExpression,
    pub effects: Vec<PolicyEffect>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PolicyEffect {
    Create,
    Read,
    Update,
    Delete,
    Invoke,
}

impl PolicyEffect {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Read => "read",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Invoke => "invoke",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedFieldRole {
    Identity,
    Create,
    CreateOrChange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceModifier {
    Identity,
    Unique,
    Index,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceDeclaration {
    pub target: TypeReference,
    pub relationship: Option<Name>,
    pub on_delete: ReferenceDeleteAction,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceDeleteAction {
    Restrict,
    Cascade,
    SetNull,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeReference {
    pub path: Vec<Name>,
    pub arguments: Vec<TypeReference>,
    pub nullable: bool,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintKind {
    Min,
    Max,
    MinLength,
    MaxLength,
    Pattern,
    Format,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constraint {
    pub kind: ConstraintKind,
    pub value: Literal,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureDeclaration {
    pub name: Name,
    pub kind: Name,
    pub code: Option<Literal>,
    pub message: Option<Literal>,
    pub public_fields: Vec<FieldDeclaration>,
    pub internal_fields: Vec<FieldDeclaration>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableKind {
    Function,
    Action,
    Query,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiverKind {
    Reference,
    Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsistencyDisposition {
    Atomic,
    DurableWorkflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryFreshness {
    Authoritative,
    ReadYourWrites,
    BoundedStaleness,
    Eventual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationGuard {
    Reload,
    Revision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableDeclaration {
    pub kind: CallableKind,
    pub name: Name,
    pub owner: Option<Name>,
    pub receiver: Option<ReceiverKind>,
    pub consistency: Option<ConsistencyDisposition>,
    pub freshness: Option<QueryFreshness>,
    pub mutation_guard: Option<MutationGuard>,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeReference,
    pub return_annotation_range: TextRange,
    pub failures: Vec<Name>,
    pub failures_range: Option<TextRange>,
    pub policy: Option<PolicyDeclaration>,
    pub body: Block,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestDeclaration {
    pub name: Literal,
    pub fixture: Option<Name>,
    pub body: Block,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureDeclaration {
    pub name: Name,
    pub clock: Option<Expression>,
    pub configuration: Option<Vec<FixtureConfigValue>>,
    pub service_fakes: Vec<FixtureServiceFake>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureConfigValue {
    pub name: Name,
    pub value: Expression,
    pub secret: bool,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureServiceFake {
    pub service: Name,
    pub outcomes: Vec<FixtureServiceFakeOutcome>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureServiceFakeOutcome {
    pub operation: Name,
    pub value: FixtureServiceFakeValue,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixtureServiceFakeValue {
    Accepted(Expression),
    Declared(NameExpression),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub name: Name,
    pub parameter_type: TypeReference,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    Binding(BindingStatement),
    Assignment(AssignmentStatement),
    Return(ReturnStatement),
    Reject(RejectStatement),
    If(IfStatement),
    Match(MatchStatement),
    Assert(AssertStatement),
    AdvanceClock(AdvanceClockStatement),
    Unsupported(UnsupportedStatement),
}

impl Statement {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Binding(statement) => statement.range,
            Self::Assignment(statement) => statement.range,
            Self::Return(statement) => statement.range,
            Self::Reject(statement) => statement.range,
            Self::If(statement) => statement.range,
            Self::Match(statement) => statement.range,
            Self::Assert(statement) => statement.range,
            Self::AdvanceClock(statement) => statement.range,
            Self::Unsupported(statement) => statement.range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchStatement {
    pub subject: Expression,
    pub arms: Vec<MatchArm>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssertStatement {
    pub condition: Expression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvanceClockStatement {
    pub duration: Expression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Block,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchPattern {
    Name(NameExpression),
    Variant(VariantPattern),
    OptionalSome(OptionalSomePattern),
    Literal(Literal),
    Wildcard(Name),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantPattern {
    pub target: NameExpression,
    pub bindings: Vec<Name>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalSomePattern {
    pub binding: Name,
    pub range: TextRange,
}

impl MatchPattern {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Name(pattern) => pattern.range,
            Self::Variant(pattern) => pattern.range,
            Self::OptionalSome(pattern) => pattern.range,
            Self::Literal(pattern) => pattern.range,
            Self::Wildcard(pattern) => pattern.range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentStatement {
    pub target: Name,
    pub value: Expression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingStatement {
    pub mutable: bool,
    pub name: Name,
    pub annotation: Option<TypeReference>,
    pub value: Expression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReturnStatement {
    pub value: Expression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectStatement {
    pub failure: Name,
    pub values: Vec<FieldInitialiser>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_block: Block,
    pub else_block: Option<Block>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedStatement {
    pub keyword: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expression {
    Literal(Literal),
    Name(NameExpression),
    Invocation(InvocationExpression),
    TestCall(TestCallExpression),
    Object(ObjectExpression),
    Construction(ConstructionExpression),
    Create(CreateExpression),
    Query(QueryExpression),
    Update(UpdateExpression),
    Delete(DeleteExpression),
    Attempt(AttemptExpression),
    OutcomeMatch(OutcomeMatchExpression),
    Unary(UnaryExpression),
    Binary(BinaryExpression),
    Grouped(GroupedExpression),
    Missing(TextRange),
}

impl Expression {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Literal(expression) => expression.range,
            Self::Name(expression) => expression.range,
            Self::Invocation(expression) => expression.range,
            Self::TestCall(expression) => expression.range,
            Self::Object(expression) => expression.range,
            Self::Construction(expression) => expression.range,
            Self::Create(expression) => expression.range,
            Self::Query(expression) => expression.range,
            Self::Update(expression) => expression.range,
            Self::Delete(expression) => expression.range,
            Self::Attempt(expression) => expression.range,
            Self::OutcomeMatch(expression) => expression.range,
            Self::Unary(expression) => expression.range,
            Self::Binary(expression) => expression.range,
            Self::Grouped(expression) => expression.range,
            Self::Missing(range) => *range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectExpression {
    pub fields: Vec<FieldInitialiser>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptExpression {
    pub value: Box<Expression>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutcomeMatchExpression {
    pub subject: Box<Expression>,
    pub arms: Vec<OutcomeMatchArm>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutcomeMatchArm {
    pub pattern: OutcomeMatchPattern,
    pub body: OutcomeMatchArmBody,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutcomeMatchPattern {
    Success(Name),
    Failure(Name),
}

impl OutcomeMatchPattern {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Success(binding) | Self::Failure(binding) => binding.range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutcomeMatchArmBody {
    Value(Expression),
    Reject(RejectStatement),
    Propagate(TextRange),
}

impl OutcomeMatchArmBody {
    pub const fn range(&self) -> TextRange {
        match self {
            Self::Value(expression) => expression.range(),
            Self::Reject(statement) => statement.range,
            Self::Propagate(range) => *range,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateExpression {
    pub target: NameExpression,
    pub fields: Vec<FieldInitialiser>,
    pub conflicts: Vec<ConflictBinding>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictBinding {
    pub constraint: Option<NameExpression>,
    pub rejection: RejectStatement,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryCardinality {
    Optional,
    Required,
    Many,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryOrderDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryOrder {
    pub field: Name,
    pub direction: QueryOrderDirection,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPagination {
    pub limit: Box<Expression>,
    pub offset: Box<Expression>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryInclude {
    pub relationship: Name,
    pub nested_relationship: Option<Name>,
    pub cardinality: QueryIncludeCardinality,
    pub result: TypeReference,
    pub order: QueryOrder,
    pub pagination: QueryPagination,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryIncludeCardinality {
    Many,
    Required,
    Optional,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryExpression {
    pub cardinality: QueryCardinality,
    pub target: NameExpression,
    pub field: Name,
    pub value: Box<Expression>,
    pub order: Option<QueryOrder>,
    pub pagination: Option<QueryPagination>,
    pub page: Option<QueryPage>,
    pub includes: Vec<QueryInclude>,
    pub missing: Option<RejectStatement>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryPagePredicateOperator {
    Equal,
    OptionalEqual,
    OptionalLessEqual,
    RequiredGreaterEqual,
    RequiredLess,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPagePredicate {
    pub field: Name,
    pub operator: QueryPagePredicateOperator,
    pub value: Box<Expression>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPage {
    pub result: TypeReference,
    pub predicates: Vec<QueryPagePredicate>,
    pub order: Vec<QueryOrder>,
    pub after: Box<Expression>,
    pub after_optional: bool,
    pub limit: Box<Expression>,
    pub projection: TypeReference,
    pub cursor: TypeReference,
    pub cursor_fields: Vec<Name>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateExpression {
    pub target: NameExpression,
    pub field: Name,
    pub value: Box<Expression>,
    pub transition: Option<Name>,
    pub changes: Vec<FieldInitialiser>,
    pub conditional_changes: Vec<PatchConditionalChange>,
    pub patch: Option<NameExpression>,
    pub empty: Option<RejectStatement>,
    pub missing: RejectStatement,
    pub conflicts: Vec<ConflictBinding>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchConditionalChange {
    pub change: FieldInitialiser,
    pub supplied: NameExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteExpression {
    pub target: NameExpression,
    pub field: Name,
    pub value: Box<Expression>,
    pub missing: RejectStatement,
    pub conflicts: Vec<ConflictBinding>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiteralKind {
    String,
    Integer,
    Decimal,
    Boolean,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Literal {
    pub kind: LiteralKind,
    pub text: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameExpression {
    pub path: Vec<Name>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationExpression {
    pub callee: NameExpression,
    pub arguments: Vec<Expression>,
    pub named_arguments: Vec<FieldInitialiser>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestCallExpression {
    pub invocation: InvocationExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructionExpression {
    pub target: NameExpression,
    pub fields: Vec<FieldInitialiser>,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOperator {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOperator {
    Not,
    Negate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnaryExpression {
    pub operator: UnaryOperator,
    pub value: Box<Expression>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryExpression {
    pub left: Box<Expression>,
    pub operator: BinaryOperator,
    pub right: Box<Expression>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupedExpression {
    pub value: Box<Expression>,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldInitialiser {
    pub name: Name,
    pub value: Expression,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RouteSuccess {
    #[default]
    Ok,
    Created,
    NoContent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteDeclaration {
    pub method: HttpMethod,
    pub path: String,
    pub path_range: TextRange,
    pub public: bool,
    pub fresh_authority: bool,
    pub path_fields: Vec<FieldDeclaration>,
    pub query: Option<TypeReference>,
    pub headers: Vec<RouteHeaderBinding>,
    pub input: Option<TypeReference>,
    pub output: Option<TypeReference>,
    pub success: RouteSuccess,
    pub deadline: Option<DurationLiteral>,
    pub run: Option<InvocationExpression>,
    pub inline_action: Option<InlineAction>,
    pub range: TextRange,
}

/// Checked schedule entry, not an authored callable or delivery capability.
/// Optional clauses retain parser recovery; diagnostics reject missing clauses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeclaration {
    pub name: Name,
    pub every: DurationLiteral,
    pub concurrency: Option<JobConcurrency>,
    pub run: Option<InvocationExpression>,
    pub retry: Option<JobRetry>,
    pub delivery: Option<JobReminderDelivery>,
    pub range: TextRange,
}

/// Closed source descriptor only. This is not a checked delivery capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobReminderDelivery {
    pub selection: JobDeliverySelection,
    pub hooks: JobDeliveryHooks,
    pub service: JobDeliveryService,
    pub authority: JobDeliveryAuthority,
    pub completion: JobDeliveryCompletion,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobDeliveryTerm {
    StrictBeforeOperationTime,
    DueIdentityContinuation,
    GeneratedIntent,
    ReminderOnly,
    CompilerReceiptObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobDeliveryLimit {
    FiveHundred,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobDeliveryPayloadVersion {
    ReminderV1,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobDeliveryDirection {
    Ascending,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliveryOrder {
    pub field: NameExpression,
    pub direction: JobDeliveryDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliverySelection {
    pub name: Name,
    pub entity: Name,
    pub identity: NameExpression,
    pub due: NameExpression,
    pub before: JobDeliveryTerm,
    pub open_field: NameExpression,
    pub open_variant: NameExpression,
    pub visible: Name,
    pub unsent: NameExpression,
    pub required_owner: NameExpression,
    pub owner_visible: Name,
    pub order_by: [JobDeliveryOrder; 2],
    pub limit: JobDeliveryLimit,
    pub continuation: JobDeliveryTerm,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliveryHooks {
    pub create: NameExpression,
    pub patch: NameExpression,
    pub supplied: NameExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliveryPayload {
    pub idempotency_key: JobDeliveryTerm,
    pub from: NameExpression,
    pub to: NameExpression,
    pub todo_title: NameExpression,
    pub due_at: NameExpression,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliveryService {
    pub operation: NameExpression,
    pub intent: Name,
    pub input: Name,
    pub output: Name,
    pub payload_version: JobDeliveryPayloadVersion,
    pub payload: JobDeliveryPayload,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliveryAuthority {
    pub validator: NameExpression,
    pub membership: Name,
    pub role: NameExpression,
    pub permit: JobDeliveryTerm,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDeliveryCompletion {
    pub name: Name,
    pub field: NameExpression,
    pub time: JobDeliveryTerm,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobConcurrency {
    Singleton,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobRetry {
    NextSchedule,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteHeaderBinding {
    pub name: Name,
    pub field_type: TypeReference,
    pub wire_name: Literal,
    pub optional: bool,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineAction {
    pub failures: Vec<Name>,
    pub failures_range: Option<TextRange>,
    pub body: Block,
    pub range: TextRange,
}
