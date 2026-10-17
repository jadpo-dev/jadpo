//! XLIFF 2.0 (OASIS): one `<file>` for each scope, one `<unit>` for each
//! wording with its notes and one `<segment>` with a source and, once
//! translated, a target. A wording is written as it is in the language
//! file, `{field}` placeholders and doubled braces included, so a service
//! sees ordinary text and nothing here needs to guess at its inline codes.

use super::xml::{self, Element, Node};
use super::{InFile, InUnit, OutFile, Unreadable};

const NAMESPACE: &str = "urn:oasis:names:tc:xliff:document:2.0";

pub fn write(file: &OutFile) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<xliff xmlns=\"{NAMESPACE}\" version=\"2.0\" srcLang=\"{}\"",
        xml::escape_attribute(&file.source_language)
    ));
    if let Some(target) = &file.target_language {
        out.push_str(&format!(" trgLang=\"{}\"", xml::escape_attribute(target)));
    }
    out.push_str(">\n");
    let mut scope: Option<&str> = None;
    for unit in &file.units {
        if scope != Some(unit.scope.as_str()) {
            if scope.is_some() {
                out.push_str("  </file>\n");
            }
            out.push_str(&format!(
                "  <file id=\"{}\" original=\"{}\">\n",
                xml::escape_attribute(&unit.scope),
                xml::escape_attribute(&unit.folder)
            ));
            scope = Some(unit.scope.as_str());
        }
        out.push_str(&format!(
            "    <unit id=\"{}\">\n",
            xml::escape_attribute(&unit.id)
        ));
        if !unit.notes.is_empty() {
            out.push_str("      <notes>\n");
            for (category, note) in &unit.notes {
                out.push_str(&format!(
                    "        <note category=\"{category}\">{}</note>\n",
                    xml::escape_text(note)
                ));
            }
            out.push_str("      </notes>\n");
        }
        let state = match (&unit.target, unit.reviewed) {
            (None, _) => "initial",
            (Some(_), true) => "reviewed",
            (Some(_), false) => "translated",
        };
        out.push_str(&format!("      <segment state=\"{state}\">\n"));
        out.push_str(&format!(
            "        <source xml:space=\"preserve\">{}</source>\n",
            xml::escape_text(&unit.source)
        ));
        if let Some(target) = &unit.target {
            out.push_str(&format!(
                "        <target xml:space=\"preserve\">{}</target>\n",
                xml::escape_text(target)
            ));
        }
        out.push_str("      </segment>\n    </unit>\n");
    }
    if scope.is_some() {
        out.push_str("  </file>\n");
    }
    out.push_str("</xliff>\n");
    out
}

pub fn read(text: &str) -> Result<InFile, Unreadable> {
    let unreadable = |at: usize, why: &str| Unreadable {
        at,
        why: why.to_string(),
    };
    let root = xml::parse(text).map_err(|(at, why)| Unreadable {
        at,
        why: format!("it is not well-formed XML: {why}"),
    })?;
    if root.name != "xliff" {
        return Err(unreadable(
            root.start,
            &format!("its root element is `<{}>`, not `<xliff>`", root.name),
        ));
    }
    let version = root.attribute("version").unwrap_or("");
    if !version.starts_with("2.") {
        return Err(unreadable(
            root.start,
            &format!(
                "it is XLIFF {}, and Jadpo reads XLIFF 2.0 and 2.1; ask the service to export 2.x",
                if version.is_empty() {
                    "of no version"
                } else {
                    version
                }
            ),
        ));
    }
    if root.attribute("xmlns") != Some(NAMESPACE) {
        return Err(unreadable(
            root.start,
            &format!("its default namespace is not `{NAMESPACE}`"),
        ));
    }
    let mut out = InFile {
        source_language: root.attribute("srcLang").map(str::to_string),
        target_language: root.attribute("trgLang").map(str::to_string),
        units: Vec::new(),
    };
    for file in root.elements("file") {
        let Some(scope) = file.attribute("id") else {
            return Err(unreadable(file.start, "a `<file>` has no `id`"));
        };
        collect(file, scope, &mut out.units)?;
    }
    for (at, unit) in out.units.iter().enumerate() {
        if let Some(first) = out.units[..at].iter().find(|seen| seen.id == unit.id) {
            let _ = first;
            return Err(unreadable(
                unit.start,
                &format!("the unit `{}` is in the file twice", unit.id),
            ));
        }
    }
    Ok(out)
}

/// The units under a file or group, depth first.
fn collect(parent: &Element, scope: &str, out: &mut Vec<InUnit>) -> Result<(), Unreadable> {
    for node in &parent.children {
        let Node::Element(child) = node else { continue };
        match child.name.as_str() {
            "group" => collect(child, scope, out)?,
            "unit" => out.push(unit(child, scope)?),
            _ => {}
        }
    }
    Ok(())
}

fn unit(element: &Element, scope: &str) -> Result<InUnit, Unreadable> {
    let Some(id) = element.attribute("id") else {
        return Err(Unreadable {
            at: element.start,
            why: "a `<unit>` has no `id`".to_string(),
        });
    };
    let segments: Vec<&Element> = element.elements("segment").collect();
    let [segment] = segments.as_slice() else {
        return Err(Unreadable {
            at: element.start,
            why: format!(
                "the unit `{scope}.{id}` has {} segments, and a wording is one; turn segmentation off in the service",
                segments.len()
            ),
        });
    };
    let source = segment.first("source").map(inline);
    let target = segment.first("target");
    Ok(InUnit {
        id: format!("{scope}.{id}"),
        source,
        target: target.map(inline),
        start: target.map_or(segment.start, |target| target.start),
        end: target.map_or(segment.end, |target| target.end),
    })
}

/// The text of a source or target, with inline codes read as the text they
/// stand for.
fn inline(element: &Element) -> String {
    let mut out = String::new();
    for node in &element.children {
        match node {
            Node::Text(text, _) => out.push_str(text),
            Node::Element(child) => match child.name.as_str() {
                "ph" => out.push_str(child.attribute("equiv").unwrap_or("")),
                "cp" => {
                    if let Some(c) = child
                        .attribute("hex")
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .and_then(char::from_u32)
                    {
                        out.push(c);
                    }
                }
                "pc" => {
                    out.push_str(child.attribute("equivStart").unwrap_or(""));
                    out.push_str(&inline(child));
                    out.push_str(child.attribute("equivEnd").unwrap_or(""));
                }
                "sc" => out.push_str(child.attribute("equiv").unwrap_or("")),
                "ec" => out.push_str(child.attribute("equiv").unwrap_or("")),
                _ => out.push_str(&inline(child)),
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::OutUnit;
    use super::*;

    fn unit(scope: &str, id: &str, source: &str, target: Option<&str>) -> OutUnit {
        OutUnit {
            scope: scope.to_string(),
            folder: format!("domains/{scope}/translations"),
            id: id.to_string(),
            notes: vec![("description", "Says \"hi\" & <more>.".to_string())],
            source: source.to_string(),
            target: target.map(str::to_string),
            reviewed: false,
        }
    }

    #[test]
    fn what_is_written_is_read_back() {
        let file = OutFile {
            source_language: "en-GB".to_string(),
            target_language: Some("de-DE".to_string()),
            units: vec![
                unit(
                    "shipping",
                    "parcels.one",
                    "{count} parcel",
                    Some("{count} Paket"),
                ),
                unit(
                    "shipping",
                    "parcels.other",
                    "{{x}}\n {count} \"parcels\" & <b>",
                    None,
                ),
                unit("shop", "farewell", "Bye.", Some("  Tschüss.\r\n")),
            ],
        };
        let text = write(&file);
        let read = read(&text).expect("reads");
        assert_eq!(read.source_language.as_deref(), Some("en-GB"));
        assert_eq!(read.target_language.as_deref(), Some("de-DE"));
        let units: Vec<(String, Option<String>, Option<String>)> = read
            .units
            .iter()
            .map(|u| (u.id.clone(), u.source.clone(), u.target.clone()))
            .collect();
        assert_eq!(
            units,
            vec![
                (
                    "shipping.parcels.one".to_string(),
                    Some("{count} parcel".to_string()),
                    Some("{count} Paket".to_string())
                ),
                (
                    "shipping.parcels.other".to_string(),
                    Some("{{x}}\n {count} \"parcels\" & <b>".to_string()),
                    None
                ),
                (
                    "shop.farewell".to_string(),
                    Some("Bye.".to_string()),
                    Some("  Tschüss.\r\n".to_string())
                ),
            ]
        );
    }

    #[test]
    fn reads_inline_codes_groups_and_other_prefixes() {
        let text = r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:2.0" xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.1" srcLang="en" trgLang="de">
  <x:file id="f"><x:group id="g"><x:unit id="u"><x:segment>
    <x:source>Hi <ph id="1" equiv="{name}"/></x:source>
    <x:target>Hallo <pc id="2" equivStart="[" equivEnd="]">du</pc> <ph id="1" equiv="{name}"/></x:target>
  </x:segment></x:unit></x:group></x:file></x:xliff>"#;
        let read = read(text).expect("reads");
        assert_eq!(read.units[0].id, "f.u");
        assert_eq!(read.units[0].source.as_deref(), Some("Hi {name}"));
        assert_eq!(read.units[0].target.as_deref(), Some("Hallo [du] {name}"));
    }

    #[test]
    fn refuses_other_versions_and_shapes() {
        let v12 = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2"/>"#;
        assert!(read(v12).expect_err("1.2").why.contains("XLIFF 1.2"));
        let two = |body: &str| {
            format!("<xliff xmlns=\"{NAMESPACE}\" version=\"2.0\" srcLang=\"en\">{body}</xliff>")
        };
        let split = two("<file id=\"f\"><unit id=\"u\"><segment/><segment/></unit></file>");
        assert!(
            read(&split)
                .expect_err("segments")
                .why
                .contains("2 segments")
        );
        let twice = two(
            "<file id=\"f\"><unit id=\"u\"><segment/></unit><unit id=\"u\"><segment/></unit></file>",
        );
        assert!(read(&twice).expect_err("twice").why.contains("twice"));
        assert!(read("<other/>").is_err());
        assert!(read("not xml").is_err());
    }
}
