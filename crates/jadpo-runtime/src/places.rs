//! Places: which `.jadpo` file and line each frame names (area 17, "Errors
//! point at the `.jadpo` line"). The table is built from the graph, once;
//! compiled code holds it as a static array.

use std::fmt;

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
    /// The line of each of its sites, by site number.
    pub sites: Vec<u32>,
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
            .copied()
            .unwrap_or(callable.line);
        Some(Place {
            file: &callable.file,
            line,
            label: &callable.label,
        })
    }

    /// How a frame names its callable; empty for an unknown one.
    pub fn label(&self, callable: u32) -> &str {
        self.callable(callable)
            .map_or("", |callable| callable.label.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_names_its_file_and_line() {
        let mut places = Places::new();
        let id = places.push(CallablePlace {
            label: "action price_line".into(),
            file: "domains/ordering/actions/price_line.jadpo".into(),
            line: 1,
            sites: vec![2, 4],
        });
        let at = places
            .place(Frame {
                callable: id,
                site: 1,
            })
            .map(|p| p.to_string());
        assert_eq!(
            at.as_deref(),
            Some("domains/ordering/actions/price_line.jadpo:4  action price_line")
        );
        let entry = places.place(Frame {
            callable: id,
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
}
