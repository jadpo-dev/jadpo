//! Fuzzed function bodies (docs/compiler-runtime.md §2.5, "Differential
//! test"; roadmap chunk 9.3): well-typed functions generated from the
//! language's own constructs, each called by tests with generated inputs,
//! for the differential run to run interpreted and compiled and compare by
//! the same records, as Cranelift compares its own generated functions
//! against its interpreter.
//!
//! A seed decides everything ([`project`]), so a run is reproduced from
//! its seed and how many bodies it made. The bodies use named numbers,
//! booleans, text, an enum and lists of numbers; `var`, `var mut` and
//! assignment, `if` chains, `for`, `match` on values and on a call's
//! outcome, `return` early, `reject`, `attempt` and `propagate`; the
//! operators, among them those that overflow; inline
//! functions given to `list.map`, `list.filter`, `list.any` and
//! `list.all`; and calls to the bodies made before, some of which fail.
//! Every one checks: the generator keeps the language's rules (every
//! value named by a declared type, every variable read, nothing after a
//! `return`, a divisor that isn't zero), and `crates/jadpo-test/tests/fuzz.rs`
//! holds it to them.

use std::fmt::Write as _;

/// The seed the gate's bounded run uses, unless `JADPO_FUZZ_SEED` gives
/// another.
pub const SEED: u64 = 1;

/// How many bodies the gate's bounded run makes, unless
/// `JADPO_FUZZ_BODIES` asks for more.
pub const BODIES: usize = 40;

/// How many bodies one project holds; a longer run makes several.
pub const PER_PROJECT: usize = 40;

/// The run asked for: its seed and how many bodies, from
/// `JADPO_FUZZ_SEED` and `JADPO_FUZZ_BODIES` (`tools/verify.py --fuzz`),
/// [`SEED`] and [`BODIES`] by default.
pub fn asked() -> (u64, usize) {
    let read = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
    };
    let seed = read("JADPO_FUZZ_SEED").unwrap_or(SEED);
    let bodies = read("JADPO_FUZZ_BODIES")
        .and_then(|bodies| usize::try_from(bodies).ok())
        .unwrap_or(BODIES);
    (seed, bodies)
}

/// The projects a run of `bodies` bodies from `seed` makes, each with its
/// name and files: [`PER_PROJECT`] bodies a project, the `n`th from seed
/// `seed + n`.
pub fn made(seed: u64, bodies: usize) -> Vec<(String, Vec<(String, String)>)> {
    (0..bodies.div_ceil(PER_PROJECT))
        .map(|n| {
            let count = (bodies - n * PER_PROJECT).min(PER_PROJECT);
            let seed = seed.wrapping_add(n as u64);
            (
                format!("fuzzed bodies, seed {seed}, {count} bodies"),
                project(seed, count),
            )
        })
        .collect()
}

/// A loose project of `bodies` functions generated from `seed`, each
/// called by two tests with generated inputs: `main.jadpo` and
/// `main.test.jadpo`.
pub fn project(seed: u64, bodies: usize) -> Vec<(String, String)> {
    let mut rng = Rng(seed ^ 0x9e37_79b9_7f4a_7c15);
    let mut source = String::from(TYPES);
    let mut tests = String::new();
    let mut made: Vec<Signature> = Vec::new();
    for n in 0..bodies {
        let signature = Signature::draw(&mut rng, n);
        let text = Function::new(&mut rng, &made, &signature).text();
        source.push('\n');
        source.push_str(&text);
        for run in 1..=2 {
            tests.push_str(&test(&mut rng, &signature, run));
        }
        made.push(signature);
    }
    vec![
        ("main.jadpo".to_string(), source),
        ("main.test.jadpo".to_string(), tests),
    ]
}

/// The types every body draws on.
const TYPES: &str = "type Drawn = Int {}\n\ntype Flag = Bool {}\n\ntype Word = Text {}\n\ntype Shade = Enum {\n    red\n    green\n    blue\n}\n\ntype Refused = Failure {\n    kind: FailureKind.rejected\n}\n";

/// SplitMix64: small, and the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`, which is at least 1.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    /// True `percent` times in a hundred.
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    Count,
    Flag,
    Word,
    Shade,
    Counts,
}

const TYPES_DRAWN: [Ty; 5] = [Ty::Count, Ty::Flag, Ty::Word, Ty::Shade, Ty::Counts];

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::Count => "Drawn",
            Ty::Flag => "Flag",
            Ty::Word => "Word",
            Ty::Shade => "Shade",
            Ty::Counts => "List<Drawn>",
        }
    }

    /// What a local of the type is called, before its number.
    fn stem(self) -> &'static str {
        match self {
            Ty::Count => "amount",
            Ty::Flag => "flag",
            Ty::Word => "word",
            Ty::Shade => "shade",
            Ty::Counts => "amounts",
        }
    }
}

/// A generated function's name and signature.
#[derive(Clone, Debug)]
struct Signature {
    name: String,
    params: Vec<Ty>,
    result: Ty,
    fails: bool,
}

impl Signature {
    fn draw(rng: &mut Rng, n: usize) -> Signature {
        let count = 1 + rng.below(3);
        Signature {
            name: format!("fuzzed_{n}"),
            params: (0..count).map(|_| *rng.pick(&TYPES_DRAWN)).collect(),
            result: *rng.pick(&TYPES_DRAWN),
            fails: rng.chance(30),
        }
    }
}

/// A local in scope.
#[derive(Clone, Debug)]
struct Local {
    name: String,
    ty: Ty,
    mutable: bool,
    used: bool,
}

/// One function's body as it is generated.
struct Function<'g> {
    rng: &'g mut Rng,
    /// The functions made before, which it may call.
    callable: &'g [Signature],
    me: &'g Signature,
    /// The locals in scope, a block's after its parent's, outermost first.
    scopes: Vec<Vec<Local>>,
    next: usize,
    out: String,
    indent: usize,
}

/// The deepest an expression nests.
const DEPTH: usize = 3;

/// The deepest blocks nest.
const NESTING: usize = 3;

impl<'g> Function<'g> {
    fn new(rng: &'g mut Rng, callable: &'g [Signature], me: &'g Signature) -> Function<'g> {
        Function {
            rng,
            callable,
            me,
            scopes: Vec::new(),
            next: 0,
            out: String::new(),
            indent: 1,
        }
    }

    fn text(mut self) -> String {
        let params: Vec<Local> = self
            .me
            .params
            .clone()
            .into_iter()
            .map(|ty| self.local(ty, false))
            .collect();
        let header = params
            .iter()
            .map(|param| format!("{}: {}", param.name, param.ty.name()))
            .collect::<Vec<_>>()
            .join(", ");
        let fails = if self.me.fails { " fails Refused" } else { "" };
        let mut text = format!(
            "/// A generated body.\nfunction {}({header}){fails} -> {} {{\n",
            self.me.name,
            self.me.result.name()
        );
        self.scopes.push(params);
        // Every body keeps a running count its blocks add their locals
        // to, so each variable is read.
        self.line("var mut tally: Drawn = 0");
        self.scopes[0].push(Local {
            name: "tally".into(),
            ty: Ty::Count,
            mutable: true,
            used: false,
        });
        if self.me.fails {
            let condition = self.condition(1);
            self.line(&format!("if ({condition}) {{"));
            self.indent += 1;
            self.line("reject Refused");
            self.indent -= 1;
            self.line("}");
        }
        let count = 2 + self.rng.below(4);
        for _ in 0..count {
            self.stmt(1);
        }
        self.settle();
        let result = self.result_expr();
        self.line(&format!("return {result}"));
        text.push_str(&self.out);
        text.push_str("}\n");
        text
    }

    /// The returned value, which reads the tally.
    fn result_expr(&mut self) -> String {
        self.read("tally");
        match self.me.result {
            Ty::Count => {
                let value = self.expr(Ty::Count, 2);
                format!("Drawn(tally + {value})")
            }
            Ty::Flag => {
                let value = self.expr(Ty::Count, 2);
                format!("Flag(tally > {value})")
            }
            ty => {
                let (one, other) = (self.arm(ty, 2), self.arm(ty, 2));
                format!(
                    "match tally > 3 {{\n{}true -> {one}\n{}false -> {other}\n{}}}",
                    self.pad(1),
                    self.pad(1),
                    self.pad(0)
                )
            }
        }
    }

    fn pad(&self, more: usize) -> String {
        "    ".repeat(self.indent + more)
    }

    fn line(&mut self, text: &str) {
        let pad = self.pad(0);
        let _ = writeln!(self.out, "{pad}{text}");
    }

    /// A new local of type `ty`, not yet in scope.
    fn local(&mut self, ty: Ty, mutable: bool) -> Local {
        self.next += 1;
        Local {
            name: format!("{}_{}", ty.stem(), self.next),
            ty,
            mutable,
            used: false,
        }
    }

    fn declare(&mut self, local: Local) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(local);
        }
    }

    /// Marks the local called `name` read.
    fn read(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(local) = scope.iter_mut().find(|local| local.name == name) {
                local.used = true;
                return;
            }
        }
    }

    /// The locals of type `ty` in scope, `mutable` ones only when asked.
    fn in_scope(&self, ty: Ty, mutable: bool) -> Vec<String> {
        self.scopes
            .iter()
            .flatten()
            .filter(|local| local.ty == ty && (!mutable || local.mutable))
            .map(|local| local.name.clone())
            .collect()
    }

    /// Reads every local of the innermost block not read yet, adding it to
    /// the tally: what keeps every variable read.
    fn settle(&mut self) {
        let unread: Vec<Local> = self
            .scopes
            .last()
            .map(|scope| scope.iter().filter(|local| !local.used).cloned().collect())
            .unwrap_or_default();
        for local in unread {
            self.read(&local.name);
            self.read("tally");
            let name = &local.name;
            match local.ty {
                Ty::Count => self.line(&format!("tally = Drawn(tally + {name})")),
                Ty::Flag => {
                    self.line(&format!("if ({name}) {{"));
                    self.indent += 1;
                    self.line("tally = Drawn(tally + 1)");
                    self.indent -= 1;
                    self.line("}");
                }
                Ty::Word => self.line(&format!("tally = Drawn(tally + text.length({name}))")),
                Ty::Shade => {
                    self.line(&format!("if ({name} == Shade.red) {{"));
                    self.indent += 1;
                    self.line("tally = Drawn(tally + 1)");
                    self.indent -= 1;
                    self.line("}");
                }
                Ty::Counts => self.line(&format!("tally = Drawn(tally + list.count({name}))")),
            }
        }
    }

    /// A block's statements, its own locals in a scope of their own,
    /// `ends` its last statement when given.
    fn block(&mut self, nesting: usize, locals: Vec<Local>, ends: Option<String>) {
        self.indent += 1;
        self.scopes.push(locals);
        let count = if nesting >= NESTING {
            1
        } else {
            1 + self.rng.below(3)
        };
        for _ in 0..count {
            self.stmt(nesting + 1);
        }
        self.settle();
        if let Some(ends) = ends {
            self.line(&ends);
        }
        self.scopes.pop();
        self.indent -= 1;
    }

    fn stmt(&mut self, nesting: usize) {
        let deep = nesting >= NESTING;
        let roll = self.rng.below(100);
        match roll {
            0..=24 => {
                let ty = *self.rng.pick(&TYPES_DRAWN);
                let mutable = self.rng.chance(40);
                let value = self.expr(ty, DEPTH);
                let local = self.local(ty, mutable);
                let word = if mutable { "var mut" } else { "var" };
                self.line(&format!("{word} {}: {} = {value}", local.name, ty.name()));
                self.declare(local);
            }
            25..=39 => {
                let ty = *self.rng.pick(&TYPES_DRAWN);
                let targets = self.in_scope(ty, true);
                let target = if targets.is_empty() {
                    "tally".to_string()
                } else {
                    self.rng.pick(&targets).clone()
                };
                let ty = if targets.is_empty() { Ty::Count } else { ty };
                let value = self.expr(ty, DEPTH);
                self.line(&format!("{target} = {value}"));
            }
            40..=59 if !deep => self.if_chain(nesting),
            60..=71 if !deep => {
                let list = self.expr(Ty::Counts, 2);
                let item = self.local(Ty::Count, false);
                self.line(&format!("for {} in {list} {{", item.name));
                self.block(nesting, vec![item], None);
                self.line("}");
            }
            72..=81 if !deep => {
                let subject = self.subject(Ty::Shade);
                self.line(&format!("match {subject} {{"));
                self.indent += 1;
                for variant in ["red", "green", "blue"] {
                    self.line(&format!("Shade.{variant} -> {{"));
                    self.block(nesting, Vec::new(), None);
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
            }
            82..=89 => {
                let condition = self.condition(2);
                let value = self.expr(self.me.result, 2);
                self.line(&format!("if ({condition}) {{"));
                self.indent += 1;
                self.line(&format!("return {value}"));
                self.indent -= 1;
                self.line("}");
            }
            90..=94 if self.me.fails => {
                let condition = self.condition(2);
                self.line(&format!("if ({condition}) {{"));
                self.indent += 1;
                self.line("reject Refused");
                self.indent -= 1;
                self.line("}");
            }
            90..=99 if self.me.fails => {
                let failing: Vec<Signature> =
                    self.callable.iter().filter(|f| f.fails).cloned().collect();
                if failing.is_empty() {
                    return self.stmt(nesting);
                }
                let called = self.rng.pick(&failing).clone();
                let call = self.call(&called);
                let local = self.local(called.result, false);
                self.line(&format!(
                    "var {}: {} = attempt {call}",
                    local.name,
                    called.result.name()
                ));
                self.declare(local);
            }
            _ => {
                // Something to the tally.
                self.read("tally");
                let value = self.expr(Ty::Count, DEPTH);
                self.line(&format!("tally = Drawn(tally + {value})"));
            }
        }
    }

    fn if_chain(&mut self, nesting: usize) {
        let condition = self.condition(2);
        self.line(&format!("if ({condition}) {{"));
        self.block(nesting, Vec::new(), None);
        for _ in 0..self.rng.below(3) {
            let condition = self.condition(2);
            self.line(&format!("}} else if ({condition}) {{"));
            self.block(nesting, Vec::new(), None);
        }
        if self.rng.chance(50) {
            self.line("} else {");
            self.block(nesting, Vec::new(), None);
        }
        self.line("}");
    }

    /// A call of `called` with an argument of each parameter's type.
    fn call(&mut self, called: &Signature) -> String {
        let args: Vec<String> = called
            .params
            .clone()
            .into_iter()
            .map(|ty| self.expr(ty, 1))
            .collect();
        format!("{}({})", called.name, args.join(", "))
    }

    /// A local of type `ty` in scope, read, if there is one.
    fn some_local(&mut self, ty: Ty) -> Option<String> {
        let found = self.in_scope(ty, false);
        if found.is_empty() {
            return None;
        }
        let name = self.rng.pick(&found).clone();
        self.read(&name);
        Some(name)
    }

    /// What a `match` chooses by: a local when there is one, so the way
    /// it goes depends on the inputs.
    fn subject(&mut self, ty: Ty) -> String {
        match self.rng.chance(90).then(|| self.some_local(ty)).flatten() {
            Some(local) => local,
            None => self.expr(ty, 1),
        }
    }

    /// A condition: a plain `Bool`.
    fn condition(&mut self, depth: usize) -> String {
        if depth == 0 {
            let flag = self.rng.chance(50);
            return match flag.then(|| self.some_local(Ty::Flag)).flatten() {
                Some(flag) => format!("{flag} == true"),
                None => {
                    let (a, b) = (self.expr(Ty::Count, 0), self.expr(Ty::Count, 0));
                    format!("{} > {}", operand(&a), operand(&b))
                }
            };
        }
        match self.rng.below(12) {
            0 | 1 => {
                let (a, b) = (
                    self.expr(Ty::Count, depth - 1),
                    self.expr(Ty::Count, depth - 1),
                );
                let op = *self.rng.pick(&["<", "<=", ">", ">=", "==", "!="]);
                format!("{} {op} {}", operand(&a), operand(&b))
            }
            2 => {
                let a = self.expr(Ty::Word, depth - 1);
                let b = *self.rng.pick(&["\"\"", "\"a\"", "\"ab\""]);
                format!("{} == {b}", operand(&a))
            }
            3 => {
                let a = self.expr(Ty::Shade, depth - 1);
                let b = *self.rng.pick(&["red", "green", "blue"]);
                format!("{} != Shade.{b}", operand(&a))
            }
            4 => {
                let inner = self.condition(depth - 1);
                format!("not ({inner})")
            }
            5 => {
                let (a, b) = (self.condition(depth - 1), self.condition(depth - 1));
                format!("({a}) and ({b})")
            }
            6 => {
                // An `or` whose right side chooses.
                let a = self.condition(depth - 1);
                let b = self.bool_match(depth - 1);
                format!("({a}) or ({b})")
            }
            7 => {
                let list = self.expr(Ty::Counts, depth - 1);
                let op = *self.rng.pick(&["any", "all"]);
                let each = self.local(Ty::Count, false);
                self.scopes.push(vec![each.clone()]);
                let bound = self.expr(Ty::Count, 0);
                self.scopes.pop();
                let cmp = *self.rng.pick(&["<", ">", "=="]);
                format!(
                    "list.{op}({list}, {} -> {} {cmp} {})",
                    each.name,
                    each.name,
                    operand(&bound)
                )
            }
            8 => self.bool_match(depth - 1),
            9 => {
                let value = self.expr(Ty::Flag, depth - 1);
                format!("{} == true", operand(&value))
            }
            _ => match self.some_local(Ty::Flag) {
                Some(flag) => format!("{flag} == true"),
                None => self.condition(0),
            },
        }
    }

    /// A `match` giving a `Bool`.
    fn bool_match(&mut self, depth: usize) -> String {
        let subject = self.subject(Ty::Shade);
        let arms: Vec<String> = ["red", "green", "blue"]
            .iter()
            .map(|variant| {
                let value = self.condition(depth.saturating_sub(1));
                format!("Shade.{variant} -> {value}")
            })
            .collect();
        self.matched(&subject, &arms)
    }

    /// `match subject { arms }`, an arm a line.
    fn matched(&self, subject: &str, arms: &[String]) -> String {
        let inner = self.pad(1);
        let mut text = format!("match {subject} {{\n");
        for arm in arms {
            let _ = writeln!(text, "{inner}{arm}");
        }
        text.push_str(&self.pad(0));
        text.push('}');
        text
    }

    /// An expression of type `ty`, at most `depth` deep.
    fn expr(&mut self, ty: Ty, depth: usize) -> String {
        if depth == 0 || self.rng.chance(25) {
            if let Some(local) = (self.rng.chance(85)).then(|| self.some_local(ty)).flatten() {
                return local;
            }
            return self.literal(ty);
        }
        let below = depth - 1;
        // A match on a value, or on a call's outcome, or a call.
        match self.rng.below(10) {
            0 => {
                let subject = self.subject(Ty::Shade);
                let arms: Vec<String> = ["red", "green", "blue"]
                    .iter()
                    .map(|variant| {
                        let value = self.arm(ty, below);
                        format!("Shade.{variant} -> {value}")
                    })
                    .collect();
                return self.matched(&subject, &arms);
            }
            1 => {
                let subject = self.subject(Ty::Count);
                let (zero, one, rest) = (
                    self.arm(ty, below),
                    self.arm(ty, below),
                    self.arm(ty, below),
                );
                return self.matched(
                    &subject,
                    &[
                        format!("0 -> {zero}"),
                        format!("1 -> {one}"),
                        format!("_ -> {rest}"),
                    ],
                );
            }
            2 => {
                let candidates: Vec<Signature> = self
                    .callable
                    .iter()
                    .filter(|f| f.result == ty)
                    .cloned()
                    .collect();
                if !candidates.is_empty() {
                    let called = self.rng.pick(&candidates).clone();
                    let call = self.call(&called);
                    if !called.fails {
                        return call;
                    }
                    let value = self.local(ty, false);
                    let failed = if self.me.fails && self.rng.chance(40) {
                        if self.rng.chance(50) {
                            "propagate".to_string()
                        } else {
                            "reject Refused".to_string()
                        }
                    } else {
                        self.arm(ty, below)
                    };
                    return self.matched(
                        &call,
                        &[
                            format!("success({}) -> {}", value.name, value.name),
                            format!("Refused -> {failed}"),
                        ],
                    );
                }
            }
            _ => {}
        }
        match ty {
            Ty::Count => match self.rng.below(7) {
                0..=2 => {
                    let op = *self.rng.pick(&["+", "-", "*", "/", "%"]);
                    let a = self.expr(Ty::Count, below);
                    // A divisor's type excludes zero (`type.divisor_zero`),
                    // so it is a literal that isn't one; `-1` overflows the
                    // smallest number.
                    let b = if matches!(op, "/" | "%") {
                        self.rng.pick(&["2", "3", "7", "-1", "-4"]).to_string()
                    } else {
                        self.expr(Ty::Count, below)
                    };
                    format!("Drawn({} {op} {})", operand(&a), operand(&b))
                }
                3 => {
                    let a = self.expr(Ty::Count, below);
                    format!("Drawn(-{})", operand(&a))
                }
                4 => {
                    let list = self.expr(Ty::Counts, below);
                    let op = *self.rng.pick(&["count", "sum"]);
                    format!("Drawn(list.{op}({list}))")
                }
                5 => {
                    let word = self.expr(Ty::Word, below);
                    format!("Drawn(text.length({word}))")
                }
                _ => self.literal(Ty::Count),
            },
            Ty::Flag => {
                let condition = self.condition(below);
                format!("Flag({condition})")
            }
            Ty::Word => match self.rng.below(3) {
                0 => {
                    let (a, b) = (self.expr(Ty::Word, below), self.expr(Ty::Word, below));
                    format!("Word({} + {})", operand(&a), operand(&b))
                }
                1 => {
                    let a = self.expr(Ty::Word, below);
                    format!("Word(text.upper({a}))")
                }
                _ => self.literal(Ty::Word),
            },
            Ty::Shade => self.literal(Ty::Shade),
            Ty::Counts => match self.rng.below(5) {
                0 => {
                    let items: Vec<String> = (0..1 + self.rng.below(3))
                        .map(|_| self.expr(Ty::Count, below))
                        .collect();
                    format!("[{}]", items.join(", "))
                }
                1 => {
                    let list = self.expr(Ty::Counts, below);
                    let each = self.local(Ty::Count, false);
                    self.scopes.push(vec![each.clone()]);
                    let other = self.expr(Ty::Count, below);
                    self.scopes.pop();
                    let op = *self.rng.pick(&["+", "-", "*"]);
                    let body = if self.rng.chance(30) {
                        let one = self.expr(Ty::Count, 0);
                        let inner = self.pad(1);
                        let end = self.pad(0);
                        format!(
                            "match {} > {} {{\n{inner}true -> Drawn({} {op} {})\n{inner}false -> Drawn({})\n{end}}}",
                            each.name,
                            operand(&other),
                            each.name,
                            operand(&one),
                            each.name
                        )
                    } else {
                        format!("Drawn({} {op} {})", each.name, operand(&other))
                    };
                    format!("list.map({list}, {} -> {body})", each.name)
                }
                2 => {
                    let list = self.expr(Ty::Counts, below);
                    let each = self.local(Ty::Count, false);
                    self.scopes.push(vec![each.clone()]);
                    let other = self.expr(Ty::Count, 0);
                    self.scopes.pop();
                    let cmp = *self.rng.pick(&["<", ">", "!="]);
                    format!(
                        "list.filter({list}, {} -> {} {cmp} {})",
                        each.name,
                        each.name,
                        operand(&other)
                    )
                }
                3 => {
                    let list = self.expr(Ty::Counts, below);
                    let item = self.expr(Ty::Count, below);
                    format!("list.append({list}, {item})")
                }
                _ => self.literal(Ty::Counts),
            },
        }
    }

    /// A `match` arm's value of type `ty`: a literal built as the type,
    /// so every arm has the one type.
    fn arm(&mut self, ty: Ty, depth: usize) -> String {
        let value = self.expr(ty, depth);
        typed(ty, &value)
    }

    fn literal(&mut self, ty: Ty) -> String {
        match ty {
            Ty::Count => self.rng.pick(&COUNTS).to_string(),
            Ty::Flag => self.rng.pick(&["true", "false"]).to_string(),
            Ty::Word => self
                .rng
                .pick(&["\"\"", "\"a\"", "\"ab\"", "\"Zed\""])
                .to_string(),
            Ty::Shade => format!("Shade.{}", self.rng.pick(&["red", "green", "blue"])),
            Ty::Counts => {
                let items: Vec<&str> = (0..1 + self.rng.below(4))
                    .map(|_| *self.rng.pick(&COUNTS))
                    .collect();
                format!("[{}]", items.join(", "))
            }
        }
    }
}

/// `value`, of type `ty`, built as the type when it is a literal, whose
/// type would otherwise be a built-in one where nothing names another.
fn typed(ty: Ty, value: &str) -> String {
    let literal = |text: &str| {
        text.starts_with('"')
            || text.starts_with('-')
            || text.chars().next().is_some_and(|c| c.is_ascii_digit())
    };
    match ty {
        Ty::Count | Ty::Word if literal(value) => format!("{}({value})", ty.name()),
        // A list of literals only, as `literal` writes one.
        Ty::Counts
            if value.starts_with('[')
                && value[1..value.len() - 1].split(", ").all(|item| {
                    !item.is_empty() && literal(item) && !item.contains(['(', '['])
                }) =>
        {
            let items: Vec<String> = value[1..value.len() - 1]
                .split(", ")
                .map(|item| typed(Ty::Count, item))
                .collect();
            format!("[{}]", items.join(", "))
        }
        _ => value.to_string(),
    }
}

/// `text` as an operator's operand: bracketed when it starts with a sign
/// or is a `match`.
fn operand(text: &str) -> String {
    if text.starts_with('-') || text.starts_with("match ") {
        format!("({text})")
    } else {
        text.to_string()
    }
}

/// The numbers literals are drawn from: small ones, and ones whose sums
/// and products overflow.
const COUNTS: [&str; 12] = [
    "0",
    "1",
    "2",
    "3",
    "5",
    "7",
    "10",
    "100",
    "-1",
    "-4",
    "3037000500",
    "4611686018427387904",
];

/// A test calling `called` with generated inputs, its value compared with
/// one it is unlikely to be, so a failed assertion's report keeps it.
fn test(rng: &mut Rng, called: &Signature, run: usize) -> String {
    let args: Vec<String> = called.params.iter().map(|ty| input(rng, *ty)).collect();
    let call = format!("{}({})", called.name, args.join(", "));
    let got = if called.fails {
        format!(
            "match {call} {{\n        success(value) -> value\n        Refused -> {}\n    }}",
            typed(called.result, &input(rng, called.result))
        )
    } else {
        call
    };
    let unlikely = match called.result {
        Ty::Count => "-987654321".to_string(),
        Ty::Flag => "true".to_string(),
        Ty::Word => "\"unlikely\"".to_string(),
        Ty::Shade => "Shade.green".to_string(),
        Ty::Counts => "[-987654321]".to_string(),
    };
    format!(
        "test \"{} runs the same both ways, inputs {run}\" {{\n    var got: {} = {got}\n    assert got == {unlikely}\n}}\n\n",
        called.name,
        called.result.name()
    )
}

/// An input of type `ty`.
fn input(rng: &mut Rng, ty: Ty) -> String {
    match ty {
        Ty::Count => rng
            .pick(&["0", "1", "2", "3", "4", "9", "-2", "1000"])
            .to_string(),
        Ty::Flag => rng.pick(&["true", "false"]).to_string(),
        Ty::Word => rng
            .pick(&["\"\"", "\"a\"", "\"ab\"", "\"word\""])
            .to_string(),
        Ty::Shade => format!("Shade.{}", rng.pick(&["red", "green", "blue"])),
        Ty::Counts => {
            let items: Vec<&str> = (0..rng.below(5))
                .map(|_| *rng.pick(&["0", "1", "2", "3", "5", "8", "-1"]))
                .collect();
            format!("[{}]", items.join(", "))
        }
    }
}
