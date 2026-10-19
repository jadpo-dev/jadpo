//! Tab completion (area 14, question 17). Each shell's script is a thin
//! wrapper that asks `jadpo __complete -- <words>` for candidates, one per
//! line as `value<TAB>description`, so commands, flags and values come from
//! the command table, and the project's own names can join them later
//! without changing the scripts.

use std::path::Path;

use crate::table::{self, Flag, GLOBAL_FLAGS, Values};

/// The hidden command the scripts call. Never listed in help.
pub const CALLBACK: &str = "__complete";

const ZSH: &str = r#"#compdef jadpo
# Tab completion for jadpo. Load it with: source <(jadpo completions zsh)
_jadpo() {
  local -a candidates
  local line
  for line in "${(@f)$(jadpo __complete -- "${(@)words[2,CURRENT]}" 2>/dev/null)}"; do
    [[ -n $line ]] || continue
    candidates+=("${${line%%$'\t'*}//:/\\:}:${line#*$'\t'}")
  done
  _describe -t jadpo jadpo candidates
}
if [[ $funcstack[1] == _jadpo ]]; then
  _jadpo "$@"
else
  compdef _jadpo jadpo
fi
"#;

const BASH: &str = r#"# Tab completion for jadpo. Load it with: eval "$(jadpo completions bash)"
_jadpo() {
  local line tab=$'\t'
  COMPREPLY=()
  while IFS= read -r line; do
    [[ -n $line ]] && COMPREPLY+=("${line%%$tab*}")
  done < <(jadpo __complete -- "${COMP_WORDS[@]:1:COMP_CWORD}" 2>/dev/null)
}
complete -o default -F _jadpo jadpo
"#;

const FISH: &str = r#"# Tab completion for jadpo. Save it as ~/.config/fish/completions/jadpo.fish
function __jadpo_complete
    set -l words (commandline -opc)
    set -e words[1]
    jadpo __complete -- $words (commandline -ct) 2>/dev/null
end
complete -c jadpo -f -a '(__jadpo_complete)'
"#;

pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "zsh" => Some(ZSH),
        "bash" => Some(BASH),
        "fish" => Some(FISH),
        _ => None,
    }
}

/// Candidates for the last word, given every word after `jadpo`. Paths
/// are completed from `cwd`.
pub fn candidates(words: &[String], cwd: Option<&Path>) -> String {
    let (current, before) = match words.split_last() {
        Some((current, before)) => (current.as_str(), before),
        None => ("", &[][..]),
    };
    let mut command = None;
    let mut position = 0;
    let mut expecting: Option<&Flag> = None;
    for word in before {
        if expecting.take().is_some() {
            continue;
        }
        if word.starts_with('-') {
            expecting = value_flag(word, command);
        } else if command.is_none() {
            match table::find(word) {
                Some(found) => command = Some(found),
                None => return String::new(),
            }
        } else {
            position += 1;
        }
    }
    let found: Vec<(String, &str)> = if let Some(flag) = expecting {
        flag.values
            .iter()
            .map(|value| (value.name.to_string(), value.description))
            .collect()
    } else if let Some((name, _)) = current.split_once('=') {
        let flag = flags(command).find(|flag| flag.name == name);
        flag.map_or_else(Vec::new, |flag| {
            flag.values
                .iter()
                .map(|value| (format!("{name}={}", value.name), value.description))
                .collect()
        })
    } else if current.starts_with('-') {
        flags(command)
            .map(|flag| (flag.name.to_string(), flag.description))
            .collect()
    } else if let Some(command) = command {
        match command
            .arguments
            .get(position)
            .map(|argument| &argument.values)
        {
            Some(Values::Fixed(values)) => values
                .iter()
                .map(|value| (value.name.to_string(), value.description))
                .collect(),
            Some(Values::Commands) => commands(current),
            Some(Values::Path) => paths(current, cwd, ".jadpo"),
            Some(Values::TestPath) => paths(current, cwd, ".test.jadpo"),
            Some(Values::Query) => cwd
                .map(crate::sql::query_and_action_names)
                .unwrap_or_default(),
            Some(Values::Schedule) => cwd.map(crate::schedules::names).unwrap_or_default(),
            None => Vec::new(),
        }
    } else {
        commands(current)
    };
    found
        .into_iter()
        .filter(|(value, _)| value.starts_with(current))
        .map(|(value, description)| format!("{value}\t{description}\n"))
        .collect()
}

/// Folders and the files whose names end in `ending` beside the word
/// typed so far; hidden ones only when the word asks for them.
fn paths(current: &str, cwd: Option<&Path>, ending: &str) -> Vec<(String, &'static str)> {
    let Some(cwd) = cwd else {
        return Vec::new();
    };
    let (folder, prefix) = match current.rfind('/') {
        Some(i) => (&current[..=i], &current[i + 1..]),
        None => ("", current),
    };
    let Ok(entries) = std::fs::read_dir(cwd.join(folder)) else {
        return Vec::new();
    };
    let mut found: Vec<(String, &'static str)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            let kind = entry.file_type().ok()?;
            if kind.is_dir() {
                Some((format!("{folder}{name}/"), "Folder"))
            } else if name.ends_with(ending) {
                let what = if ending == ".test.jadpo" {
                    "Test file"
                } else {
                    "Jadpo file"
                };
                Some((format!("{folder}{name}"), what))
            } else {
                None
            }
        })
        .collect();
    found.sort();
    found
}

/// Canonical names; synonyms only when no canonical name fits the prefix.
fn commands(prefix: &str) -> Vec<(String, &'static str)> {
    let canonical: Vec<(String, &str)> = table::in_help_order()
        .into_iter()
        .filter(|command| command.name.starts_with(prefix))
        .map(|command| (command.name.to_string(), command.summary))
        .collect();
    if !canonical.is_empty() {
        return canonical;
    }
    table::COMMANDS
        .iter()
        .flat_map(|command| {
            command
                .synonyms
                .iter()
                .map(move |synonym| (synonym.to_string(), command.summary))
        })
        .collect()
}

fn flags(command: Option<&'static table::Command>) -> impl Iterator<Item = &'static Flag> {
    command
        .map_or(&[][..], |command| command.flags)
        .iter()
        .chain(GLOBAL_FLAGS)
}

/// The flag `word` names when it takes its value from the next word.
fn value_flag(word: &str, command: Option<&'static table::Command>) -> Option<&'static Flag> {
    if word.contains('=') {
        return None;
    }
    flags(command).find(|flag| flag.name == word && flag.value.is_some())
}
