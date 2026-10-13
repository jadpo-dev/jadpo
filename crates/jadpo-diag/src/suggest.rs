//! Nearest-name suggestions for typos ("did you mean `help`?"), shared by
//! command-line errors and compiler errors.

/// The candidate nearest to `word`, if one is close enough to be what was
/// meant: within one edit for short words and two for longer ones (a swap
/// of neighbouring letters counts as one), or else the candidate it is the
/// start of, when only one fits (a name and its plural count as one: the
/// shorter wins). Ties go to the earlier candidate.
pub fn nearest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let word = word.to_lowercase();
    let length = word.chars().count();
    let limit = match length {
        0..=2 => 0,
        3..=4 => 1,
        _ => 2,
    };
    let candidates: Vec<&str> = candidates.into_iter().collect();
    let closest = candidates
        .iter()
        .map(|candidate| (distance(&word, &candidate.to_lowercase()), *candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate);
    closest.or_else(|| {
        if length < 3 {
            return None;
        }
        let starting: Vec<&str> = candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.to_lowercase().starts_with(&word))
            .collect();
        let shortest = starting
            .iter()
            .copied()
            .min_by_key(|candidate| candidate.len())?;
        starting
            .iter()
            .all(|candidate| candidate.starts_with(shortest))
            .then_some(shortest)
    })
}

/// Optimal string alignment distance: insertions, deletions, substitutions
/// and swaps of adjacent characters each cost one.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut rows = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMANDS: [&str; 4] = ["completions", "completion", "help", "version"];

    #[test]
    fn swaps_and_slips_find_the_command() {
        assert_eq!(nearest("hlep", COMMANDS), Some("help"));
        assert_eq!(nearest("verison", COMMANDS), Some("version"));
        assert_eq!(nearest("HELP", COMMANDS), Some("help"));
        assert_eq!(nearest("completoin", COMMANDS), Some("completion"));
        assert_eq!(nearest("helpp", COMMANDS), Some("help"));
    }

    #[test]
    fn a_unique_prefix_finds_the_command() {
        assert_eq!(nearest("vers", COMMANDS), Some("version"));
        assert_eq!(
            nearest("comp", COMMANDS),
            Some("completion"),
            "a name and its plural are one"
        );
        assert_eq!(
            nearest("complet", ["completions", "help"]),
            Some("completions")
        );
        assert_eq!(nearest("hel", ["help", "hello"]), Some("help"));
        assert_eq!(
            nearest("ver", ["version", "verify"]),
            None,
            "two different names start with it"
        );
    }

    #[test]
    fn distant_or_tiny_words_find_nothing() {
        assert_eq!(nearest("frobnicate", COMMANDS), None);
        assert_eq!(nearest("he", COMMANDS), None);
        assert_eq!(nearest("", COMMANDS), None);
    }

    #[test]
    fn distance_counts_a_swap_once() {
        assert_eq!(distance("ab", "ba"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
    }
}
