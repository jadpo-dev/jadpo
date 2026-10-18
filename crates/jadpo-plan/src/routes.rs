//! The route table (area 7; docs/compiler-runtime.md §2.6): each checked
//! route as `jadpo-std`'s plain data, with a JSON codec for every type that
//! crosses the boundary, read from the checker's route models and the
//! graph's type plans, so the runtime's HTTP component routes, decodes and
//! answers a request the same way for either executor.

use std::collections::HashMap;

use jadpo_check::routes::wire::{int_as_number, merge};
use jadpo_check::routes::{self, ArgRoot};
use jadpo_db::{Db, Project};
use jadpo_graph::{Base, Detail, Key, Kind, Section, TypeRef};
use jadpo_std::http::{
    ApiPlan, Arg, Codec, CodecId, FailureCodec, InputPlan, RoutePlan, RouteTable, Run, Segment,
    VariantCodec,
};

use crate::queries::QueryPlan;
use crate::types::{Types, last};

/// How far a type's chain is followed; only a defect gets this far.
const DEPTH: usize = 256;

/// The codecs `json.read` and `json.write` run with (area 19, "Data
/// formats"): the wire's codec for each of `types`, in the order the
/// calls' sites number them, built as the route table's are.
pub(crate) fn json_table(
    db: &dyn Db,
    project: Project,
    types: &mut Types,
    read_or_written: &[TypeRef],
) -> jadpo_std::http::JsonTable {
    let mut codecs =
        Codecs::new(types).supporting(jadpo_check::locales::locales(db, project).plan.clone());
    let ids = read_or_written.iter().map(|ty| codecs.codec(ty)).collect();
    jadpo_std::http::JsonTable {
        codecs: codecs.into_codecs(),
        types: ids,
    }
}

/// The route table of a project whose routes checked cleanly.
pub(crate) fn table(
    db: &dyn Db,
    project: Project,
    types: &mut Types,
    callable: &dyn Fn(&Key) -> Option<u32>,
    queries: &[QueryPlan],
) -> RouteTable {
    let groups = routes::groups(db, project);
    let apis = routes::api_table(db, project);
    let identity = jadpo_check::identity::identity(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let mut codecs =
        Codecs::new(types).supporting(jadpo_check::locales::locales(db, project).plan.clone());
    let mut out = RouteTable {
        apis: apis
            .apis
            .iter()
            .map(|api| ApiPlan {
                name: api.name.clone(),
                prefix: api.prefix.clone(),
                idempotency: api.idempotency,
                // An API that names none accepts every declared method.
                methods: if api.methods.is_empty() {
                    identity
                        .methods
                        .iter()
                        .map(|method| method.method)
                        .collect()
                } else {
                    api.methods.clone()
                },
            })
            .collect(),
        ..RouteTable::default()
    };
    let graph = std::sync::Arc::clone(jadpo_check::graph::graph(db, project));
    let written = jadpo_check::types::written_stores(db, project);
    let databases = jadpo_check::data::connections(db, project).databases();
    let mut stores: Vec<String> = jadpo_check::data::data_model(db, project)
        .entities
        .iter()
        .filter_map(|entity| entity.store.clone())
        .filter(|store| databases.contains(store))
        .collect();
    stores.sort();
    stores.dedup();
    out.stores = stores;
    for (group, route) in groups.routes() {
        let Some(api) = out.apis.iter().position(|api| api.name == group.api) else {
            continue;
        };
        let mut params = Vec::new();
        let segments = route
            .segments
            .iter()
            .map(|segment| match segment {
                routes::Segment::Fixed(text) => Segment::Fixed(text.clone()),
                routes::Segment::Param { name, ty } => {
                    params.push(name.clone());
                    let ty = jadpo_check::graph::type_ref(&lookup, ty);
                    Segment::Param {
                        name: name.clone(),
                        codec: codecs.codec(&ty),
                    }
                }
            })
            .collect();
        let input = route.input.as_ref().and_then(|input| {
            let key = jadpo_check::graph::declaration_key(&lookup, input)?;
            let headers = graph
                .find(&key)
                .map(|id| {
                    graph
                        .fields(id)
                        .into_iter()
                        .filter_map(|field| match &graph.node(field).detail {
                            Detail::Field {
                                header: Some(header),
                                ..
                            } => Some((
                                last(&graph.node(field).name).to_string(),
                                header.to_ascii_lowercase(),
                            )),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(InputPlan {
                codec: codecs.codec(&TypeRef::Node(key)),
                query: route.method.reads_query(),
                headers,
            })
        });
        let output = route
            .output
            .as_ref()
            .map(|ty| codecs.codec(&jadpo_check::graph::type_ref(&lookup, ty)));
        let (run, keys) = if let Some(op) = route.run.generated {
            (
                Run::Listing {
                    keys: op.on_service(),
                },
                None,
            )
        } else if route.run.query {
            let Some(query) = queries.iter().find(|query| query.name == route.run.name) else {
                continue;
            };
            (Run::Query(Box::new(query.read.clone())), None)
        } else {
            let Some(key) = jadpo_check::graph::declaration_key(&lookup, &route.run.name) else {
                continue;
            };
            let Some(id) = callable(&key) else {
                continue;
            };
            // Its keys go in the store it writes (area 5, "System tables").
            let keys = written
                .get(key.name())
                .and_then(|stores| stores.keys().next().cloned());
            (Run::Action(id), keys)
        };
        let args = route
            .run
            .args
            .iter()
            .map(|arg| Arg {
                root: match &arg.root {
                    ArgRoot::Input => jadpo_std::http::ArgRoot::Input,
                    ArgRoot::Param(name) => jadpo_std::http::ArgRoot::Param(
                        params.iter().position(|p| p == name).unwrap_or(0) as u32,
                    ),
                    ArgRoot::Caller => jadpo_std::http::ArgRoot::Caller,
                },
                path: arg.path.clone(),
            })
            .collect();
        for failure in &route.fails {
            let Some(key) = jadpo_check::graph::declaration_key(&lookup, failure) else {
                continue;
            };
            if out.failure(key.as_str()).is_some() {
                continue;
            }
            let client = graph
                .find(&key)
                .map(|id| {
                    graph
                        .fields(id)
                        .into_iter()
                        .filter(|field| {
                            matches!(
                                graph.node(*field).detail,
                                Detail::Field {
                                    section: Some(Section::Client),
                                    ..
                                }
                            )
                        })
                        .map(|field| {
                            let node = graph.node(field);
                            (
                                last(&node.name).to_string(),
                                codecs.codec(&TypeRef::Field(node.key.clone())),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.failures.push(FailureCodec {
                key: key.as_str().to_string(),
                client,
            });
        }
        let line = graph
            .get(&route.key)
            .and_then(|node| node.place.as_ref())
            .map_or(1, |place| place.lines.0);
        out.routes.push(RoutePlan {
            key: route.key.as_str().to_string(),
            method: route.method,
            segments,
            api: api as u32,
            input,
            output,
            status: route.status,
            deadline_ms: route.deadline_ms,
            run,
            keys,
            args,
            authentication: route.authentication,
            caller: route.caller,
            file: group.path.clone(),
            line,
        });
    }
    // The exchange the runtime answers (area 8, "Ways to sign in"): in
    // the first API that accepts service keys, which the token it gives
    // signs in to, as every API that accepts them is.
    let exchange_api = out.apis.iter().position(|api| {
        api.methods
            .contains(&jadpo_std::AuthenticationMethod::ServiceKey)
    });
    if identity.exchanges()
        && let Some(api) = exchange_api
    {
        out.routes.push(RoutePlan {
            key: format!("route POST /{}", jadpo_std::http::EXCHANGE_PATH.join("/")),
            method: jadpo_std::HttpMethod::Post,
            segments: jadpo_std::http::EXCHANGE_PATH
                .iter()
                .map(|text| Segment::Fixed(text.to_string()))
                .collect(),
            api: api as u32,
            input: None,
            output: None,
            status: jadpo_std::SuccessStatus::Ok,
            deadline_ms: jadpo_std::http::DEFAULT_DEADLINE_MS,
            run: Run::Exchange,
            keys: None,
            args: Vec::new(),
            authentication: jadpo_std::http::Authentication::Required,
            caller: Some(jadpo_std::http::PrincipalKind::Service),
            file: "generated".to_string(),
            line: 0,
        });
    }
    // The passkey routes the runtime answers (area 8, "Passkeys"): in the
    // first API that accepts browser sessions, whose cookie they read and
    // start.
    let passkey_api = out.apis.iter().position(|api| {
        api.methods
            .contains(&jadpo_std::AuthenticationMethod::BrowserSession)
    });
    if let Some(passkey) = identity.passkeys()
        && let Some(api) = passkey_api
    {
        let id = codecs.codec(&TypeRef::Builtin(jadpo_std::Builtin::Uuid));
        for step in jadpo_std::http::PasskeyStep::ALL {
            let segments = step.segments(&passkey.path);
            out.routes.push(RoutePlan {
                key: format!("route {} /{}", step.method().word(), segments.join("/")),
                method: step.method(),
                segments: segments
                    .into_iter()
                    .map(|text| {
                        if text == jadpo_std::http::PASSKEY_ID {
                            Segment::Param {
                                name: "passkey_id".to_string(),
                                codec: id,
                            }
                        } else {
                            Segment::Fixed(text)
                        }
                    })
                    .collect(),
                api: api as u32,
                input: None,
                output: None,
                status: step.status(),
                deadline_ms: jadpo_std::http::DEFAULT_DEADLINE_MS,
                run: Run::Passkey(step),
                keys: None,
                args: Vec::new(),
                authentication: step.authentication(),
                caller: (step.authentication().needs_caller())
                    .then_some(jadpo_std::http::PrincipalKind::User),
                file: "generated".to_string(),
                line: 0,
            });
        }
    }
    out.codecs = codecs.codecs;
    out
}

/// Numbers the codecs of the types the routes use, once each.
pub(crate) struct Codecs<'t> {
    types: &'t mut Types,
    codecs: Vec<Codec>,
    ids: HashMap<TypeRef, CodecId>,
    /// The locales the application supports, when it declares them: a
    /// `Locale` on the wire is one of these or an invalid value (area 10,
    /// "Locales").
    locales: Option<jadpo_std::LocalePlan>,
}

/// What a type is made of, for its codec.
enum Made {
    Scalar,
    Object(Key),
    Enum(Key),
    StdEnum(String),
    List(TypeRef),
    /// A `Vector` (area 19, chunk 5.5s2): a list of single-precision
    /// floats, each a JSON number.
    Floats,
    Set(TypeRef),
    Map(TypeRef, TypeRef),
    Page(TypeRef),
    /// `IssuedToken` or `IssuedKey` (area 8), or `DecodeProblem` (area
    /// 19).
    Issued(&'static str),
    /// A standard object type, `InstantRange` (area 19): its fields from
    /// its data entry.
    StdObject(&'static jadpo_std::StandardType),
    /// `Range<T>` (area 19): its two ends, each a `T` or `null`.
    Range(TypeRef),
    Unknown,
}

impl<'t> Codecs<'t> {
    /// No codec numbered yet, over `types`.
    pub(crate) fn new(types: &'t mut Types) -> Codecs<'t> {
        Codecs {
            types,
            codecs: Vec::new(),
            ids: HashMap::new(),
            locales: None,
        }
    }

    /// Reads a `Locale` as one of `locales` only.
    pub(crate) fn supporting(mut self, locales: Option<jadpo_std::LocalePlan>) -> Codecs<'t> {
        self.locales = locales;
        self
    }

    /// Every codec numbered, by number.
    pub(crate) fn into_codecs(self) -> Vec<Codec> {
        self.codecs
    }

    pub(crate) fn codec(&mut self, ty: &TypeRef) -> CodecId {
        if let Some(id) = self.ids.get(ty) {
            return *id;
        }
        // Reserved first, so a type that holds itself (through a list)
        // refers to its own number.
        let id = CodecId(self.codecs.len() as u32);
        self.codecs.push(Codec::Nullable(id));
        self.ids.insert(ty.clone(), id);
        let codec = if self.types.is_nullable(ty) && !matches!(ty, TypeRef::Field(_)) {
            match ty {
                TypeRef::Nullable(inner) => Codec::Nullable(self.codec(inner)),
                _ => self.made(ty),
            }
        } else if self.types.is_nullable(ty) {
            // A field declared `T?`: its own codec without the `?`.
            let inner = self.made(ty);
            let inner_id = CodecId(self.codecs.len() as u32);
            self.codecs.push(inner);
            Codec::Nullable(inner_id)
        } else {
            self.made(ty)
        };
        if let Some(slot) = self.codecs.get_mut(id.0 as usize) {
            *slot = codec;
        }
        id
    }

    /// The codec of `ty` as if it weren't nullable.
    fn made(&mut self, ty: &TypeRef) -> Codec {
        match self.what(ty) {
            Made::Scalar => {
                let (builtin, rules) = self
                    .types
                    .chain(ty)
                    .unwrap_or((jadpo_std::Builtin::Text, Vec::new()));
                Codec::Scalar {
                    builtin,
                    validator: self.types.validator_id(ty),
                    int_as_number: int_as_number(&merge(&rules)),
                }
            }
            Made::Object(key) => {
                let Some(shape) = self.types.shape_id(&key) else {
                    return self.made_unknown();
                };
                let keys: Vec<String> = self
                    .types
                    .shape(shape)
                    .map(|plan| plan.fields.iter().map(|f| f.key.clone()).collect())
                    .unwrap_or_default();
                let fields = keys
                    .into_iter()
                    .map(|key| self.codec(&TypeRef::Field(Key::from_text(key))))
                    .collect();
                // Its rules across its fields, checked once they are read
                // (area 2, "Rules across fields").
                let object = TypeRef::Node(key);
                let validator = (!self.types.across(&object).is_empty())
                    .then(|| self.types.validator_id(&object));
                Codec::Object {
                    shape,
                    fields,
                    validator,
                }
            }
            Made::Enum(key) => {
                let ty_name = self.types.name(&key).to_string();
                let variants: Vec<(String, u32, Key, bool)> = {
                    let g = self.types.graph();
                    let Some(id) = g.find(&key) else {
                        return Codec::Enum {
                            ty: ty_name,
                            variants: Vec::new(),
                        };
                    };
                    let mut found: Vec<(String, u32, Key, bool)> = g
                        .children(id)
                        .filter_map(|child| {
                            let node = g.node(child);
                            match &node.detail {
                                Detail::Variant { order, payload } => Some((
                                    last(&node.name).to_string(),
                                    *order,
                                    node.key.clone(),
                                    *payload,
                                )),
                                _ => None,
                            }
                        })
                        .collect();
                    found.sort_by_key(|(_, order, ..)| *order);
                    found
                };
                let variants = variants
                    .into_iter()
                    .map(|(name, ordinal, key, payload)| {
                        let fields = if payload {
                            self.types.shape_id(&key).map(|shape| {
                                let keys: Vec<String> = self
                                    .types
                                    .shape(shape)
                                    .map(|plan| plan.fields.iter().map(|f| f.key.clone()).collect())
                                    .unwrap_or_default();
                                let codecs = keys
                                    .into_iter()
                                    .map(|key| self.codec(&TypeRef::Field(Key::from_text(key))))
                                    .collect();
                                (shape, codecs)
                            })
                        } else {
                            None
                        };
                        VariantCodec {
                            wire: name.clone(),
                            name,
                            ordinal,
                            fields,
                        }
                    })
                    .collect();
                Codec::Enum {
                    ty: ty_name,
                    variants,
                }
            }
            // `Geometry`, sent as the GeoJSON geometry it holds (chunk
            // 5.5s).
            Made::StdEnum(name)
                if jadpo_std::standard_type(&name)
                    .is_some_and(|entry| entry.json == jadpo_std::standard::Json::GeoJson) =>
            {
                Codec::Standard { ty: name }
            }
            Made::StdEnum(name) => {
                let spelling = jadpo_std::config::Spelling::of(&name);
                let listed: Vec<(usize, &str)> = jadpo_std::enums()
                    .into_iter()
                    .find(|(written, _)| *written == name)
                    .map(|(_, variants)| variants)
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .filter(|(ordinal, _)| {
                        name != jadpo_std::LOCALE
                            || self.locales.as_ref().is_none_or(|plan| {
                                u32::try_from(*ordinal).is_ok_and(|at| plan.supports(at))
                            })
                    })
                    .collect();
                // A choice's variants carry their parts (chunk 5.5i3).
                let variants = listed
                    .into_iter()
                    .map(|(ordinal, variant)| VariantCodec {
                        name: variant.to_string(),
                        wire: spelling.spelling(variant),
                        ordinal: ordinal as u32,
                        fields: self.choice_fields(&name, variant),
                    })
                    .collect();
                Codec::Enum { variants, ty: name }
            }
            Made::List(item) => Codec::List {
                item: self.codec(&item),
                validator: self.types.validator_id(ty),
            },
            Made::Floats => {
                let item = CodecId(self.codecs.len() as u32);
                self.codecs.push(Codec::Float);
                Codec::List {
                    item,
                    validator: self.types.validator_id(ty),
                }
            }
            Made::Set(item) => Codec::Set {
                item: self.codec(&item),
                validator: self.types.validator_id(ty),
            },
            Made::Map(key, value) => Codec::Map {
                key: self.codec(&key),
                value: self.codec(&value),
                validator: self.types.validator_id(ty),
            },
            Made::Page(item) => Codec::Page {
                item: self.codec(&item),
            },
            Made::Issued(name) => {
                let shape = self.types.issued_shape(name);
                let fields = jadpo_std::http::builtin_object_fields(name)
                    .unwrap_or_default()
                    .iter()
                    .map(|(_, builtin)| self.codec(&TypeRef::Builtin(*builtin)))
                    .collect();
                Codec::Object {
                    shape,
                    fields,
                    validator: None,
                }
            }
            Made::StdObject(entry) => {
                let jadpo_std::standard::Holds::Object(fields) = entry.holds else {
                    return self.made_unknown();
                };
                // Sent as its canonical text (area 19, "Time and calendar
                // additions"), or as an object of its fields.
                // A geometry is sent as GeoJSON (chunk 5.5s).
                if matches!(
                    entry.json,
                    jadpo_std::standard::Json::Text | jadpo_std::standard::Json::GeoJson
                ) {
                    return Codec::Standard {
                        ty: entry.name.to_string(),
                    };
                }
                let shape = self.types.standard_shape(entry.name, fields);
                let fields = fields
                    .iter()
                    .map(|(field, part)| {
                        self.codec(&crate::types::part_type(entry.name, field, *part))
                    })
                    .collect();
                // A type built on it with rules of its own, a rate type's
                // currencies, checks the whole value once read (chunk
                // 5.5h4), as its rules across its parts do (chunk 5.5t2).
                let plan = self.types.validator_plan(ty);
                let validator = (!plan.across.is_empty()
                    || !plan.rules.iter().all(jadpo_std::Rules::is_empty))
                .then(|| self.types.validator_id(ty));
                Codec::Object {
                    shape,
                    fields,
                    validator,
                }
            }
            // Its start not after its end, by the rule every range keeps
            // (chunk 5.5t2).
            Made::Range(item) => {
                let shape = self.types.range_shape();
                let end = self.codec(&TypeRef::Nullable(Box::new(item)));
                let range = TypeRef::Std(jadpo_std::RANGE.to_string(), Vec::new());
                Codec::Object {
                    shape,
                    fields: vec![end, end],
                    validator: Some(self.types.validator_id(&range)),
                }
            }
            Made::Unknown => self.made_unknown(),
        }
    }

    /// A standard choice's variant's shape and its fields' codecs, from its
    /// entry's parts (chunk 5.5i3); `None` for a variant with no data.
    fn choice_fields(
        &mut self,
        choice: &str,
        variant: &str,
    ) -> Option<(jadpo_std::types::ShapeId, Vec<CodecId>)> {
        let fields = jadpo_std::standard::choice_variant(choice, variant)?;
        let shape = self
            .types
            .shape_id(&Key::new(Kind::Variant, &format!("{choice}.{variant}")))?;
        let host = format!("{choice}.{variant}");
        let codecs = fields
            .iter()
            .map(|(field, part)| self.codec(&crate::types::part_type(&host, field, *part)))
            .collect();
        Some((shape, codecs))
    }

    /// A type the boundary can't carry, which the checker rules out: `Unit`.
    fn made_unknown(&mut self) -> Codec {
        Codec::Scalar {
            builtin: jadpo_std::Builtin::Unit,
            validator: self
                .types
                .validator_id(&TypeRef::Builtin(jadpo_std::Builtin::Unit)),
            int_as_number: false,
        }
    }

    fn what(&self, ty: &TypeRef) -> Made {
        if self.types.chain(ty).is_some() {
            return Made::Scalar;
        }
        let graph = self.types.graph();
        let mut current = ty.clone();
        for _ in 0..DEPTH {
            match current {
                TypeRef::Nullable(inner) => current = *inner,
                TypeRef::Field(key) => match graph.get(&key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, .. }) => current = ty.clone(),
                    _ => return Made::Unknown,
                },
                TypeRef::Node(key) => match graph.get(&key) {
                    Some(node) => match &node.detail {
                        Detail::Type {
                            base: Base::Refines(parent),
                            ..
                        } => current = parent.clone(),
                        Detail::Type {
                            base: Base::Enum, ..
                        } => return Made::Enum(key),
                        _ if matches!(node.kind, Kind::Type | Kind::Entity) => {
                            return Made::Object(key);
                        }
                        _ => return Made::Unknown,
                    },
                    None => return Made::Unknown,
                },
                TypeRef::List(inner) => return Made::List(*inner),
                TypeRef::Set(inner) => return Made::Set(*inner),
                TypeRef::Map(key, value) => return Made::Map(*key, *value),
                TypeRef::Std(name, mut args) if name == "Page" && args.len() == 1 => {
                    return Made::Page(args.remove(0));
                }
                TypeRef::Std(name, mut args) if name == jadpo_std::RANGE && args.len() == 1 => {
                    return Made::Range(args.remove(0));
                }
                TypeRef::Std(name, args) if args.is_empty() => {
                    if let Some(entry) = jadpo_std::standard_type(&name) {
                        // A named collection is sent as JSON's own (chunk
                        // 5.5o2): an array of its items, an object of its
                        // entries.
                        let part = |field: &str, part| crate::types::part_type(&name, field, part);
                        match entry.holds {
                            jadpo_std::standard::Holds::Object(_) => {
                                return Made::StdObject(entry);
                            }
                            jadpo_std::standard::Holds::List(jadpo_std::Part::Float) => {
                                return Made::Floats;
                            }
                            jadpo_std::standard::Holds::List(item) => {
                                return Made::List(part("item", item));
                            }
                            jadpo_std::standard::Holds::Map(key, value) => {
                                return Made::Map(part("key", key), part("value", value));
                            }
                            _ => {}
                        }
                    }
                    return match routes::wire::issued_name(&name) {
                        Some(issued) => Made::Issued(issued),
                        None => Made::StdEnum(name),
                    };
                }
                _ => return Made::Unknown,
            }
        }
        Made::Unknown
    }
}
