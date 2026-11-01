//! Feature flags (area 16, "Feature flags"; chunk B9): how a flag's value
//! is worked out for the caller, the one path both executors read a flag
//! through ([`crate::Ctx::flag`]).
//!
//! Each program holds the state `jadpo_flags` keeps ([`State`]) in its own
//! memory, in what it started with ([`Held`]), and reads it again at most
//! every [`REFRESH_MS`] as a request or a tick begins ([`refresh`]), so a
//! change reaches it within ten seconds. A read that fails keeps the
//! state it had, or none, which gives every flag its `default:`: reading
//! a flag never fails a request. The first flag an operation reads fixes
//! the state for the rest of it, and each flag's value is worked out once
//! per operation, so one request never sees a flag change halfway through.
//!
//! The first rule that matches wins: a named user's fixed value; a share
//! of users chosen by a stable hash of the flag's name and the user's `id`
//! ([`bucket`]); then everyone's value, at first the `default:`. A service,
//! a schedule's run and a subscriber's delivery have no user, so they get
//! everyone's value.

use std::cell::RefCell;
use std::rc::Rc;

use jadpo_std::flags::{FlagPlan, FlagsPlan};
use jadpo_value::Value;

use crate::{Executor, Host, SqlValue};

/// How long a program keeps the state it read before reading it again, in
/// milliseconds: half of the ten seconds a change takes to apply.
pub const REFRESH_MS: i64 = 5_000;

/// Reads every flag's state.
pub const READ: &str = "SELECT \"flag\", \"value\", \"users\", \"shares\" FROM \"jadpo_flags\"";

/// One flag's state, as `jadpo_flags` keeps it, each value as its place
/// among the flag's values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Everyone's value; `None` for the flag's `default:`.
    pub value: Option<u32>,
    /// Each named user's fixed value, by the user's `id` in lowercase.
    pub users: Vec<(String, u32)>,
    /// Each share of users given a value, in order, as a percentage.
    pub shares: Vec<(u32, u8)>,
}

/// Every flag's state, by the flag's number in the plan; a flag with no
/// row gives everyone its `default:`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub rows: Vec<Option<Row>>,
}

impl State {
    /// The state `rows` give, read from `jadpo_flags` for `plan`'s flags:
    /// a flag no row names, and a value no flag has (a variant since
    /// removed), give the `default:`.
    pub fn read(plan: &FlagsPlan, rows: &[Vec<SqlValue>]) -> State {
        let mut state = State {
            rows: vec![None; plan.flags.len()],
        };
        for row in rows {
            let [SqlValue::Text(name), value, users, shares, ..] = row.as_slice() else {
                continue;
            };
            let Some((at, flag)) = plan.flag(name) else {
                continue;
            };
            let read = Row {
                value: match value {
                    SqlValue::Text(text) => flag.value(text),
                    _ => None,
                },
                users: json(users)
                    .and_then(|users| match users {
                        serde_json::Value::Object(users) => Some(
                            users
                                .into_iter()
                                .filter_map(|(id, value)| {
                                    Some((id.to_ascii_lowercase(), flag.value(value.as_str()?)?))
                                })
                                .collect(),
                        ),
                        _ => None,
                    })
                    .unwrap_or_default(),
                shares: json(shares)
                    .and_then(|shares| match shares {
                        serde_json::Value::Array(shares) => Some(
                            shares
                                .iter()
                                .filter_map(|share| {
                                    let [value, percent] = share.as_array()?.as_slice() else {
                                        return None;
                                    };
                                    let percent = u8::try_from(percent.as_u64()?.min(100)).ok()?;
                                    Some((flag.value(value.as_str()?)?, percent))
                                })
                                .collect(),
                        ),
                        _ => None,
                    })
                    .unwrap_or_default(),
            };
            if let Some(slot) = state.rows.get_mut(at as usize) {
                *slot = Some(read);
            }
        }
        state
    }

    /// The value flag number `at` gives the user whose `id` is `user`, or
    /// anyone else with `None`, as a place among its values.
    pub fn choose(&self, at: u32, flag: &FlagPlan, user: Option<&str>) -> u32 {
        let Some(Some(row)) = self.rows.get(at as usize) else {
            return flag.default;
        };
        if let Some(user) = user {
            let user = user.to_ascii_lowercase();
            if let Some((_, value)) = row.users.iter().find(|(id, _)| *id == user) {
                return *value;
            }
            let bucket = bucket(&flag.name, &user);
            let mut reached: u32 = 0;
            for (value, percent) in &row.shares {
                reached += u32::from(*percent);
                if u32::from(bucket) < reached {
                    return *value;
                }
            }
        }
        row.value.unwrap_or(flag.default)
    }
}

/// The JSON a column holds, when it holds any.
fn json(value: &SqlValue) -> Option<serde_json::Value> {
    match value {
        SqlValue::Json(text) | SqlValue::Text(text) => serde_json::from_str(text).ok(),
        _ => None,
    }
}

/// Where the user whose `id` is `user` falls for flag `flag`, from 0 to
/// 99: the first eight bytes of the BLAKE3 hash of the flag's name, a zero
/// byte and the id in lowercase, as a little-endian number, modulo 100.
/// The same user always falls in the same place, so raising a share from
/// 10% to 20% keeps the first 10% in, and each flag places users apart.
pub fn bucket(flag: &str, user: &str) -> u8 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(flag.as_bytes());
    hasher.update(&[0]);
    hasher.update(user.to_ascii_lowercase().as_bytes());
    let hash = hasher.finalize();
    let mut first = [0u8; 8];
    first.copy_from_slice(&hash.as_bytes()[..8]);
    u8::try_from(u64::from_le_bytes(first) % 100).unwrap_or(0)
}

/// The `id` of `caller` when it is a signed-in user, as the runtime holds
/// it: what a share and a named user are chosen by. `None` for a service
/// and for no one.
pub fn user_of(caller: Option<&crate::Caller>) -> Option<String> {
    match &caller?.value {
        Value::Record(record)
            if &*record.ty == jadpo_std::http::PrincipalKind::User.type_name() =>
        {
            match record.get("id") {
                Some(Value::Uuid(id)) => Some(id.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The value at place `at` among `flag`'s values, as code reads it.
pub fn value(flag: &FlagPlan, at: u32) -> Value {
    match &flag.enum_type {
        None => Value::Bool(at == 1),
        Some(ty) => Value::variant(
            ty,
            at,
            flag.values.get(at as usize).map_or("", String::as_str),
            Vec::new(),
        ),
    }
}

/// What a program holds of its flags: the plan's flags, and the state it
/// last read with when it read it.
#[derive(Debug, Default)]
pub struct Held {
    plan: FlagsPlan,
    state: RefCell<Kept>,
}

#[derive(Debug, Default)]
struct Kept {
    state: Rc<State>,
    /// When it was last read, or tried, on the host's clock.
    read_at: Option<i64>,
}

impl Held {
    /// What a program of `plan`'s flags starts with: no state, so every
    /// flag gives its `default:` until the first read.
    pub fn new(plan: FlagsPlan) -> Held {
        Held {
            plan,
            state: RefCell::default(),
        }
    }

    /// Flag number `at` of the plan.
    pub fn flag(&self, at: u32) -> Option<&FlagPlan> {
        self.plan.flags.get(at as usize)
    }

    /// The state as last read: what an operation's first flag fixes.
    pub fn state(&self) -> Rc<State> {
        Rc::clone(&self.state.borrow().state)
    }

    /// Whether the state is due to be read at `now`: never read, or read
    /// [`REFRESH_MS`] or more ago, or the clock went back.
    fn due(&self, now: i64) -> bool {
        match self.state.borrow().read_at {
            None => true,
            Some(read_at) => now < read_at || now - read_at >= REFRESH_MS,
        }
    }
}

/// Reads every flag's state again for `executor`'s program on `host` when
/// it is due at `now`: as a request or a tick begins. A read that fails
/// keeps what was held, and the next is tried [`REFRESH_MS`] later.
pub async fn refresh(executor: &dyn Executor, host: &dyn Host, now: i64) {
    let Some(started) = executor.started() else {
        return;
    };
    let held = started.flags();
    let Some(store) = held.plan.store.as_deref() else {
        return;
    };
    if held.plan.flags.is_empty() || !held.due(now) {
        return;
    }
    // Marked before the read, so the requests that begin while it waits
    // don't each read too.
    held.state.borrow_mut().read_at = Some(now);
    let Some(database) = host.database(store) else {
        return;
    };
    if let Ok(rows) = database.query_once(READ, &[]).await {
        held.state.borrow_mut().state = Rc::new(State::read(&held.plan, &rows));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> FlagsPlan {
        FlagsPlan {
            flags: vec![
                FlagPlan {
                    name: "checkout_layout".into(),
                    enum_type: Some("CheckoutLayout".into()),
                    values: vec!["classic".into(), "compact".into(), "wide".into()],
                    default: 0,
                },
                FlagPlan {
                    name: "new_checkout".into(),
                    enum_type: None,
                    values: vec!["false".into(), "true".into()],
                    default: 0,
                },
            ],
            store: Some("main".into()),
        }
    }

    fn row(
        name: &str,
        value: Option<&str>,
        users: Option<&str>,
        shares: Option<&str>,
    ) -> Vec<SqlValue> {
        let text = |text: Option<&str>| match text {
            Some(text) => SqlValue::Text(text.into()),
            None => SqlValue::Null(jadpo_std::SqlType::Text),
        };
        let json = |text: Option<&str>| match text {
            Some(text) => SqlValue::Json(text.into()),
            None => SqlValue::Null(jadpo_std::SqlType::Json),
        };
        vec![
            SqlValue::Text(name.into()),
            text(value),
            json(users),
            json(shares),
        ]
    }

    /// Ids of users, numbered, as the runtime holds a user's `id`.
    fn user(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    #[test]
    fn a_flag_with_no_row_gives_everyone_its_default() {
        let plan = plan();
        let state = State::read(&plan, &[]);
        for (at, flag) in plan.flags.iter().enumerate() {
            assert_eq!(state.choose(at as u32, flag, Some(&user(1))), flag.default);
            assert_eq!(state.choose(at as u32, flag, None), flag.default);
        }
    }

    #[test]
    fn a_named_user_gets_their_value_before_any_share_and_everyone_else_the_rest() {
        let plan = plan();
        let users = format!(r#"{{"{}": "true"}}"#, user(1).to_uppercase());
        let state = State::read(
            &plan,
            &[row(
                "new_checkout",
                Some("false"),
                Some(&users),
                Some(r#"[["false", 100]]"#),
            )],
        );
        let flag = &plan.flags[1];
        assert_eq!(state.choose(1, flag, Some(&user(1))), 1, "the named user");
        assert_eq!(
            state.choose(1, flag, Some(&user(2))),
            0,
            "a user in the share"
        );
        assert_eq!(state.choose(1, flag, None), 0, "no user");
        let everyone = State::read(&plan, &[row("new_checkout", Some("true"), None, None)]);
        assert_eq!(everyone.choose(1, flag, Some(&user(2))), 1);
        assert_eq!(everyone.choose(1, flag, None), 1, "the system and services");
    }

    #[test]
    fn a_share_is_stable_grows_by_keeping_its_users_and_differs_per_flag() {
        let plan = plan();
        let flag = &plan.flags[1];
        let at = |percent: u8| {
            State::read(
                &plan,
                &[row(
                    "new_checkout",
                    None,
                    None,
                    Some(&format!(r#"[["true", {percent}]]"#)),
                )],
            )
        };
        let (ten, twenty) = (at(10), at(20));
        let users: Vec<String> = (0..2000).map(user).collect();
        let on = |state: &State| -> Vec<bool> {
            users
                .iter()
                .map(|id| state.choose(1, flag, Some(id)) == 1)
                .collect()
        };
        let (in_ten, in_twenty) = (on(&ten), on(&twenty));
        let count = |on: &[bool]| on.iter().filter(|on| **on).count();
        // About a tenth and a fifth of 2,000 users.
        assert!((150..250).contains(&count(&in_ten)), "{}", count(&in_ten));
        assert!(
            (330..470).contains(&count(&in_twenty)),
            "{}",
            count(&in_twenty)
        );
        // Raising 10% to 20% keeps the first 10% in.
        assert!(
            in_ten
                .iter()
                .zip(&in_twenty)
                .all(|(ten, twenty)| !ten || *twenty)
        );
        // The same user always gets the same answer.
        assert_eq!(on(&at(10)), in_ten);
        // Another flag picks a different 10%.
        let other: Vec<bool> = users
            .iter()
            .map(|id| bucket("checkout_layout", id) < 10)
            .collect();
        assert_ne!(other, in_ten);
        // Nobody at 0%, everybody at 100%.
        assert_eq!(count(&on(&at(0))), 0);
        assert_eq!(count(&on(&at(100))), users.len());
    }

    #[test]
    fn a_split_gives_each_variant_its_share_and_an_unknown_value_is_ignored() {
        let plan = plan();
        let flag = &plan.flags[0];
        let state = State::read(
            &plan,
            &[
                row(
                    "checkout_layout",
                    Some("narrow"),
                    Some(&format!(r#"{{"{}": "gone"}}"#, user(7))),
                    Some(r#"[["classic", 50], ["compact", 50]]"#),
                ),
                row("retired_flag", Some("true"), None, None),
            ],
        );
        let mut seen = [0usize; 3];
        for n in 0..1000 {
            seen[state.choose(0, flag, Some(&user(n))) as usize] += 1;
        }
        assert!(seen[0] > 400 && seen[1] > 400 && seen[2] == 0, "{seen:?}");
        // No user, and a value no variant has, give the default.
        assert_eq!(state.choose(0, flag, None), 0);
        assert_eq!(
            value(flag, 1),
            Value::variant("CheckoutLayout", 1, "compact", Vec::new())
        );
        assert_eq!(value(&plan.flags[1], 1), Value::Bool(true));
    }

    #[test]
    fn a_held_state_is_read_again_only_when_due() {
        let held = Held::new(plan());
        assert!(held.due(0));
        held.state.borrow_mut().read_at = Some(1_000);
        assert!(!held.due(1_000 + REFRESH_MS - 1));
        assert!(held.due(1_000 + REFRESH_MS));
        assert!(held.due(999), "a clock that went back");
    }
}
