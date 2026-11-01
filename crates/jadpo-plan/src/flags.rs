//! The flags plan (area 16, "Feature flags"; chunk B9): each flag `flags
//! { }` declares as `jadpo-std`'s plain data, numbered in name order, with
//! the enum its values carry, so both executors read a flag through the
//! runtime's one path.

use jadpo_check::flags::flags;
use jadpo_db::{Db, Project};
use jadpo_std::flags::{FlagPlan, FlagsPlan};
use jadpo_std::http::Codec;

use crate::routes::Codecs;
use crate::types::Types;

/// The flags of `project`, and the store their state is kept in.
pub(crate) fn plan(db: &dyn Db, project: Project, types: &mut Types) -> FlagsPlan {
    let model = flags(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let mut codecs = Codecs::new(types);
    // Each enum flag's codec, read back for the name its values carry.
    let ids: Vec<_> = model
        .flags
        .iter()
        .map(|flag| {
            flag.enum_type
                .as_ref()
                .map(|_| codecs.codec(&jadpo_check::graph::type_ref(&lookup, &flag.ty)))
        })
        .collect();
    let codecs = codecs.into_codecs();
    let flags = model
        .flags
        .iter()
        .zip(ids)
        .map(|(flag, id)| FlagPlan {
            name: flag.name.clone(),
            enum_type: id.and_then(|id| match codecs.get(id.0 as usize) {
                Some(Codec::Enum { ty, .. }) => Some(ty.clone()),
                _ => flag.enum_type.clone(),
            }),
            values: flag.values.clone(),
            default: flag.default,
            client: flag.client,
        })
        .collect();
    FlagsPlan {
        flags,
        store: jadpo_check::flags::store(db, project),
    }
}

/// The number of flag `name`, its place in name order.
pub(crate) fn number(db: &dyn Db, project: Project, name: &str) -> Option<u32> {
    flags(db, project)
        .flags
        .iter()
        .position(|flag| flag.name == name)
        .and_then(|at| u32::try_from(at).ok())
}
