//! Where `use` lines go (area 1, Imports): at the top of the file, one line
//! per folder, sorted. Names come first, from the most general place to the
//! most specific: the root files, then `shared/`, then each domain's folders
//! by domain name; then the domains brought in for calls (`use menu`). The
//! folders of a domain or of `shared/` follow the order the layout lists
//! them in (`entities/`, `types/`, `failures/` ...), and the names inside
//! the braces are sorted. Reading the path alone, the formatter needs no
//! project to sort them.

use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::SyntaxNode;

/// The root files a `use` line can name, in the layout's order.
pub const ROOT_FILES: [&str; 5] = ["app", "config", "authentication", "policy", "flags"];

/// A domain's folders, in the layout's order (area 1, Domains).
pub const FOLDERS: [&str; 14] = [
    "entities",
    "types",
    "failures",
    "actions",
    "queries",
    "libraries",
    "routes",
    "schedules",
    "events",
    "subscribers",
    "emails",
    "translations",
    "prompts",
    "fixtures",
];

/// A `use` line's place among the others: lines are sorted by it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Rank {
    group: u8,
    /// The root file's position, or the domain's name.
    holder: (usize, String),
    /// The folder's position in the layout, then its name.
    folder: (usize, String),
    /// The line as written, so equal lines keep a stable order.
    written: String,
}

fn segments(node: &SyntaxNode) -> Vec<String> {
    node.children()
        .find(|child| child.kind() == USE_PATH)
        .into_iter()
        .flat_map(|path| path.children())
        .filter(|child| child.kind() == NAME_REF)
        .map(|name| text_of(&name))
        .collect()
}

fn text_of(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect()
}

fn folder_position(folder: &str) -> (usize, String) {
    let position = FOLDERS
        .iter()
        .position(|known| *known == folder)
        .unwrap_or(FOLDERS.len());
    (position, folder.to_string())
}

/// Where the `use` line `node` sorts.
pub(crate) fn rank(node: &SyntaxNode) -> Rank {
    let parts = segments(node);
    let names = node.children().any(|child| child.kind() == USE_NAMES);
    let written = text_of(node);
    let none = (0, String::new());
    let (group, holder, folder) = match (parts.as_slice(), names) {
        ([root], true) if ROOT_FILES.contains(&root.as_str()) => (
            0,
            (
                ROOT_FILES
                    .iter()
                    .position(|known| known == root)
                    .unwrap_or_default(),
                String::new(),
            ),
            none,
        ),
        ([shared, folder], true) if shared == "shared" => (1, none, folder_position(folder)),
        ([domain, folder], true) => (2, (0, domain.clone()), folder_position(folder)),
        ([domain], false) => (3, (0, domain.clone()), none),
        _ => (4, none.clone(), none),
    };
    Rank {
        group,
        holder,
        folder,
        written,
    }
}

/// The name a `use` line's braces hold, for sorting.
pub(crate) fn name_key(node: &SyntaxNode) -> String {
    text_of(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_syntax::parse_file;

    fn order(text: &str) -> Vec<String> {
        let tree = parse_file(text).syntax();
        let mut uses: Vec<SyntaxNode> = tree
            .children()
            .filter(|child| child.kind() == USE_DECL)
            .collect();
        uses.sort_by_key(rank);
        uses.iter().map(text_of).collect()
    }

    #[test]
    fn names_go_from_general_to_specific_then_domains() {
        let text = "use menu\nuse sales/actions { price_lines }\nuse shared/libraries { money }\nuse sales/types { Basket }\nuse app { stripe }\nuse shared/types { Pence }\nuse sales/failures { BasketEmpty }\nuse menu/types { MenuItem }\n";
        assert_eq!(
            order(text),
            [
                "useapp{stripe}",
                "useshared/types{Pence}",
                "useshared/libraries{money}",
                "usemenu/types{MenuItem}",
                "usesales/types{Basket}",
                "usesales/failures{BasketEmpty}",
                "usesales/actions{price_lines}",
                "usemenu",
            ]
        );
    }
}
