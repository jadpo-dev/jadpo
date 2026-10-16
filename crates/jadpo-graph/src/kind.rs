//! The kinds of node and edge (docs/compiler-runtime.md §2.3). Every kind
//! the language's areas need is listed now, including those later areas
//! build, so the graph's shape is not fixed by what Phase 1 happens to
//! produce (§2.9, "graph shape fixed too early"): a later area fills a
//! reserved kind without changing anything that reads the others.

/// The node groups of §2.3's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Structure,
    Types,
    Data,
    Behaviour,
    Triggers,
    Access,
    Outside,
    Services,
    Records,
}

impl Group {
    pub fn word(self) -> &'static str {
        match self {
            Group::Structure => "structure",
            Group::Types => "types",
            Group::Data => "data",
            Group::Behaviour => "behaviour",
            Group::Triggers => "triggers",
            Group::Access => "access",
            Group::Outside => "outside",
            Group::Services => "services",
            Group::Records => "records",
        }
    }
}

/// How far the compiler builds a kind today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// Nodes of this kind carry everything their area decides.
    Built,
    /// A node exists for each declaration, with its identity, place, digest
    /// and description; what is inside it comes with its own area.
    Declared,
    /// Named so the shape is fixed; no node is built yet.
    Reserved,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Built => "built",
            Status::Declared => "declared",
            Status::Reserved => "reserved",
        }
    }
}

macro_rules! kinds {
    ($($name:ident: $word:literal, $group:ident, $status:ident, $area:literal;)*) => {
        /// What a node is.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Kind {
            $($name,)*
        }

        impl Kind {
            /// Every kind, in §2.3's order.
            pub const ALL: &'static [Kind] = &[$(Kind::$name,)*];

            /// The kind's word: the first part of every key, and `kind` in
            /// the manifest.
            pub fn word(self) -> &'static str {
                match self {
                    $(Kind::$name => $word,)*
                }
            }

            pub fn group(self) -> Group {
                match self {
                    $(Kind::$name => Group::$group,)*
                }
            }

            pub fn status(self) -> Status {
                match self {
                    $(Kind::$name => Status::$status,)*
                }
            }

            /// The language area that decides the kind.
            pub fn area(self) -> u8 {
                match self {
                    $(Kind::$name => $area,)*
                }
            }

            pub fn from_word(word: &str) -> Option<Kind> {
                Kind::ALL.iter().copied().find(|kind| kind.word() == word)
            }
        }
    };
}

kinds! {
    Domain: "domain", Structure, Built, 1;
    Library: "library", Structure, Built, 1;
    File: "file", Structure, Built, 1;
    Deployable: "deployable", Structure, Reserved, 14;

    Type: "type", Types, Built, 2;
    Field: "field", Types, Built, 2;
    Variant: "variant", Types, Built, 2;
    Constraint: "constraint", Types, Reserved, 5;

    Entity: "entity", Data, Built, 5;
    UniqueRule: "unique_rule", Data, Built, 5;
    Index: "index", Data, Built, 5;
    Reference: "reference", Data, Built, 5;
    Lifecycle: "lifecycle", Data, Built, 5;
    Transition: "transition", Data, Built, 5;
    Cache: "cache", Data, Reserved, 5;
    StorePlacement: "store_placement", Data, Built, 5;
    SystemTable: "system_table", Data, Built, 5;
    PersonalMark: "personal_mark", Data, Reserved, 15;
    Retention: "retention", Data, Reserved, 15;
    Person: "person", Data, Reserved, 15;

    Function: "function", Behaviour, Built, 3;
    Action: "action", Behaviour, Built, 3;
    Query: "query", Behaviour, Built, 6;
    Fixture: "fixture", Behaviour, Declared, 13;
    Seed: "seed", Behaviour, Declared, 5;
    Test: "test", Behaviour, Declared, 13;
    Site: "site", Behaviour, Built, 18;
    Branch: "branch", Behaviour, Built, 16;

    Api: "api", Triggers, Built, 7;
    RouteGroup: "route_group", Triggers, Built, 7;
    Route: "route", Triggers, Built, 7;
    Schedule: "schedule", Triggers, Declared, 12;
    Subscriber: "subscriber", Triggers, Declared, 12;
    GeneratedRoute: "generated_route", Triggers, Reserved, 8;

    Principal: "principal", Access, Reserved, 8;
    AuthenticationMethod: "authentication_method", Access, Reserved, 8;
    SignInProvider: "sign_in_provider", Access, Reserved, 8;
    Grant: "grant", Access, Built, 9;
    FieldRestriction: "field_restriction", Access, Built, 9;
    Tenancy: "tenancy", Access, Built, 9;
    ActionException: "action_exception", Access, Built, 9;
    OauthScope: "oauth_scope", Access, Reserved, 8;

    Connection: "connection", Outside, Reserved, 11;
    Package: "package", Outside, Reserved, 11;
    Resource: "resource", Outside, Reserved, 11;
    Operation: "operation", Outside, Reserved, 11;
    CapabilityMapping: "capability_mapping", Outside, Reserved, 11;
    Host: "host", Outside, Reserved, 11;

    ConfigField: "config_field", Services, Reserved, 10;
    LocaleSet: "locale_set", Services, Reserved, 10;
    TranslationKey: "translation_key", Services, Built, 10;
    EmailTemplate: "email_template", Services, Reserved, 16;
    Flag: "flag", Services, Reserved, 16;
    RateLimit: "rate_limit", Services, Reserved, 7;
    Threshold: "threshold", Services, Reserved, 16;

    ApiRecord: "api_record", Records, Reserved, 7;
    MigrationSnapshot: "migration_snapshot", Records, Built, 5;
    RegistryEntry: "registry_entry", Records, Reserved, 18;
    Acceptance: "acceptance", Records, Reserved, 18;
    LockEntry: "lock_entry", Records, Reserved, 11;
}

impl Kind {
    /// Whether nodes of this kind are numbered per build rather than
    /// known by a stable identity: a callable's sites and branches (area
    /// 18), which a diff leaves to the callable's own change.
    pub fn per_build(self) -> bool {
        matches!(self, Kind::Site | Kind::Branch)
    }

    /// Whether nodes of this kind are written in source and can be called
    /// by name: functions, actions, queries and fixtures.
    pub fn is_callable(self) -> bool {
        matches!(
            self,
            Kind::Function | Kind::Action | Kind::Query | Kind::Fixture
        )
    }
}

macro_rules! edges {
    ($($name:ident: $word:literal, $status:ident, $meaning:literal;)*) => {
        /// What an edge says about its two nodes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum EdgeKind {
            $($name,)*
        }

        impl EdgeKind {
            pub const ALL: &'static [EdgeKind] = &[$(EdgeKind::$name,)*];

            /// The edge's word: the key of its list on the node it starts at.
            pub fn word(self) -> &'static str {
                match self {
                    $(EdgeKind::$name => $word,)*
                }
            }

            pub fn status(self) -> Status {
                match self {
                    $(EdgeKind::$name => Status::$status,)*
                }
            }

            /// One line for the schema.
            pub fn meaning(self) -> &'static str {
                match self {
                    $(EdgeKind::$name => $meaning,)*
                }
            }

            pub fn from_word(word: &str) -> Option<EdgeKind> {
                EdgeKind::ALL.iter().copied().find(|kind| kind.word() == word)
            }
        }
    };
}

edges! {
    Calls: "calls", Built, "A callable calls a function, action, query, fixture or library function; a test sends a request through a route";
    Reads: "reads", Built, "A query or an entity's `get` reads an entity";
    Writes: "writes", Built, "An action or fixture creates, updates or deletes an entity's records";
    Emits: "emits", Reserved, "Emits an event";
    Subscribes: "subscribes", Reserved, "A subscriber listens for an event";
    Runs: "runs", Built, "A trigger runs an action or query";
    Raises: "raises", Built, "A failure can leave the callable";
    Maps: "maps", Reserved, "One failure becomes another";
    References: "references", Built, "An entity refers to another";
    Includes: "includes", Built, "A query includes related records of an entity";
    Grants: "grants", Built, "A grant or field restriction grants an effect on its entity or field; a listed action's line lets its callers in";
    Scopes: "scopes", Built, "A tenancy limits an entity's records to its tenant";
    Invalidates: "invalidates", Reserved, "A write invalidates a cache";
    Uses: "uses", Built, "Names a type, entity or failure in its signature, fields or body";
    Provides: "provides", Reserved, "A package provides a capability";
    Reaches: "reaches", Reserved, "A deployable reaches a node";
    Covers: "covers", Reserved, "A test covers a node";
}
