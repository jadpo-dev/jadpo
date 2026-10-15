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
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let mut codecs = Codecs {
        types,
        codecs: Vec::new(),
        ids: HashMap::new(),
    };
    let mut out = RouteTable {
        apis: apis
            .apis
            .iter()
            .map(|api| ApiPlan {
                name: api.name.clone(),
                prefix: api.prefix.clone(),
            })
            .collect(),
        ..RouteTable::default()
    };
    let graph = std::sync::Arc::clone(jadpo_check::graph::graph(db, project));
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
        let run = if route.run.query {
            let Some(query) = queries.iter().find(|query| query.name == route.run.name) else {
                continue;
            };
            Run::Query(Box::new(query.read.clone()))
        } else {
            let Some(id) = jadpo_check::graph::declaration_key(&lookup, &route.run.name)
                .and_then(|key| callable(&key))
            else {
                continue;
            };
            Run::Action(id)
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
            args,
            file: group.path.clone(),
            line,
        });
    }
    out.codecs = codecs.codecs;
    out
}

/// Numbers the codecs of the types the routes use, once each.
struct Codecs<'t> {
    types: &'t mut Types,
    codecs: Vec<Codec>,
    ids: HashMap<TypeRef, CodecId>,
}

/// What a type is made of, for its codec.
enum Made {
    Scalar,
    Object(Key),
    Enum(Key),
    StdEnum(String),
    List(TypeRef),
    Set(TypeRef),
    Map(TypeRef, TypeRef),
    Page(TypeRef),
    Unknown,
}

impl Codecs<'_> {
    fn codec(&mut self, ty: &TypeRef) -> CodecId {
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
                Codec::Object { shape, fields }
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
            Made::StdEnum(name) => Codec::Enum {
                variants: jadpo_std::enums()
                    .into_iter()
                    .find(|(written, _)| *written == name)
                    .map(|(_, variants)| variants)
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, variant)| VariantCodec {
                        name: variant.to_string(),
                        ordinal: ordinal as u32,
                        fields: None,
                    })
                    .collect(),
                ty: name,
            },
            Made::List(item) => Codec::List {
                item: self.codec(&item),
                validator: self.types.validator_id(ty),
            },
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
            Made::Unknown => self.made_unknown(),
        }
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
                TypeRef::Std(name, args) if args.is_empty() => return Made::StdEnum(name),
                _ => return Made::Unknown,
            }
        }
        Made::Unknown
    }
}
