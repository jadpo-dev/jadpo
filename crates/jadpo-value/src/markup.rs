//! `markup`'s operations (area 19, "Text formats", chunk 5.5o): CommonMark
//! rendered as `Html`, and the text people read in `Html`, over
//! `jadpo_std::standard::markup`, whose sanitiser every `Html` value is
//! built with.

use jadpo_std::standard::markup::{plain_text, sanitise};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{Fault, Value};

/// `markdown` rendered by CommonMark, with none of pulldown-cmark's
/// extensions, then made safe as every `Html` is. Raw HTML inside it is
/// rendered as CommonMark says and kept only where the sanitiser keeps it;
/// an image, which `Html` doesn't keep, is shown as its description.
pub fn render(markdown: &str) -> String {
    // An image's description is the text between its start and end, so
    // leaving out the two keeps the words and drops the image.
    let events = Parser::new_ext(markdown, Options::empty()).filter(|event| {
        !matches!(
            event,
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image)
        )
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    sanitise(&html)
}

/// `markup.to_html(markdown)`.
pub fn to_html(markdown: &Value) -> Result<Value, Fault> {
    match markdown {
        Value::Text(text) => Ok(Value::text(&render(text))),
        _ => Err(Fault::Shape),
    }
}

/// `markup.to_plain_text(html)`.
pub fn to_plain_text(html: &Value) -> Result<Value, Fault> {
    match html {
        Value::Text(text) => Ok(Value::text(&plain_text(text))),
        _ => Err(Fault::Shape),
    }
}

/// The text people read in a `Markdown` text: its rendering's plain text,
/// as `text.format` shows it (chunk 5.5o).
pub fn markdown_text(markdown: &str) -> String {
    plain_text(&render(markdown))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commonmark_is_rendered_then_made_safe() {
        assert_eq!(
            render("# Menu\n\nSoup of the **day**."),
            "<h1>Menu</h1>\n<p>Soup of the <strong>day</strong>.</p>\n"
        );
        assert_eq!(
            render("[site](https://example.com/) and [bad](javascript:alert(1))"),
            "<p><a href=\"https://example.com/\" rel=\"noopener noreferrer nofollow\">site</a> and <a rel=\"noopener noreferrer nofollow\">bad</a></p>\n"
        );
        assert_eq!(
            render("Hi <script>alert(1)</script><b onclick=x>there</b>"),
            "<p>Hi <b>there</b></p>\n"
        );
        assert_eq!(
            render("<div onmouseover=\"alert(1)\">\n\nblock\n\n</div>"),
            "\n<p>block</p>\n"
        );
        assert_eq!(
            render("![a cat](https://example.com/cat.png)"),
            "<p>a cat</p>\n"
        );
        assert_eq!(
            render("```rust\nlet x = 1;\n```"),
            "<pre><code>let x = 1;\n</code></pre>\n"
        );
        assert_eq!(
            render("[x](//evil.example/)"),
            "<p><a rel=\"noopener noreferrer nofollow\">x</a></p>\n"
        );
        assert_eq!(
            markdown_text("# Menu\n\n- Soup\n- Bread"),
            "Menu\nSoup\nBread"
        );
    }
}
