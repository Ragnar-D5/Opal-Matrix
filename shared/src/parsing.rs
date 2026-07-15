use crate::timeline::{RichTextSpan, RoomIdFormat};
use ego_tree::NodeRef;
use linkify::LinkFinder;
use scraper::{Html, Node};

fn walk_node(node: NodeRef<'_, Node>, spans: &mut Vec<RichTextSpan>) {
    match node.value() {
        Node::Text(text) => {
            let content = text.text.to_string();
            if !content.is_empty() {
                spans.push(RichTextSpan::Plain(content));
            }
        }

        Node::Element(elem) => {
            let tag_name = elem.name();

            if tag_name == "br" {
                spans.push(RichTextSpan::Plain("\n".to_string()));
                return;
            }

            if tag_name == "a"
                && let Some(href) = elem.attr("href")
            {
                if let Some(id_str) = href.strip_prefix("https://matrix.to/#/") {
                    let display_string = extract_inner_text(node);

                    let span = if let Some(starting_char) = id_str.chars().next() {
                        match starting_char {
                            '#' => RichTextSpan::RoomMention {
                                room_id: RoomIdFormat::Alias(id_str.to_string()),
                                display_name: display_string,
                            },
                            '!' => RichTextSpan::RoomMention {
                                room_id: RoomIdFormat::Id(id_str.to_string()),
                                display_name: display_string,
                            },
                            '@' => {
                                let user_id = extract_mxid(id_str);
                                RichTextSpan::UserMention {
                                    user_id,
                                    display_name: display_string,
                                }
                            }
                            _ => {
                                spans.push(RichTextSpan::Link {
                                    url: href.to_string(),
                                    text: Some(display_string),
                                });
                                return;
                            }
                        }
                    } else {
                        RichTextSpan::Link {
                            url: href.to_string(),
                            text: None,
                        }
                    };

                    spans.push(span);
                    return;
                } else {
                    spans.push(RichTextSpan::Link {
                        url: href.to_string(),
                        text: None,
                    });
                    return;
                }
            }

            for child in node.children() {
                walk_node(child, spans);
            }
        }

        _ => {
            for child in node.children() {
                walk_node(child, spans);
            }
        }
    }
}

fn extract_inner_text(node: NodeRef<'_, Node>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let Node::Text(t) = child.value() {
            text.push_str(&t.text);
        } else {
            text.push_str(&extract_inner_text(child));
        }
    }
    text
}

fn extract_mxid(href: &str) -> String {
    if let Some(idx) = href.find('@') {
        href[idx..].to_string()
    } else {
        href.to_string()
    }
}

pub fn parse_html_to_spans(html: &str, fallback_body: &str) -> Vec<RichTextSpan> {
    let document = Html::parse_fragment(html);
    let mut spans = Vec::new();

    for node in document.tree.root().children() {
        walk_node(node, &mut spans);
    }

    if spans.is_empty() {
        vec![RichTextSpan::Plain(fallback_body.to_string())]
    } else {
        spans
    }
}

pub fn parse_plain_text_to_spans(text: &str) -> Vec<RichTextSpan> {
    let mut spans = Vec::new();
    let mut finder = LinkFinder::new();
    finder.kinds(&[linkify::LinkKind::Url]);

    let mut last_end = 0;

    for link in finder.links(text) {
        if link.start() > last_end {
            spans.push(RichTextSpan::Plain(
                text[last_end..link.start()].to_string(),
            ));
        }

        spans.push(RichTextSpan::Link {
            url: link.as_str().to_string(),
            text: None,
        });

        last_end = link.end();
    }

    if last_end < text.len() {
        spans.push(RichTextSpan::Plain(text[last_end..].to_string()));
    }

    if spans.is_empty() {
        vec![RichTextSpan::Plain(text.to_string())]
    } else {
        spans
    }
}
