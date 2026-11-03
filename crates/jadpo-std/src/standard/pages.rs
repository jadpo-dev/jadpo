//! What each standard type's page says that the compiler's data can't: what
//! it is, when to use it and what to use instead, a working example, and
//! what is good to know (docs/writing.md, "How a standard type's page
//! reads"). It is written once, beside the types, one Markdown file per
//! group in `pages/`:
//!
//! ~~~text
//! # PersonName
//!
//! One line saying what it is.
//!
//! When to use it and what to use instead, in a paragraph.
//!
//! ```jadpo
//! PersonName { given: "Mary" family: "Somerville" }
//! ```
//!
//! - Something a reader wouldn't guess.
//! ~~~
//!
//! `jadpo-cli` puts these together with the compiler's data to make the
//! page. A test here holds every standard type to exactly one entry, and
//! the docs check compiles and runs every example.

use super::groups::GROUPS;

/// The hand-written part of one type's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypePage {
    pub name: &'static str,
    /// What it is, in one line.
    pub meaning: String,
    /// When to use it and what to use instead.
    pub when: String,
    /// Code that makes one, as the page shows it.
    pub example: String,
    /// What a reader wouldn't guess, each a sentence or two.
    pub notes: Vec<String>,
}

/// Each group's file, in the groups' order.
const FILES: &[(&str, &str)] = &[
    ("People and contact", include_str!("pages/people.md")),
    ("Places", include_str!("pages/places.md")),
    ("Geospatial", include_str!("pages/geospatial.md")),
    ("Money and banking", include_str!("pages/money.md")),
    ("Tax and identity numbers", include_str!("pages/identity.md")),
    ("Commerce and logistics", include_str!("pages/commerce.md")),
    ("Time and calendar", include_str!("pages/time.md")),
    ("Physical quantities", include_str!("pages/quantities.md")),
    ("Maths and data", include_str!("pages/maths.md")),
    ("Health", include_str!("pages/health.md")),
    ("Media and visual", include_str!("pages/media.md")),
    ("Web and network", include_str!("pages/web.md")),
    ("Software", include_str!("pages/software.md")),
    ("Text formats", include_str!("pages/text_formats.md")),
    ("Security", include_str!("pages/security.md")),
    ("Travel", include_str!("pages/travel.md")),
    ("Other with a published standard", include_str!("pages/other.md")),
    ("AI", include_str!("pages/ai.md")),
];

/// Every type's entry, each file's in the order it writes them.
pub fn all() -> Vec<TypePage> {
    FILES
        .iter()
        .flat_map(|(_, file)| entries(file))
        .collect()
}

/// `name`'s entry.
pub fn of(name: &str) -> Option<TypePage> {
    let (group, _) = GROUPS
        .iter()
        .find(|(_, names)| names.contains(&name))?;
    let (_, file) = FILES.iter().find(|(written, _)| written == group)?;
    entries(file).into_iter().find(|page| page.name == name)
}

/// A file's entries: each `# Name`, and what is under it.
fn entries(file: &'static str) -> Vec<TypePage> {
    let mut pages = Vec::new();
    let mut lines = file.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(name) = line.strip_prefix("# ") else {
            continue;
        };
        let mut paragraphs: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut example = String::new();
        let mut notes: Vec<String> = Vec::new();
        let mut in_notes = false;
        while let Some(line) = lines.peek().copied() {
            if line.starts_with("# ") {
                break;
            }
            lines.next();
            if line.starts_with("```") {
                for code in lines.by_ref() {
                    if code.starts_with("```") {
                        break;
                    }
                    example.push_str(code);
                    example.push('\n');
                }
            } else if let Some(note) = line.strip_prefix("- ") {
                in_notes = true;
                notes.push(note.to_string());
            } else if in_notes && line.starts_with("  ") {
                if let Some(note) = notes.last_mut() {
                    note.push(' ');
                    note.push_str(line.trim());
                }
            } else if line.is_empty() {
                if !current.is_empty() {
                    paragraphs.push(std::mem::take(&mut current));
                }
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(line.trim());
            }
        }
        if !current.is_empty() {
            paragraphs.push(current);
        }
        let mut paragraphs = paragraphs.into_iter();
        pages.push(TypePage {
            name,
            meaning: paragraphs.next().unwrap_or_default(),
            when: paragraphs.collect::<Vec<_>>().join("\n\n"),
            example: example.trim_end().to_string(),
            notes,
        });
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::super::STANDARD_TYPES;
    use super::super::groups::group_of;
    use super::*;

    #[test]
    fn every_standard_type_has_exactly_one_entry_in_its_groups_file() {
        let mut written: Vec<&str> = Vec::new();
        for (group, file) in FILES {
            for page in entries(file) {
                assert_eq!(group_of(page.name), Some(*group), "{}", page.name);
                written.push(page.name);
            }
        }
        for entry in STANDARD_TYPES {
            let times = written.iter().filter(|name| **name == entry.name).count();
            assert_eq!(times, 1, "{} has {times} entries in pages/", entry.name);
        }
        for name in written {
            assert!(
                STANDARD_TYPES.iter().any(|entry| entry.name == name),
                "{name} has an entry in pages/ but isn't a standard type"
            );
        }
    }

    #[test]
    fn the_files_are_in_the_groups_order() {
        let files: Vec<&str> = FILES.iter().map(|(group, _)| *group).collect();
        let groups: Vec<&str> = GROUPS.iter().map(|(group, _)| *group).collect();
        assert_eq!(files, groups);
    }

    #[test]
    fn every_entry_says_what_it_is_when_to_use_it_and_shows_one() {
        for page in all() {
            let name = page.name;
            assert!(
                page.meaning.ends_with('.') && page.meaning.len() <= 110,
                "{name}: what it is must be a sentence of at most 110 characters, for the index: {}",
                page.meaning
            );
            assert!(page.when.ends_with('.'), "{name}: when to use it");
            assert!(!page.example.is_empty(), "{name}: an example");
        }
    }

    #[test]
    fn a_page_keeps_to_plain_words() {
        for page in all() {
            let mut prose = vec![page.meaning.as_str(), page.when.as_str()];
            prose.extend(page.notes.iter().map(String::as_str));
            for text in prose {
                for word in ["contract", "baseline", "not ordered", "canonical"] {
                    assert!(
                        !text.to_lowercase().contains(word),
                        "{}: \"{word}\" in: {text}",
                        page.name
                    );
                }
            }
        }
    }
}
