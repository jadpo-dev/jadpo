//! The paths a run took (docs/compiler-runtime.md §2.5, "Differential
//! test"; roadmap chunk 9.3): what each executor tells its [`crate::Ctx`]
//! as it runs a body numbered by `jadpo_graph::paths`, counted by body,
//! region, segment and path number, for a test's record.
//!
//! Each frame's body opens region 0 as it is entered and closes it as it
//! is left, however it is left; a `for`'s turn or an inline function's run
//! opens its region and closes it at its end. A turn a stop cuts short is
//! closed when its frame next adds to a region around it, or as the frame
//! is left, with the number it had reached: when it is closed doesn't
//! change what is counted, so both executors count alike whichever stops
//! catch where.

use std::collections::BTreeMap;

/// One path a run took, and how many times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Taken {
    /// The callable whose body it went through.
    pub callable: u32,
    /// The region: 0 the body, otherwise a `for`'s or an inline function
    /// body's site plus one.
    pub region: u32,
    /// The segment: 0 from the region's start, otherwise the site plus one
    /// of the statement after which it began.
    pub segment: u32,
    pub path: u64,
    pub times: u64,
}

/// A region being run.
#[derive(Clone, Copy, Debug)]
struct Open {
    region: u32,
    segment: u32,
    path: u64,
}

/// The paths of one run, while it is kept.
#[derive(Debug, Default)]
pub(crate) struct PathLog {
    /// Every region being run, innermost last.
    open: Vec<Open>,
    /// Each frame's callable and where its body's region is in `open`.
    frames: Vec<(u32, usize)>,
    /// Each path closed, by callable, region, segment and number.
    taken: BTreeMap<(u32, u32, u32, u64), u64>,
}

impl PathLog {
    /// Callable `callable`'s body is entered.
    pub fn enter(&mut self, callable: u32) {
        self.frames.push((callable, self.open.len()));
        self.open.push(Open {
            region: 0,
            segment: 0,
            path: 0,
        });
    }

    /// The current frame is left: every region it has open is closed.
    pub fn leave(&mut self) {
        if let Some((callable, at)) = self.frames.pop() {
            self.close_from(callable, at);
        }
    }

    /// Where `region` is open in the current frame, the regions opened
    /// after it closed.
    fn find(&mut self, region: u32) -> Option<usize> {
        let (callable, base) = *self.frames.last()?;
        let at = (base..self.open.len())
            .rev()
            .find(|&at| self.open.get(at).is_some_and(|open| open.region == region))?;
        self.close_from(callable, at + 1);
        Some(at)
    }

    /// Closes every region open from `at` on, as `callable`'s.
    fn close_from(&mut self, callable: u32, at: usize) {
        for open in self.open.drain(at.min(self.open.len())..) {
            *self
                .taken
                .entry((callable, open.region, open.segment, open.path))
                .or_default() += 1;
        }
    }

    /// A branch, a `return` or an `and`'s right side adds `increment` in
    /// `region`.
    pub fn branch(&mut self, region: u32, increment: u64) {
        if let Some(at) = self.find(region)
            && let Some(open) = self.open.get_mut(at)
        {
            open.path = open.path.wrapping_add(increment);
        }
    }

    /// A turn of `region` starts in the current frame.
    pub fn turn(&mut self, region: u32) {
        if !self.frames.is_empty() {
            self.open.push(Open {
                region,
                segment: 0,
                path: 0,
            });
        }
    }

    /// A turn of `region` ran to its end.
    pub fn turned(&mut self, region: u32) {
        if let (Some(at), Some(&(callable, _))) = (self.find(region), self.frames.last()) {
            self.close_from(callable, at);
        }
    }

    /// `region`'s way so far ends as a segment after statement `site`; the
    /// next starts from 0.
    pub fn cut(&mut self, region: u32, site: u32) {
        let Some(at) = self.find(region) else {
            return;
        };
        let Some(&(callable, _)) = self.frames.last() else {
            return;
        };
        if let Some(open) = self.open.get_mut(at) {
            let closed = *open;
            open.segment = site.saturating_add(1);
            open.path = 0;
            *self
                .taken
                .entry((callable, closed.region, closed.segment, closed.path))
                .or_default() += 1;
        }
    }

    /// Every path taken, in callable, region, segment and number order,
    /// every frame still open closed first.
    pub fn taken(&mut self) -> Vec<Taken> {
        while !self.frames.is_empty() {
            self.leave();
        }
        self.taken
            .iter()
            .map(|(&(callable, region, segment, path), &times)| Taken {
                callable,
                region,
                segment,
                path,
                times,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(log: &mut PathLog) -> Vec<(u32, u32, u32, u64, u64)> {
        log.taken()
            .into_iter()
            .map(|taken| {
                (
                    taken.callable,
                    taken.region,
                    taken.segment,
                    taken.path,
                    taken.times,
                )
            })
            .collect()
    }

    /// A body's region is counted as it is left, with what its branches
    /// added; a call's own body is its own.
    #[test]
    fn each_frame_counts_its_own_path() {
        let mut log = PathLog::default();
        log.enter(1);
        log.branch(0, 2);
        log.enter(2);
        log.branch(0, 1);
        log.leave();
        log.enter(2);
        log.leave();
        log.branch(0, 1);
        log.leave();
        assert_eq!(
            taken(&mut log),
            [(1, 0, 0, 3, 1), (2, 0, 0, 0, 1), (2, 0, 0, 1, 1)]
        );
    }

    /// Each turn is counted by the path it took; a turn a stop cut short
    /// is counted when its frame next adds to a region around it, with
    /// what it had reached, and a branch of a region not open adds
    /// nothing.
    #[test]
    fn turns_are_counted_however_they_end() {
        let mut log = PathLog::default();
        log.enter(1);
        for path in [1, 0, 1] {
            log.turn(5);
            log.branch(5, path);
            log.turned(5);
        }
        log.turn(5);
        log.branch(5, 1);
        log.branch(9, 4);
        log.branch(0, 2);
        log.leave();
        assert_eq!(
            taken(&mut log),
            [(1, 0, 0, 2, 1), (1, 5, 0, 0, 1), (1, 5, 0, 1, 3)]
        );
    }

    /// A cut counts the way so far as a segment and starts the next from
    /// 0, named by the statement it follows.
    #[test]
    fn a_cut_starts_a_segment() {
        let mut log = PathLog::default();
        log.enter(1);
        log.branch(0, 3);
        log.cut(0, 7);
        log.branch(0, 1);
        log.leave();
        assert_eq!(taken(&mut log), [(1, 0, 0, 3, 1), (1, 0, 8, 1, 1)]);
    }
}
