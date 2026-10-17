//! The small part of XML that XLIFF needs, read and written by hand: the
//! project takes no XML crate for one exchange format. It reads elements,
//! attributes, text, comments, processing instructions, CDATA and the five
//! named and the numeric character references. It refuses a document type
//! declaration (nothing here expands entities, so there is nothing for one
//! to define) and anything not well formed, with the byte where it went
//! wrong.

/// One element, with where it is in the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// The name without its prefix (`xliff`, not `x:xliff`).
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// The byte of its `<`.
    pub start: usize,
    /// The byte after its end tag's `>`.
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    /// Text with its references read, and where it started.
    Text(String, usize),
}

impl Element {
    /// An attribute by name, without its prefix (`id`, also `xml:space`'s
    /// `space`).
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The child elements with a name.
    pub fn elements<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter_map(move |node| match node {
            Node::Element(element) if element.name == name => Some(element),
            _ => None,
        })
    }

    pub fn first(&self, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|node| match node {
            Node::Element(element) if element.name == name => Some(element),
            _ => None,
        })
    }
}

/// A document: its root element.
pub fn parse(text: &str) -> Result<Element, (usize, String)> {
    let mut reader = Reader {
        text,
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.skip_misc()?;
    if reader.at >= reader.bytes.len() {
        return Err((0, "the document is empty".to_string()));
    }
    let root = reader.element()?;
    reader.skip_misc()?;
    if reader.at < reader.bytes.len() {
        return Err((
            reader.at,
            "there is more after the root element".to_string(),
        ));
    }
    Ok(root)
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

type Failure = (usize, String);

impl Reader<'_> {
    fn rest(&self) -> &str {
        &self.text[self.at..]
    }

    fn fail<T>(&self, message: &str) -> Result<T, Failure> {
        Err((self.at, message.to_string()))
    }

    fn skip_space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
    }

    /// The declaration, comments, processing instructions and space around
    /// the root.
    fn skip_misc(&mut self) -> Result<(), Failure> {
        if self.at == 0 && self.rest().starts_with('\u{feff}') {
            self.at += '\u{feff}'.len_utf8();
        }
        loop {
            self.skip_space();
            if self.rest().starts_with("<?") {
                self.processing_instruction()?;
            } else if self.rest().starts_with("<!--") {
                self.comment()?;
            } else if self.rest().starts_with("<!DOCTYPE") {
                return self
                    .fail("a document type declaration; XLIFF does not use one, and none is read");
            } else {
                return Ok(());
            }
        }
    }

    fn processing_instruction(&mut self) -> Result<(), Failure> {
        match self.rest().find("?>") {
            Some(end) => {
                self.at += end + 2;
                Ok(())
            }
            None => self.fail("a `<?` that is never closed"),
        }
    }

    fn comment(&mut self) -> Result<(), Failure> {
        match self.rest()[4..].find("-->") {
            Some(end) => {
                self.at += 4 + end + 3;
                Ok(())
            }
            None => self.fail("a comment that is never closed"),
        }
    }

    fn name(&mut self) -> Result<String, Failure> {
        let start = self.at;
        while self.bytes.get(self.at).is_some_and(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':') || *b >= 0x80
        }) {
            self.at += 1;
        }
        if start == self.at {
            return self.fail("a name was expected");
        }
        Ok(self.text[start..self.at].to_string())
    }

    fn element(&mut self) -> Result<Element, Failure> {
        let start = self.at;
        if self.bytes.get(self.at) != Some(&b'<') {
            return self.fail("an element was expected");
        }
        self.at += 1;
        let full = self.name()?;
        let mut attributes = Vec::new();
        loop {
            self.skip_space();
            match self.bytes.get(self.at) {
                Some(b'/') => {
                    self.at += 1;
                    if self.bytes.get(self.at) != Some(&b'>') {
                        return self.fail("`/` must be followed by `>`");
                    }
                    self.at += 1;
                    return Ok(Element {
                        name: local(&full),
                        attributes,
                        children: Vec::new(),
                        start,
                        end: self.at,
                    });
                }
                Some(b'>') => {
                    self.at += 1;
                    break;
                }
                Some(_) => {
                    let key = self.name()?;
                    self.skip_space();
                    if self.bytes.get(self.at) != Some(&b'=') {
                        return self.fail("an attribute needs `=` and a value");
                    }
                    self.at += 1;
                    self.skip_space();
                    let quote = match self.bytes.get(self.at) {
                        Some(&q @ (b'"' | b'\'')) => q,
                        _ => return self.fail("an attribute value needs quotes"),
                    };
                    self.at += 1;
                    let value_start = self.at;
                    while self.bytes.get(self.at).is_some_and(|b| *b != quote) {
                        self.at += 1;
                    }
                    if self.at >= self.bytes.len() {
                        return self.fail("an attribute value that is never closed");
                    }
                    let value = unescape(&self.text[value_start..self.at], value_start)?;
                    self.at += 1;
                    if attributes
                        .iter()
                        .any(|(seen, _): &(String, String)| *seen == key)
                    {
                        return Err((value_start, format!("`{key}` is written twice")));
                    }
                    attributes.push((attribute_key(&key), value));
                }
                None => return self.fail("a tag that is never closed"),
            }
        }
        let mut children = Vec::new();
        loop {
            if self.at >= self.bytes.len() {
                return Err((start, format!("`<{full}>` is never closed")));
            }
            if self.rest().starts_with("</") {
                self.at += 2;
                let close = self.name()?;
                self.skip_space();
                if self.bytes.get(self.at) != Some(&b'>') {
                    return self.fail("an end tag needs `>`");
                }
                if close != full {
                    return Err((self.at, format!("`</{close}>` closes `<{full}>`")));
                }
                self.at += 1;
                return Ok(Element {
                    name: local(&full),
                    attributes,
                    children,
                    start,
                    end: self.at,
                });
            } else if self.rest().starts_with("<!--") {
                self.comment()?;
            } else if self.rest().starts_with("<![CDATA[") {
                let from = self.at + 9;
                match self.text[from..].find("]]>") {
                    Some(end) => {
                        push_text(
                            &mut children,
                            self.text[from..from + end].to_string(),
                            self.at,
                        );
                        self.at = from + end + 3;
                    }
                    None => return self.fail("a CDATA section that is never closed"),
                }
            } else if self.rest().starts_with("<?") {
                self.processing_instruction()?;
            } else if self.rest().starts_with("<!") {
                return self.fail("a declaration inside an element");
            } else if self.rest().starts_with('<') {
                children.push(Node::Element(self.element()?));
            } else {
                let from = self.at;
                while self.bytes.get(self.at).is_some_and(|b| *b != b'<') {
                    self.at += 1;
                }
                let text = unescape(&self.text[from..self.at], from)?;
                push_text(&mut children, text, from);
            }
        }
    }
}

/// Adjacent text (a CDATA section beside characters) is one text node.
fn push_text(children: &mut Vec<Node>, text: String, at: usize) {
    if let Some(Node::Text(last, _)) = children.last_mut() {
        last.push_str(&text);
    } else {
        children.push(Node::Text(text, at));
    }
}

fn local(name: &str) -> String {
    name.rsplit_once(':')
        .map_or(name, |(_, local)| local)
        .to_string()
}

/// `xml:space` is `space`; `xmlns:x` stays whole, since only the default
/// namespace is read.
fn attribute_key(key: &str) -> String {
    if key.starts_with("xmlns") {
        key.to_string()
    } else {
        local(key)
    }
}

/// Text with its character references read.
fn unescape(raw: &str, base: usize) -> Result<String, Failure> {
    if !raw.contains('&') {
        return Ok(raw.to_string());
    }
    let mut out = String::new();
    let mut rest = raw;
    let mut offset = base;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semi) = after.find(';') else {
            return Err((
                offset + amp,
                "a `&` that is never closed with `;`".to_string(),
            ));
        };
        let reference = &after[..semi];
        let c = match reference {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => match reference.strip_prefix('#') {
                Some(number) => {
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => number.parse::<u32>().ok(),
                    };
                    match code.and_then(char::from_u32).filter(|c| allowed(*c)) {
                        Some(c) => c,
                        None => {
                            return Err((
                                offset + amp,
                                format!("`&{reference};` is not a character XML allows"),
                            ));
                        }
                    }
                }
                None => {
                    return Err((
                        offset + amp,
                        format!(
                            "`&{reference};` is not an entity this reads (only `amp`, `lt`, `gt`, `quot`, `apos` and numbers)"
                        ),
                    ));
                }
            },
        };
        out.push(c);
        let used = amp + 1 + semi + 1;
        offset += used;
        rest = &rest[used..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The characters XML 1.0 allows in a document.
pub fn allowed(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r')
        || ('\u{20}'..='\u{d7ff}').contains(&c)
        || ('\u{e000}'..='\u{fffd}').contains(&c)
        || c >= '\u{10000}'
}

/// Text for element content: the characters that mean something escaped,
/// and a carriage return as a reference, since a parser reads a bare one
/// as a line end.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// Text for a double-quoted attribute value.
pub fn escape_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_and_references() {
        let root = parse(
            "<?xml version=\"1.0\"?>\n<!-- c --><a xmlns=\"u\" x:b='1 &amp; 2'>t&lt;<b/><![CDATA[<c>]]></a>",
        )
        .expect("reads");
        assert_eq!(root.name, "a");
        assert_eq!(root.attribute("b"), Some("1 & 2"));
        assert_eq!(root.attribute("xmlns"), Some("u"));
        assert_eq!(root.children.len(), 3);
        assert!(matches!(&root.children[0], Node::Text(text, _) if text == "t<"));
        assert!(matches!(&root.children[2], Node::Text(text, _) if text == "<c>"));
    }

    #[test]
    fn refuses_what_is_not_well_formed() {
        for bad in [
            "",
            "<a>",
            "<a></b>",
            "<a b=1/>",
            "<a>&nope;</a>",
            "<!DOCTYPE a><a/>",
            "<a/><b/>",
            "<a>&#0;</a>",
            "<a b='1' b='2'/>",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn writes_what_it_reads() {
        let text = "a < b & c > \"d\"\r\n";
        let doc = format!("<t>{}</t>", escape_text(text));
        let root = parse(&doc).expect("reads");
        assert!(matches!(&root.children[..], [Node::Text(read, 3)] if read == text));
        let doc = format!("<t v=\"{}\"/>", escape_attribute("a\"b\nc"));
        assert_eq!(parse(&doc).expect("reads").attribute("v"), Some("a\"b\nc"));
    }
}
