use crate::timeline::{RichTextSpan, RoomIdFormat};
use ego_tree::NodeRef;
use linkify::LinkFinder;
use ruma::{OwnedRoomId, OwnedUserId};
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

                    let room_id = OwnedRoomId::try_from(id_str).ok();
                    let user_id = OwnedUserId::try_from(id_str).ok();

                    if let Some(room_id) = room_id {
                        let span = RichTextSpan::RoomMention {
                            room_id: RoomIdFormat::Id(room_id),
                            display_name: display_string,
                        };
                        spans.push(span);
                        return;
                    }

                    if let Some(user_id) = user_id {
                        let span = RichTextSpan::UserMention {
                            user_id,
                            display_name: display_string,
                        };
                        spans.push(span);
                        return;
                    }

                    if id_str.starts_with('#') {
                        let span = RichTextSpan::RoomMention {
                            room_id: RoomIdFormat::Alias(id_str.to_string()),
                            display_name: display_string,
                        };
                        spans.push(span);
                        return;
                    }

                    spans.push(RichTextSpan::Link {
                        url: href.to_string(),
                        text: Some(display_string),
                    });

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
