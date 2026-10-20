//! A Jadpo application's `x-jadpo-constraints` (area 7, chunk 7.2): a
//! field's type and rules as the API record writes them, `List<Text {
//! max_length: 20 }> { max_length: 5 }?`, read back so the importer writes
//! exactly the type the other application declared.

/// One type as the record writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// `List`, `Text`, `TodoView`.
    pub name: String,
    pub args: Vec<Written>,
    /// Each rule's name and its value as the record writes it.
    pub rules: Vec<(String, String)>,
    pub optional: bool,
}

/// `text` read, or `None` when it isn't the record's notation.
pub fn parse(text: &str) -> Option<Written> {
    let mut reader = Reader {
        chars: text.chars().collect(),
        at: 0,
    };
    let written = reader.ty()?;
    (reader.at == reader.chars.len()).then_some(written)
}

struct Reader {
    chars: Vec<char>,
    at: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn eat(&mut self, text: &str) -> bool {
        let wanted: Vec<char> = text.chars().collect();
        if self.chars[self.at..].starts_with(&wanted) {
            self.at += wanted.len();
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Option<String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            self.at += 1;
        }
        let name: String = self.chars[start..self.at].iter().collect();
        (!name.is_empty() && name.starts_with(|c: char| c.is_ascii_alphabetic())).then_some(name)
    }

    fn ty(&mut self) -> Option<Written> {
        let name = self.name()?;
        let mut args = Vec::new();
        if self.eat("<") {
            loop {
                args.push(self.ty()?);
                if self.eat(", ") {
                    continue;
                }
                if self.eat(">") {
                    break;
                }
                return None;
            }
        }
        let mut rules = Vec::new();
        if self.eat(" { ") {
            loop {
                let rule = self.name()?;
                if !self.eat(": ") {
                    return None;
                }
                rules.push((rule, self.value()?));
                if self.eat(", ") {
                    continue;
                }
                if self.eat(" }") {
                    break;
                }
                return None;
            }
        }
        let optional = self.eat("?");
        Some(Written {
            name,
            args,
            rules,
            optional,
        })
    }

    /// A rule's value: up to the next `, ` or ` }`, or a quoted pattern,
    /// which the record writes unescaped, up to the quote that ends it.
    fn value(&mut self) -> Option<String> {
        let start = self.at;
        if self.peek() == Some('"') {
            self.at += 1;
            while self.at < self.chars.len() {
                if self.chars[self.at] == '"' {
                    let rest: String = self.chars[self.at + 1..].iter().collect();
                    if rest.starts_with(" }") || ends_a_rule(&rest) {
                        self.at += 1;
                        let quoted: String = self.chars[start + 1..self.at - 1].iter().collect();
                        return Some(format!("\"{quoted}\""));
                    }
                }
                self.at += 1;
            }
            return None;
        }
        while self.at < self.chars.len() {
            let rest: String = self.chars[self.at..].iter().collect();
            if rest.starts_with(" }") || rest.starts_with(", ") {
                break;
            }
            self.at += 1;
        }
        let value: String = self.chars[start..self.at].iter().collect();
        (!value.is_empty()).then_some(value)
    }
}

/// Whether `rest` starts `, <rule>: `.
fn ends_a_rule(rest: &str) -> bool {
    let Some(after) = rest.strip_prefix(", ") else {
        return false;
    };
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    !name.is_empty() && after[name.len()..].starts_with(": ")
}

/// A rule's value as Jadpo source writes it in a type: an option by its
/// enum (`format: TextFormat.email`, `currency: Currency.gbp`), a pattern
/// as a string with its backslashes kept, anything else as written.
pub fn source_value(rule: &str, value: &str) -> String {
    match rule {
        "format" => format!("TextFormat.{value}"),
        "version" => format!("UuidVersion.v{value}"),
        "currency" | "from" | "to" => format!("Currency.{}", value.to_ascii_lowercase()),
        "rounding" => format!("Rounding.{value}"),
        "distance" => format!("VectorDistance.{value}"),
        "pattern" => {
            let inner = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(value);
            super::model::quote(inner)
        }
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(name: &str) -> Written {
        Written {
            name: name.to_string(),
            args: Vec::new(),
            rules: Vec::new(),
            optional: false,
        }
    }

    #[test]
    fn reads_the_records_notation() {
        assert_eq!(parse("Uuid"), Some(plain("Uuid")));
        assert_eq!(
            parse("Text { min_length: 1, max_length: 200 }"),
            Some(Written {
                rules: vec![
                    ("min_length".to_string(), "1".to_string()),
                    ("max_length".to_string(), "200".to_string()),
                ],
                ..plain("Text")
            })
        );
        let list = parse("List<Text { max_length: 20 }> { max_length: 5 }?").expect("a list");
        assert_eq!(list.name, "List");
        assert!(list.optional);
        assert_eq!(list.args[0].rules[0].1, "20");
        assert_eq!(
            parse("Map<Text, TodoView>").map(|map| map.args.len()),
            Some(2)
        );
        assert_eq!(
            parse("Text { pattern: \"a, b: \"c\" }").map(|text| text.rules[0].1.clone()),
            Some("\"a, b: \"c\"".to_string())
        );
        assert_eq!(parse("Text {"), None);
        assert_eq!(parse("Text\n}"), None);
    }

    #[test]
    fn writes_rules_as_source() {
        assert_eq!(source_value("currency", "GBP"), "Currency.gbp");
        assert_eq!(source_value("format", "email"), "TextFormat.email");
        assert_eq!(source_value("pattern", "\"[a-z]\\d\""), "\"[a-z]\\\\d\"");
        assert_eq!(source_value("max", "50"), "50");
    }
}
