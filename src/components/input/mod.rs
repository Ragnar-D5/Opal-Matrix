use leptos::prelude::*;
use log::warn;
use shared::messages::RichTextSpan;

use crate::components::RichTextExt;

pub mod menu;

pub fn input_backspace(
    caret_position: (usize, usize),
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &mut Vec<RichTextSpan>,
) -> () {
    let (t_idx, c_idx) = caret_position;

    if tokens.is_empty() || (t_idx == 0 && c_idx == 0) {
        return;
    }

    // Case 1: Deleting characters normally inside a token
    if c_idx > 0 {
        if let Some(token) = tokens.get_mut(t_idx) {
            match token {
                RichTextSpan::Plain(text) => {
                    text.remove(c_idx - 1);
                    set_caret_position.update(|(_, c)| *c -= 1);
                    return;
                }
                RichTextSpan::Link { url, .. } => {
                    url.remove(c_idx - 1);
                    set_caret_position.update(|(_, c)| *c -= 1);
                    return;
                }
                _ => {}
            }
        }
    }

    // Case 2: Boundary Deletion (c_idx == 0)
    // We are at the start of a token, so we need to delete the PRECEDING token.
    if t_idx > 0 {
        // First, if we are sitting on a useless empty string, clean it up
        let is_empty_plain = match tokens.get(t_idx) {
            Some(RichTextSpan::Plain(text)) => text.is_empty(),
            _ => false,
        };

        if is_empty_plain {
            tokens.remove(t_idx);
        }

        // Delete the block token before it (e.g., the Newline or a Mention)
        tokens.remove(t_idx - 1);

        // Calculate where the cursor should land on the token before the deleted one
        let target_t_idx = (t_idx - 1).saturating_sub(1);

        let new_c_idx = match tokens.get(target_t_idx) {
            Some(RichTextSpan::Plain(text)) => text.len(),
            Some(RichTextSpan::Link { url, .. }) => url.len(),
            Some(_) => 1, // Treat other block tokens as length 1
            None => 0,
        };

        set_caret_position.set((target_t_idx, new_c_idx));
    }
}

pub fn input_char(
    c: char,
    caret_position: (usize, usize),
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &mut Vec<RichTextSpan>,
) -> () {
    let (t_idx, c_idx) = caret_position;

    if t_idx < tokens.len() {
        let token = &mut tokens[t_idx];
        match token {
            RichTextSpan::Plain(text) => {
                text.insert(c_idx, c);
                set_caret_position.set((t_idx, c_idx + 1));
            }
            _ => {
                tokens.insert(t_idx + 1, RichTextSpan::Plain(c.to_string()));
                set_caret_position.set((t_idx + 1, 1));
            }
        }
    } else {
        tokens.push(RichTextSpan::Plain(c.to_string()));
        set_caret_position.set((tokens.len().saturating_sub(1), 1));
    }
}

pub fn input_arrow_left(
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &Vec<RichTextSpan>,
) -> () {
    set_caret_position.update(|(t_idx, c_idx)| {
        if tokens.is_empty() {
            return;
        }

        if *c_idx > 0 {
            *c_idx -= 1;
        } else if *t_idx > 0 {
            *t_idx -= 1;
            *c_idx = tokens[*t_idx].len();

            if *c_idx > 0 {
                *c_idx -= 1;
            }
        }

        if let Some(RichTextSpan::Newline) = tokens.get(*t_idx) {
            if *t_idx > 0 {
                *t_idx -= 1;
                *c_idx = tokens[*t_idx].len();
            }
        }
    });
}

pub fn input_arrow_right(
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &Vec<RichTextSpan>,
) -> () {
    set_caret_position.update(|(t_idx, c_idx)| {
        if tokens.is_empty() || *t_idx >= tokens.len() {
            return;
        }

        let len = tokens[*t_idx].len();

        if *c_idx < len {
            *c_idx += 1;
            if *c_idx == len && *t_idx < tokens.len() - 1 {
                *t_idx += 1;
                *c_idx = 0;
            }
        } else if *t_idx < tokens.len() - 1 {
            *t_idx += 1;
            *c_idx = 0;
        }

        if let Some(RichTextSpan::Newline) = tokens.get(*t_idx) {
            if *t_idx < tokens.len() - 1 {
                *t_idx += 1;
                *c_idx = 0;
            }
        }
    });
}

pub fn input_newline(
    caret_position: (usize, usize),
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &mut Vec<RichTextSpan>,
) {
    let (t_idx, c_idx) = caret_position;

    if tokens.is_empty() || t_idx >= tokens.len() {
        tokens.push(RichTextSpan::Newline);
        tokens.push(RichTextSpan::Plain(String::new()));
        set_caret_position.set((tokens.len() - 1, 0));
        return;
    }

    match &tokens[t_idx] {
        RichTextSpan::Plain(text) => {
            let left: String = text.chars().take(c_idx).collect();
            let right: String = text.chars().skip(c_idx).collect();

            tokens[t_idx] = RichTextSpan::Plain(left);

            tokens.insert(t_idx + 1, RichTextSpan::Newline);

            tokens.insert(t_idx + 2, RichTextSpan::Plain(right));
            set_caret_position.set((t_idx + 2, 0));
        }
        _ => {
            tokens.insert(t_idx + 1, RichTextSpan::Newline);
            set_caret_position.set((t_idx + 2, 0));
        }
    }
}

pub fn get_carat_index(x: f32, y: f32) -> Option<(usize, usize)> {
    let document = document();

    if let Some(caret_pos) = document.caret_position_from_point(x, y) {
        if let Some(text_node) = caret_pos.offset_node() {
            let c_idx = caret_pos.offset() as usize;

            if let Some(parent) = text_node.parent_element() {
                if let Some(t_idx_str) = parent.get_attribute("data-t-idx") {
                    if let Ok(t_idx) = t_idx_str.parse::<usize>() {
                        return Some((t_idx, c_idx));
                    } else {
                        warn!("Failed to parse t-index: {t_idx_str}");
                    }
                } else {
                    warn!("No data-t-idx attribute found on parent element");
                }
            } else {
                warn!("Caret offset node has no parent element");
            }
        } else {
            warn!("No caret position found at point ({x}, {y})");
        }
    } else {
        warn!("No caret position found at point ({x}, {y})");
    }

    None
}
