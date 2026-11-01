//! A small element tree over `quick_xml`, for the site files of other clients: they nest
//! folders, mix text with elements, and are small enough to read whole.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Element {
    /// Without any namespace prefix.
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// The element's own text, not its children's.
    pub text: String,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children
            .iter()
            .find(|child| child.name.eq_ignore_ascii_case(name))
    }

    /// The trimmed text of the first child called `name`, if it has any.
    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name)
            .map(|child| child.text.trim())
            .filter(|text| !text.is_empty())
    }

    /// This element and every element inside it, depth first.
    pub fn descendants(&self) -> Vec<&Element> {
        let mut found = vec![self];
        for child in &self.children {
            found.extend(child.descendants());
        }
        found
    }
}

pub fn parse(text: &str) -> AppResult<Element> {
    let mut reader = Reader::from_str(text);
    // The root collects the document's top-level elements.
    let mut stack = vec![Element::default()];
    loop {
        let event = reader
            .read_event()
            .map_err(|error| AppError::invalid(format!("The file is not valid XML: {error}")))?;
        match event {
            Event::Start(start) => stack.push(element(&start)),
            Event::Empty(start) => {
                let empty = element(&start);
                stack
                    .last_mut()
                    .expect("the root is never popped")
                    .children
                    .push(empty);
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let finished = stack.pop().expect("checked above");
                    stack
                        .last_mut()
                        .expect("checked above")
                        .children
                        .push(finished);
                }
            }
            Event::Text(text) => current(&mut stack).text.push_str(&text.xml10_content()),
            Event::CData(data) => {
                current(&mut stack).text.push_str(&data.into_inner());
            }
            Event::GeneralRef(reference) => {
                let resolved = match reference.resolve_char_ref() {
                    Ok(Some(character)) => character.to_string(),
                    _ => resolve_predefined_entity(&reference)
                        .unwrap_or_default()
                        .to_string(),
                };
                current(&mut stack).text.push_str(&resolved);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    // Elements left open at the end of a cut-off file still count.
    while stack.len() > 1 {
        let open = stack.pop().expect("checked by the loop");
        current(&mut stack).children.push(open);
    }
    let mut root = stack.pop().expect("the root is never popped");
    match root.children.len() {
        0 => Err(AppError::invalid("The file has no XML elements")),
        1 => Ok(root.children.remove(0)),
        _ => Ok(root),
    }
}

fn current(stack: &mut [Element]) -> &mut Element {
    stack.last_mut().expect("the root is never popped")
}

fn element(start: &BytesStart) -> Element {
    let attributes = start
        .attributes()
        .flatten()
        .map(|attribute| {
            let key = attribute.key.local_name().as_ref().to_string();
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|value| value.into_owned())
                .unwrap_or_else(|_| attribute.value.to_string());
            (key, value)
        })
        .collect();
    Element {
        name: start.local_name().as_ref().to_string(),
        attributes,
        ..Element::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_elements_text_and_references() {
        let root = parse(
            r#"<?xml version="1.0"?>
            <a:Root xmlns:a="urn:x"><Folder expanded="1">Work &amp; play&#x0A;<Server><Host>h</Host></Server></Folder><Empty/></a:Root>"#,
        )
        .unwrap();
        assert_eq!(root.name, "Root");
        let folder = root.child("folder").unwrap();
        assert_eq!(folder.attribute("expanded"), Some("1"));
        assert_eq!(folder.text.trim(), "Work & play");
        assert_eq!(
            folder.child("Server").unwrap().child_text("Host"),
            Some("h")
        );
        assert!(root.child("Empty").is_some());
        assert_eq!(root.descendants().len(), 5);
    }

    #[test]
    fn rejects_what_is_not_xml() {
        assert!(parse("just words").is_err());
    }
}
