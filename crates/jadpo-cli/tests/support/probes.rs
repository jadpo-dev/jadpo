//! A domain of routes that probe a deployable's limits (roadmap chunk
//! 8.4a): recursions to them, through plain calls, through frames that
//! keep a good deal, and through an inline function a level; and work
//! that never waits, in loops and inline functions, against a one-second
//! deadline. A project holding it lists [`POLICY`]'s actions under its
//! root policy's `actions:` and brings its API's record up to date.

/// The domain's files, by path from the project's folder.
pub const FILES: &[(&str, &str)] = &[
    (
        "domains/probe/types/depth.jadpo",
        "/// How deep a walk goes.\ntype Depth = Int {}\n",
    ),
    (
        "domains/probe/types/label.jadpo",
        "/// A label carried down a walk.\ntype Label = Text {}\n",
    ),
    (
        "domains/probe/types/total.jadpo",
        "/// A count of work done.\ntype Total = Int {}\n",
    ),
    (
        "domains/probe/types/reach_request.jadpo",
        "use probe/types { Depth }\n\n/// How deep to walk.\ntype ReachRequest = Input {\n    depth: Depth\n}\n",
    ),
    (
        "domains/probe/types/reached.jadpo",
        "use probe/types { Depth }\n\n/// How deep the walk went.\ntype Reached = Output {\n    depth: Depth\n}\n",
    ),
    (
        "domains/probe/types/spin_request.jadpo",
        "use probe/types { Total }\n\n/// The rounds to work through.\ntype SpinRequest = Input {\n    rounds: List<Total>\n}\n",
    ),
    (
        "domains/probe/types/spun.jadpo",
        "use probe/types { Total }\n\n/// What the work counted.\ntype Spun = Output {\n    total: Total\n}\n",
    ),
    (
        "domains/probe/actions/reach.jadpo",
        r#"use probe/types { Depth, Reached }

/// One frame a level.
function down(depth: Depth) -> Depth {
    if (depth == 0) {
        return 0
    }
    return Depth(down(Depth(depth - 1)) + 1)
}

/// Walks `depth` frames down and back.
action reach(depth: Depth) -> Reached {
    return Reached {
        depth: down(depth)
    }
}
"#,
    ),
    (
        "domains/probe/actions/reach_heavy.jadpo",
        r#"use probe/types { Depth, Label, Reached }

/// One frame a level, each keeping a good deal on the way.
function down_heavy(depth: Depth, label: Label) -> Depth {
    if (depth == 0) {
        return 0
    }
    var a: Text = text.upper(label)
    var b: Text = text.lower(a)
    var c: List<Text> = [a, b, text.trim(label)]
    var d: Int = list.count(c) + text.length(a) * 3 + text.length(b) % 7
    var f: Int = match list.first(c) {
        some(found) -> text.length(found)
        none -> 0
    }
    var g: Text = text.join(list.reverse(c), ",")
    var h: Int = f + d + text.length(g)
    var next: Depth = Depth(depth - 1)
    var below: Depth = down_heavy(next, Label(text.slice(g, 0, 5)))
    return Depth(below + 1 + (h - h))
}

/// Walks `depth` heavy frames down and back.
action reach_heavy(depth: Depth) -> Reached {
    return Reached {
        depth: down_heavy(depth, "Probe")
    }
}
"#,
    ),
    (
        "domains/probe/actions/reach_mapped.jadpo",
        r#"use probe/types { Depth, Reached }

/// One frame a level, each through an inline function.
function down_mapped(depth: Depth) -> Depth {
    if (depth == 0) {
        return 0
    }
    return Depth(list.sum(list.map([depth], level -> down_mapped(Depth(level - 1)))) + 1)
}

/// Walks `depth` levels down through inline functions and back.
action reach_mapped(depth: Depth) -> Reached {
    return Reached {
        depth: down_mapped(depth)
    }
}
"#,
    ),
    (
        "domains/probe/actions/spin.jadpo",
        r#"use probe/types { Spun, Total }

/// Counts every triple of rounds, in an action's loops.
action spin(rounds: List<Total>) -> Spun {
    var mut total: Total = 0
    for a in rounds {
        for b in rounds {
            for c in rounds {
                total = Total(total + a + b + c)
            }
        }
    }
    return Spun {
        total: total
    }
}
"#,
    ),
    (
        "domains/probe/actions/spin_function.jadpo",
        r#"use probe/types { Spun, Total }

/// Counts every triple of rounds, in a function's loops.
function looped(rounds: List<Total>) -> Total {
    var mut total: Total = 0
    for a in rounds {
        for b in rounds {
            for c in rounds {
                total = Total(total + a + b + c)
            }
        }
    }
    return total
}

/// Loops in a function.
action spin_function(rounds: List<Total>) -> Spun {
    return Spun {
        total: looped(rounds)
    }
}
"#,
    ),
    (
        "domains/probe/actions/spin_inline.jadpo",
        r#"use probe/types { Spun, Total }

/// Counts every triple of rounds, in inline functions.
function mapped(rounds: List<Total>) -> Total {
    return Total(list.sum(list.map(rounds, a -> list.sum(list.map(rounds, b -> list.count(list.filter(rounds, c -> c >= a + b)))))))
}

/// Inline functions.
action spin_inline(rounds: List<Total>) -> Spun {
    return Spun {
        total: mapped(rounds)
    }
}
"#,
    ),
    (
        "domains/probe/routes/probe.jadpo",
        r#"use probe/types { ReachRequest, Reached, SpinRequest, Spun }
use probe/actions { reach, reach_heavy, reach_mapped, spin, spin_function, spin_inline }

/// A deployable's limits, probed.
routes Probe {
    prefix: /probe

    /// Walks down.
    POST /reach {
        authentication: RouteAuthentication.none
        input: ReachRequest
        output: Reached
        run: reach(input.depth)
    }

    /// Walks down heavily.
    POST /reach-heavy {
        authentication: RouteAuthentication.none
        input: ReachRequest
        output: Reached
        run: reach_heavy(input.depth)
    }

    /// Walks down through inline functions.
    POST /reach-mapped {
        authentication: RouteAuthentication.none
        input: ReachRequest
        output: Reached
        run: reach_mapped(input.depth)
    }

    /// Loops in an action.
    POST /spin {
        authentication: RouteAuthentication.none
        input: SpinRequest
        output: Spun
        deadline: 1s
        run: spin(input.rounds)
    }

    /// Loops in a function.
    POST /spin-function {
        authentication: RouteAuthentication.none
        input: SpinRequest
        output: Spun
        deadline: 1s
        run: spin_function(input.rounds)
    }

    /// Inline functions.
    POST /spin-inline {
        authentication: RouteAuthentication.none
        input: SpinRequest
        output: Spun
        deadline: 1s
        run: spin_inline(input.rounds)
    }
}
"#,
    ),
];

/// The lines a project's root policy lists under `actions:`, each probe
/// open to anyone.
pub const POLICY: &str = "        probe.reach: [Access.public]\n        probe.reach_heavy: [Access.public]\n        probe.reach_mapped: [Access.public]\n        probe.spin: [Access.public]\n        probe.spin_function: [Access.public]\n        probe.spin_inline: [Access.public]\n";

/// Each walk asked for and what it answers: a thousand frames, the
/// action's and 999 below it, plain or heavy; four hundred inline
/// functions running at once; and one level more of each, the
/// `recursion_limit` fault, which answers `internal_fault`.
pub const WALKS: &[(&str, u32, Result<(), &str>)] = &[
    ("/probe/reach", 998, Ok(())),
    ("/probe/reach", 999, Err("internal_fault")),
    ("/probe/reach-heavy", 998, Ok(())),
    ("/probe/reach-heavy", 999, Err("internal_fault")),
    ("/probe/reach-mapped", 400, Ok(())),
    ("/probe/reach-mapped", 401, Err("internal_fault")),
];

/// A walk's request body.
pub fn walk(depth: u32) -> String {
    format!(r#"{{"depth":{depth}}}"#)
}

/// The routes whose work never waits, each against a one-second deadline.
pub const SPINS: &[&str] = &["/probe/spin", "/probe/spin-function", "/probe/spin-inline"];

/// A spin's request body: a thousand rounds, a thousand million turns of
/// its innermost loop or calls of its innermost inline function, which
/// take far longer than the deadline however fast the machine.
pub fn spin() -> String {
    let rounds = vec!["1"; 1000].join(",");
    format!(r#"{{"rounds":[{rounds}]}}"#)
}
