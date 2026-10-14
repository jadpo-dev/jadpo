//! Places: which `.jadpo` file and line each frame names (area 17, "Errors
//! point at the `.jadpo` line"), and the bytes of every site, so a report
//! can show a site's code frame and hang a value under it. The table comes
//! from the plan, once; compiled code holds it as a static array.

use std::fmt;

/// One site of a callable: its line, its bytes in the file, and where a
/// report hangs its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SitePlace {
    /// The 1-based line it starts on.
    pub line: u32,
    pub start: u32,
    pub end: u32,
    /// An operator's first byte for arithmetic, comparisons and
    /// `and`/`or`, a field's name for a field read, otherwise `start`.
    pub anchor: u32,
}

impl SitePlace {
    /// A site that starts and ends at the start of `line`.
    pub fn line(line: u32) -> SitePlace {
        SitePlace {
            line,
            start: 0,
            end: 0,
            anchor: 0,
        }
    }
}

/// One callable's place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallablePlace {
    /// How a frame names it: `action place_order`, `function
    /// rounding.half_even`, `test "a large size is rejected"`.
    pub label: String,
    /// Its project-relative `.jadpo` file.
    pub file: String,
    /// The line its declaration starts on, for a frame at no site yet.
    pub line: u32,
    /// Each of its sites, by site number.
    pub sites: Vec<SitePlace>,
}

/// Where each callable's sites are, by callable number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places {
    callables: Vec<CallablePlace>,
}

/// One frame of the shadow stack: a callable and the site it is at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Frame {
    pub callable: u32,
    pub site: u32,
}

impl Frame {
    /// The site of a frame that has reached none yet: its declaration.
    pub const ENTRY: u32 = u32::MAX;
}

/// A frame as a reader sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place<'p> {
    pub file: &'p str,
    pub line: u32,
    pub label: &'p str,
}

impl fmt::Display for Place<'_> {
    /// `domains/ordering/actions/price_line.jadpo:4  action price_line`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}  {}", self.file, self.line, self.label)
    }
}

/// A run of frames that name the same place, as area 17 prints a
/// recursion: once, with how many frames it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repeated<'p> {
    /// The run's first frame, innermost.
    pub frame: Frame,
    pub place: Place<'p>,
    pub repeats: usize,
}

impl Places {
    pub fn new() -> Places {
        Places::default()
    }

    /// Adds a callable; its number.
    pub fn push(&mut self, callable: CallablePlace) -> u32 {
        self.callables.push(callable);
        u32::try_from(self.callables.len() - 1).unwrap_or(u32::MAX)
    }

    pub fn callable(&self, callable: u32) -> Option<&CallablePlace> {
        self.callables.get(callable as usize)
    }

    pub fn len(&self) -> usize {
        self.callables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.callables.is_empty()
    }

    /// The file, line and label a frame names.
    pub fn place(&self, frame: Frame) -> Option<Place<'_>> {
        let callable = self.callable(frame.callable)?;
        let line = callable
            .sites
            .get(frame.site as usize)
            .map_or(callable.line, |site| site.line);
        Some(Place {
            file: &callable.file,
            line,
            label: &callable.label,
        })
    }

    /// The site a frame stands at, with its callable's file; `None` for a
    /// callable's entry, which stands at no site.
    pub fn site(&self, frame: Frame) -> Option<(&str, SitePlace)> {
        let callable = self.callable(frame.callable)?;
        let site = callable.sites.get(frame.site as usize)?;
        Some((&callable.file, *site))
    }

    /// How a frame names its callable; empty for an unknown one.
    pub fn label(&self, callable: u32) -> &str {
        self.callable(callable)
            .map_or("", |callable| callable.label.as_str())
    }

    /// `frames`, innermost first, with each run of frames naming the same
    /// place counted once: what every stack a person reads shows.
    pub fn collapse(&self, frames: &[Frame]) -> Vec<Repeated<'_>> {
        let mut out: Vec<Repeated<'_>> = Vec::new();
        for frame in frames {
            let Some(place) = self.place(*frame) else {
                continue;
            };
            match out.last_mut() {
                Some(last) if last.place == place => last.repeats += 1,
                _ => out.push(Repeated {
                    frame: *frame,
                    place,
                    repeats: 1,
                }),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> Places {
        let mut places = Places::new();
        places.push(CallablePlace {
            label: "action price_line".into(),
            file: "domains/ordering/actions/price_line.jadpo".into(),
            line: 1,
            sites: vec![SitePlace::line(2), SitePlace::line(4)],
        });
        places
    }

    #[test]
    fn a_frame_names_its_file_and_line() {
        let places = places();
        let at = places
            .place(Frame {
                callable: 0,
                site: 1,
            })
            .map(|p| p.to_string());
        assert_eq!(
            at.as_deref(),
            Some("domains/ordering/actions/price_line.jadpo:4  action price_line")
        );
        let entry = places.place(Frame {
            callable: 0,
            site: Frame::ENTRY,
        });
        assert_eq!(entry.map(|p| p.line), Some(1));
        assert_eq!(
            places.place(Frame {
                callable: 7,
                site: 0
            }),
            None
        );
    }

    #[test]
    fn a_recursion_is_one_frame_with_its_count() {
        let places = places();
        let at = |site| Frame { callable: 0, site };
        let collapsed = places.collapse(&[at(1), at(1), at(1), at(0), at(1)]);
        let shown: Vec<(u32, usize)> = collapsed
            .iter()
            .map(|run| (run.place.line, run.repeats))
            .collect();
        assert_eq!(shown, [(4, 3), (2, 1), (4, 1)]);
    }
}
