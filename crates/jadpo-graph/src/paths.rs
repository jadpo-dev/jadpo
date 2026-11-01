//! Path numbers (docs/compiler-runtime.md §2.3, "Bodies", and §2.5,
//! "Differential test"; area 16, "Path IDs"): each way through a typed
//! body numbered when it is compiled, Ball–Larus style, so an executor
//! adds a known increment as it takes a branch and ends a run with the
//! number of the way it went.
//!
//! A body is cut into regions. The body itself is region 0; the body of
//! each `for` and of each inline function is a region of its own, keyed by
//! its site plus one (the `for` statement's, the inline function's body's),
//! numbered from its start for each turn it runs, so a loop records which
//! ways its turns took rather than one number per count of turns. Within a
//! region a `for` or an inline function is one step, which a `return`
//! inside it leaves the region by.
//!
//! A region's ways are numbered from 0, each different: at each choice the
//! first way adds nothing and each later way adds how many ways run
//! through the ones before it ([`Edge`]). The choices are an `if` chain's
//! conditions (each arm's block, and the `else` or the way past the
//! chain, written or not), a `match`'s arms, an `and` or `or` whose right
//! side holds a choice of its own (worked out or not), and, for each
//! region a `return` leaves, whether the step it sits in left through it.
//! A run a failure, a fault or an assertion cuts short ends with the sum so
//! far, which names the way it went up to the site it stopped at, every
//! way to that site having its own sum.
//!
//! A region with more ways than a `u64` holds is cut after a statement:
//! there the way so far ends as a segment of its own, and the rest is
//! numbered from 0 again, from that statement on ([`Paths::cuts`]). A
//! region that can't be numbered even so (a single statement with that
//! many ways) has no increments, and records nothing but that it ran.

use std::collections::{BTreeMap, BTreeSet};

use crate::body::{ArmBody, BinaryOp, Body, BranchId, Expr, Node, SiteId, Stmt};

/// What taking a branch, leaving through a `return` or working out an
/// `and`'s right side adds to the path number of region `region`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Edge {
    pub region: u32,
    pub increment: u64,
}

/// One region: the body, or a `for`'s or an inline function's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Region {
    /// The site that names it: the `for` statement, or the inline
    /// function's body; `None` for the body itself.
    pub site: Option<SiteId>,
    /// How many ways run through it from its start to its end or its
    /// first cut; 0 when it has too many to number.
    pub ways: u64,
}

/// A body's path numbering.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Paths {
    /// Each region, by its number: 0 the body, otherwise its site's
    /// number plus one.
    pub regions: BTreeMap<u32, Region>,
    /// What taking each branch adds, by branch number; `None` for the
    /// first way of its choice, which adds nothing, and a branch of a
    /// region not numbered.
    pub branches: Vec<Option<Edge>>,
    /// Each `for` statement and inline function body whose region is
    /// numbered and has more than one way or a cut: the region its turns
    /// run, which an executor opens at each turn and closes at its end.
    pub turns: BTreeMap<SiteId, u32>,
    /// Each `return` inside a `for` or an inline function: what it adds to
    /// each region around the one it sits in as it leaves them, innermost
    /// first.
    pub returns: BTreeMap<SiteId, Vec<Edge>>,
    /// Each `and` and `or` whose right side holds a choice: what working
    /// it out adds.
    pub rights: BTreeMap<SiteId, Edge>,
    /// Each statement after which its region's way so far ends as a
    /// segment of its own: the region, and how many ways run from there.
    pub cuts: BTreeMap<SiteId, (u32, u64)>,
}

impl Paths {
    /// What taking branch `branch` adds, if anything.
    pub fn branch(&self, branch: BranchId) -> Option<Edge> {
        self.branches.get(branch.0 as usize).copied().flatten()
    }

    /// The region each turn of the `for` at `site`, or of the inline
    /// function whose body is `site`, runs, when it is opened per turn.
    pub fn turn(&self, site: SiteId) -> Option<u32> {
        self.turns.get(&site).copied()
    }

    /// What the `return` at `site` adds to each region it leaves.
    pub fn returning(&self, site: SiteId) -> &[Edge] {
        self.returns.get(&site).map_or(&[], Vec::as_slice)
    }

    /// What working out the right side of the `and` or `or` at `site`
    /// adds.
    pub fn right(&self, site: SiteId) -> Option<Edge> {
        self.rights.get(&site).copied()
    }

    /// The region whose way so far ends as a segment after statement
    /// `site`, if one does.
    pub fn cut(&self, site: SiteId) -> Option<u32> {
        self.cuts.get(&site).map(|(region, _)| *region)
    }

    /// How many ways the body has: its region's, from its start.
    pub fn ways(&self) -> u64 {
        self.regions.get(&0).map_or(0, |region| region.ways)
    }
}

/// The region a `for` statement's or an inline function body's site
/// keys.
pub fn region_of(site: SiteId) -> u32 {
    site.0.saturating_add(1)
}

/// `body`'s path numbering.
pub fn number(body: &Body) -> Paths {
    let mut numbering = Numbering {
        body,
        edits: Vec::new(),
        returns: vec![BTreeSet::new()],
    };
    let ways = numbering.seq(&body.block, 1, 0).unwrap_or_else(|Overflow| {
        numbering.edits.clear();
        0
    });
    numbering
        .edits
        .push(Edit::Region(0, Region { site: None, ways }));
    let mut paths = Paths {
        branches: vec![None; body.branches.len()],
        ..Paths::default()
    };
    for edit in numbering.edits {
        match edit {
            Edit::Branch(branch, edge) => {
                if let Some(slot) = paths.branches.get_mut(branch.0 as usize) {
                    *slot = Some(edge);
                }
            }
            Edit::Return(site, edge) => paths.returns.entry(site).or_default().push(edge),
            Edit::Right(site, edge) => {
                paths.rights.insert(site, edge);
            }
            Edit::Cut(site, region, ways) => {
                paths.cuts.insert(site, (region, ways));
            }
            Edit::Turn(site, region) => {
                paths.turns.insert(site, region);
            }
            Edit::Region(number, region) => {
                paths.regions.insert(number, region);
            }
        }
    }
    paths
}

/// More ways than a `u64` holds.
struct Overflow;

/// How many ways run from a point to its region's end, or its next cut.
type Ways = Result<u64, Overflow>;

/// One decision of the numbering, kept in order so a statement numbered
/// again, after a cut is placed, takes back what it decided before.
enum Edit {
    Branch(BranchId, Edge),
    Return(SiteId, Edge),
    Right(SiteId, Edge),
    Cut(SiteId, u32, u64),
    Turn(SiteId, u32),
    Region(u32, Region),
}

struct Numbering<'b> {
    body: &'b Body,
    edits: Vec<Edit>,
    /// For each region being numbered, innermost last: the `return`
    /// statements inside it, in it or in a region inside it.
    returns: Vec<BTreeSet<SiteId>>,
}

impl<'b> Numbering<'b> {
    fn node(&self, id: SiteId) -> Option<&'b Node> {
        self.body.sites.get(id.0 as usize).map(|site| &site.node)
    }

    /// Records that taking `branch` adds `increment` in `region`.
    fn branch(&mut self, branch: BranchId, region: u32, increment: u64) {
        if increment > 0 {
            self.edits
                .push(Edit::Branch(branch, Edge { region, increment }));
        }
    }

    /// The ways from the start of `stmts` in `region`, `next` running on
    /// from their end. Numbered from the last statement back, each one's
    /// ways those of what follows it; where that passes a `u64`, a cut
    /// after the statement starts what follows as a segment of its own.
    fn seq(&mut self, stmts: &[SiteId], next: u64, region: u32) -> Ways {
        let mut ways = next;
        for &stmt in stmts.iter().rev() {
            let mark = self.edits.len();
            ways = match self.stmt(stmt, ways, region) {
                Ok(through) => through,
                Err(Overflow) if ways > 1 => {
                    self.edits.truncate(mark);
                    let through = self.stmt(stmt, 1, region)?;
                    self.edits.push(Edit::Cut(stmt, region, ways));
                    through
                }
                Err(Overflow) => return Err(Overflow),
            };
        }
        Ok(ways)
    }

    fn opt(&mut self, id: Option<SiteId>, next: u64, region: u32) -> Ways {
        match id {
            Some(id) => self.expr(id, next, region),
            None => Ok(next),
        }
    }

    fn stmt(&mut self, id: SiteId, next: u64, region: u32) -> Ways {
        let Some(Node::Stmt(stmt)) = self.node(id) else {
            return self.expr(id, next, region);
        };
        match stmt {
            Stmt::Var { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::Emit { value }
            | Stmt::Assert { value, .. }
            | Stmt::Expr { value } => self.opt(*value, next, region),
            // A `return` ends its region's way; it leaves the regions
            // around it through the step it sits in.
            Stmt::Return { value } => {
                if let Some(inside) = self.returns.last_mut() {
                    inside.insert(id);
                }
                self.opt(*value, 1, region)
            }
            // At each condition the way on to the next comes first, so the
            // `else`, or the way past the chain, adds nothing.
            Stmt::If {
                arms, otherwise, ..
            } => {
                let mut ways = self.seq(otherwise, next, region)?;
                for arm in arms.iter().rev() {
                    let then = self.seq(&arm.then, next, region)?;
                    self.branch(arm.branch, region, ways);
                    let after = ways.checked_add(then).ok_or(Overflow)?;
                    ways = self.opt(arm.condition, after, region)?;
                }
                Ok(ways)
            }
            Stmt::For { list, body, .. } => {
                let inner = region_of(id);
                let returns = self.region(inner, id, |numbering| numbering.seq(body, 1, inner));
                let after = self.leave_through(&returns, next, region)?;
                self.opt(*list, after, region)
            }
        }
    }

    fn expr(&mut self, id: SiteId, next: u64, region: u32) -> Ways {
        let Some(Node::Expr(expr)) = self.node(id) else {
            return self.stmt(id, next, region);
        };
        match expr {
            Expr::Match { subject, arms, .. } => {
                let mut ways: u64 = 0;
                for arm in arms {
                    let through = match &arm.body {
                        ArmBody::Value(value) => self.expr(*value, next, region)?,
                        ArmBody::Block(block) => self.seq(block, next, region)?,
                        ArmBody::Empty => next,
                    };
                    self.branch(arm.branch, region, ways);
                    ways = ways.checked_add(through).ok_or(Overflow)?;
                }
                self.opt(*subject, ways.max(1), region)
            }
            Expr::Inline {
                body: Some(inner), ..
            } => {
                let (inner, number) = (*inner, region_of(*inner));
                let returns =
                    self.region(number, inner, |numbering| numbering.expr(inner, 1, number));
                self.leave_through(&returns, next, region)
            }
            // The right side is a choice of its own only when it holds
            // one: worked out, after the way that skips it.
            Expr::Binary {
                op: BinaryOp::And | BinaryOp::Or,
                lhs,
                rhs,
            } => {
                let (lhs, rhs) = (*lhs, *rhs);
                let right = self.expr(rhs, next, region)?;
                if right == next {
                    return self.expr(lhs, next, region);
                }
                self.edits.push(Edit::Right(
                    id,
                    Edge {
                        region,
                        increment: next,
                    },
                ));
                let both = next.checked_add(right).ok_or(Overflow)?;
                self.expr(lhs, both, region)
            }
            _ => {
                let mut ways = next;
                for child in children(expr).into_iter().rev() {
                    ways = self.expr(child, ways, region)?;
                }
                Ok(ways)
            }
        }
    }

    /// Numbers region `number`, named by `site`, with `inner`: the
    /// `return` statements inside it.
    fn region(
        &mut self,
        number: u32,
        site: SiteId,
        inner: impl FnOnce(&mut Self) -> Ways,
    ) -> BTreeSet<SiteId> {
        self.returns.push(BTreeSet::new());
        let mark = self.edits.len();
        let ways = inner(self);
        let returns = self.returns.pop().unwrap_or_default();
        let ways = match ways {
            Ok(ways) => {
                let cut = self.edits[mark..]
                    .iter()
                    .any(|edit| matches!(edit, Edit::Cut(_, region, _) if *region == number));
                if ways > 1 || cut {
                    self.edits.push(Edit::Turn(site, number));
                }
                ways
            }
            Err(Overflow) => {
                self.edits.truncate(mark);
                0
            }
        };
        self.edits.push(Edit::Region(
            number,
            Region {
                site: Some(site),
                ways,
            },
        ));
        returns
    }

    /// The ways from a `for` or an inline function in `region`, `next`
    /// running on after it: one more when a `return` inside it can leave
    /// the region through it, which then adds `next`.
    fn leave_through(&mut self, returns: &BTreeSet<SiteId>, next: u64, region: u32) -> Ways {
        if returns.is_empty() {
            return Ok(next);
        }
        for &site in returns {
            self.edits.push(Edit::Return(
                site,
                Edge {
                    region,
                    increment: next,
                },
            ));
        }
        if let Some(around) = self.returns.last_mut() {
            around.extend(returns.iter().copied());
        }
        next.checked_add(1).ok_or(Overflow)
    }
}

/// The sites an expression works out before itself, in the order it works
/// them out: never an inline function's body, which its operation runs,
/// nor the fields of a failure an `attempt` or a `conflict:` arm raises,
/// which are worked out only on the way out.
fn children(expr: &Expr) -> Vec<SiteId> {
    let mut out = Vec::new();
    match expr {
        Expr::Field { receiver, .. } => out.push(*receiver),
        Expr::Call { args, .. } => out.extend(args.iter().flatten().copied()),
        Expr::Build { fields, .. } | Expr::Reject { fields, .. } => {
            out.extend(fields.iter().map(|(_, site)| *site));
        }
        Expr::List { items } => out.extend(items.iter().copied()),
        Expr::Binary { lhs, rhs, .. } => out.extend([*lhs, *rhs]),
        Expr::Unary { operand, .. } => out.push(*operand),
        Expr::Match { subject, .. } => out.extend(*subject),
        Expr::Attempt { inner, .. } => out.extend(*inner),
        Expr::Write {
            target,
            set,
            patch,
            when_supplied,
            ..
        } => {
            out.extend(*target);
            out.extend(set.iter().map(|(_, site)| *site));
            out.extend(*patch);
            for (_, fields) in when_supplied {
                out.extend(fields.iter().map(|(_, site)| *site));
            }
        }
        Expr::Request {
            params,
            input,
            idempotency_key,
            caller,
            sign_in,
            strength,
            ..
        } => {
            out.extend(params.iter().copied());
            out.extend(*input);
            out.extend(*idempotency_key);
            out.extend(*caller);
            out.extend(*sign_in);
            out.extend(*strength);
        }
        Expr::Inline { .. }
        | Expr::Literal(_)
        | Expr::Local(_)
        | Expr::Stored { .. }
        | Expr::Propagate { .. }
        | Expr::Config { .. }
        | Expr::Flag { .. }
        | Expr::Constant { .. }
        | Expr::Now
        | Expr::SentEmails
        | Expr::Unchecked => {}
    }
    out
}
