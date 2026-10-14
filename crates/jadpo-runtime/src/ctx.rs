//! The per-invocation context (docs/compiler-runtime.md §2.5): the shadow
//! stack of frames both executors keep, the depth limit, and the faults
//! and failures that capture it. Effects (`read`, `write`, `emit`, `call`,
//! `now`) join it with their areas.

use crate::{Failure, Fault, FaultKind, Frame, MAX_DEPTH, Places};

/// One invocation's context.
#[derive(Debug)]
pub struct Ctx<'p> {
    places: &'p Places,
    /// Outermost first.
    frames: Vec<Frame>,
}

impl<'p> Ctx<'p> {
    /// A context running `entry`, the callable an invocation starts at.
    pub fn new(places: &'p Places, entry: u32) -> Ctx<'p> {
        Ctx {
            places,
            frames: vec![Frame {
                callable: entry,
                site: Frame::ENTRY,
            }],
        }
    }

    pub fn places(&self) -> &'p Places {
        self.places
    }

    /// How many frames the stack holds.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Enters `callable`, called from `site` of the current frame. Past
    /// [`MAX_DEPTH`] frames it is the `recursion_limit` fault, at that site.
    pub fn enter(&mut self, callable: u32, site: u32) -> Result<(), Box<Fault>> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(self.fault(FaultKind::RecursionLimit, site));
        }
        self.at(site);
        self.frames.push(Frame {
            callable,
            site: Frame::ENTRY,
        });
        Ok(())
    }

    /// Leaves the current callable.
    pub fn leave(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Moves the current frame to `site`.
    pub fn at(&mut self, site: u32) {
        if let Some(top) = self.frames.last_mut() {
            top.site = site;
        }
    }

    /// The frames, innermost first.
    pub fn frames(&self) -> Vec<Frame> {
        self.frames.iter().rev().copied().collect()
    }

    /// A fault at `site` of the current frame, with the stack.
    pub fn fault(&mut self, kind: FaultKind, site: u32) -> Box<Fault> {
        self.at(site);
        Box::new(Fault {
            kind,
            frames: self.frames(),
        })
    }

    /// `failure`, raised at `site` of the current frame.
    pub fn fail(&mut self, mut failure: Failure, site: u32) -> Box<Failure> {
        self.at(site);
        failure.frames = self.frames();
        Box::new(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CallablePlace;

    fn places() -> Places {
        let mut places = Places::new();
        for (label, line) in [("action place_order", 10), ("function countdown", 20)] {
            places.push(CallablePlace {
                label: label.into(),
                file: "main.jadpo".into(),
                line,
                sites: vec![line + 1, line + 2],
            });
        }
        places
    }

    #[test]
    fn faults_carry_the_frames_innermost_first() {
        let places = places();
        let mut ctx = Ctx::new(&places, 0);
        ctx.enter(1, 1).expect("room");
        let fault = ctx.fault(FaultKind::IntOverflow, 0);
        assert_eq!(
            fault.stack(&places),
            [
                "main.jadpo:21  function countdown",
                "main.jadpo:12  action place_order"
            ]
        );
        ctx.leave();
        let failure = ctx.fail(Failure::new("type Gone", "Gone", "gone"), 0);
        assert_eq!(failure.trace(&places), ["action place_order"]);
        assert_eq!(
            failure.site(),
            Some(Frame {
                callable: 0,
                site: 0
            })
        );
    }

    #[test]
    fn the_depth_limit_is_a_fault_at_the_call() {
        let places = places();
        let mut ctx = Ctx::new(&places, 0);
        for _ in 1..MAX_DEPTH {
            ctx.enter(1, 0).expect("room");
        }
        let fault = ctx.enter(1, 1).expect_err("too deep");
        assert_eq!(fault.kind, FaultKind::RecursionLimit);
        assert_eq!(fault.frames.len(), MAX_DEPTH);
        assert_eq!(
            fault.site(),
            Some(Frame {
                callable: 1,
                site: 1
            })
        );
        assert_eq!(fault.kind.classification(), "recursion_limit");
    }
}
