use std::cell::{Cell, RefCell};
use std::time::Duration;

use leptos::prelude::{set_interval_with_handle, IntervalHandle};
use leptos::tachys::dom::document;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, Node, NodeFilter, Text};

const REDACTED_CLASS: &str = "redacted";
const REDACT_PERIOD: Duration = Duration::from_secs(2);

// Marks an element with a stable id (independent of its text) the first time
// we see it, so the same element gets the same redaction pattern on every
// later pass (room switch, periodic re-run, ...) even if its text changes.
const ID_ATTR: &str = "data-redact-id";

thread_local! {
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
}

// The fraction (0.0-1.0) of eligible words to redact, as last set via
// `set_redaction_mode`. Read fresh by the periodic re-run so a slider change
// takes effect without restarting the interval.
thread_local! {
    static CURRENT_PERCENTAGE: Cell<f64> = const { Cell::new(0.0) };
}

/// Redacts every piece of text in the document (script/style/input/textarea
/// content is always skipped regardless of root).
pub const REDACTION_ROOT_SELECTOR: &str = "body";

// `NodeFilter`'s `SHOW_*`/`FILTER_*` constants per the DOM spec; web-sys doesn't
// generate them since they're plain `const` WebIDL members, not methods.
const SHOW_TEXT: u32 = 0x4;
const FILTER_ACCEPT: u16 = 1;
const FILTER_REJECT: u16 = 2;

// Only one redaction cycle should ever be running; re-entering `set_redaction_mode(true, ..)`
// (e.g. on every room change) must not stack up duplicate intervals.
thread_local! {
    static PERIODIC_HANDLE: RefCell<Option<IntervalHandle>> = const { RefCell::new(None) };
}

/// Call this whenever the redaction setting (or the active room) changes.
/// `percentage` is 0.0-100.0, the percentage of eligible words to redact
/// (`0.0` redacts nothing, clearing any existing redactions). Redacts
/// immediately, deterministically, and keeps re-redacting every 2 seconds to
/// cover newly rendered content.
pub fn set_redaction_mode(percentage: f64, container_selector: &'static str) {
    CURRENT_PERCENTAGE.with(|p| p.set(percentage));
    redact_now(container_selector);
    start_periodic_redaction(container_selector);
}

fn start_periodic_redaction(container_selector: &'static str) {
    PERIODIC_HANDLE.with(|handle| {
        if handle.borrow().is_some() {
            return;
        }
        let new_handle =
            set_interval_with_handle(move || redact_now(container_selector), REDACT_PERIOD).ok();
        *handle.borrow_mut() = new_handle;
    });
}

fn redact_now(container_selector: &str) {
    clear_redactions();
    redact_words(container_selector);
}

pub fn clear_redactions() {
    let doc = document();
    let Ok(nodes) = doc.query_selector_all(&format!(".{REDACTED_CLASS}")) else {
        return;
    };

    for i in 0..nodes.length() {
        let Some(el) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
            continue;
        };
        let Some(parent) = el.parent_node() else {
            continue;
        };
        let text = doc.create_text_node(&el.text_content().unwrap_or_default());
        let _ = parent.replace_child(&text, &el);
        parent.normalize();
    }
}

fn redact_words(container_selector: &str) {
    let doc = document();
    let Some(container) = doc.query_selector(container_selector).ok().flatten() else {
        return;
    };

    let text_nodes = collect_text_nodes(&container);
    if text_nodes.is_empty() {
        return;
    }

    let percentage = CURRENT_PERCENTAGE.with(|p| p.get());

    // Split each text node into alternating word/whitespace tokens and decide,
    // per word, whether it gets redacted. The decision is keyed by the parent
    // element's stable id and the word's position, not by the word's text, so
    // the same element keeps the same redaction pattern across passes even if
    // its text changes.
    for node in &text_nodes {
        let Some(parent) = node.parent_element() else {
            continue;
        };
        let id = element_id(&parent);

        let parts = split_preserving_whitespace(&node.text_content().unwrap_or_default());
        let decisions: Vec<bool> = parts
            .iter()
            .enumerate()
            .map(|(word_idx, part)| should_redact(id, word_idx, part, percentage))
            .collect();

        if !decisions.iter().any(|&redact| redact) {
            continue;
        }

        let frag = doc.create_document_fragment();
        for (part, &redact) in parts.iter().zip(&decisions) {
            if redact {
                let span = doc.create_element("span").unwrap();
                span.set_class_name(REDACTED_CLASS);
                span.set_text_content(Some(part));
                let _ = frag.append_child(&span);
            } else {
                let _ = frag.append_child(&doc.create_text_node(part));
            }
        }
        let _ = parent.replace_child(&frag, node);
    }
}

/// Returns the element's stable redaction id, assigning and stamping a new one
/// (via `data-redact-id`) the first time it's seen.
fn element_id(el: &Element) -> u32 {
    if let Some(id) = el.get_attribute(ID_ATTR).and_then(|v| v.parse().ok()) {
        return id;
    }

    let id = NEXT_ID.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    let _ = el.set_attribute(ID_ATTR, &id.to_string());
    id
}

/// Equivalent to JS's `text.split(/(\s+)/)`: alternating word/whitespace tokens.
fn split_preserving_whitespace(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_space = false;

    for ch in text.chars() {
        let is_space = ch.is_whitespace();
        if current.is_empty() {
            in_space = is_space;
        } else if is_space != in_space {
            parts.push(std::mem::take(&mut current));
            in_space = is_space;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Deterministically decides whether a word gets redacted, keyed by the parent
/// element's stable id and the word's position within it — never by the word's
/// own text. Words shorter than 3 chars are never candidates.
fn should_redact(element_id: u32, word_idx: usize, word: &str, percentage: f64) -> bool {
    if word.trim().chars().count() <= 2 {
        return false;
    }

    let threshold = percentage.clamp(0.0, 100.0) as u32;
    fnv1a(&[element_id.to_le_bytes(), (word_idx as u32).to_le_bytes()].concat()) % 100 < threshold
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in bytes {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Walks all text nodes inside `container`, skipping script/style/input/textarea
/// content.
fn collect_text_nodes(container: &Element) -> Vec<Text> {
    let doc = document();

    let filter = Closure::wrap(Box::new(move |node: Node| -> u16 {
        let Some(parent) = node.parent_element() else {
            return FILTER_REJECT;
        };

        let tag = parent.tag_name().to_lowercase();
        if matches!(tag.as_str(), "script" | "style" | "input" | "textarea") {
            return FILTER_REJECT;
        }

        let has_text = node
            .text_content()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);

        if has_text {
            FILTER_ACCEPT
        } else {
            FILTER_REJECT
        }
    }) as Box<dyn FnMut(Node) -> u16>);

    let node_filter = NodeFilter::new();
    node_filter.set_accept_node(filter.as_ref().unchecked_ref());

    let mut nodes = Vec::new();
    if let Ok(walker) = doc.create_tree_walker_with_what_to_show_and_filter(
        container,
        SHOW_TEXT,
        Some(&node_filter),
    ) {
        while let Ok(Some(n)) = walker.next_node() {
            if let Ok(text) = n.dyn_into::<Text>() {
                nodes.push(text);
            }
        }
    }
    // `filter` must outlive the synchronous traversal above; dropped here.
    nodes
}
