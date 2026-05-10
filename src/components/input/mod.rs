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
                RichTextSpan::UserMention { .. } | RichTextSpan::RoomMention => {
                    tokens.remove(t_idx);

                    let (new_t_idx, new_c_idx) = if t_idx == 0 {
                        (0, 0)
                    } else {
                        let target_t_idx = t_idx - 1;
                        let new_c_idx = match tokens.get(target_t_idx) {
                            Some(RichTextSpan::Plain(text)) => text.len(),
                            Some(RichTextSpan::Link { url, .. }) => url.len(),
                            Some(_) => 1,
                            None => 0,
                        };

                        (target_t_idx, new_c_idx)
                    };

                    set_caret_position.set((new_t_idx, new_c_idx));
                    return;
                }
                _ => {}
            }
        }
    }

    if t_idx > 0 {
        let is_empty_plain = match tokens.get(t_idx) {
            Some(RichTextSpan::Plain(text)) => text.is_empty(),
            _ => false,
        };

        if is_empty_plain {
            tokens.remove(t_idx);
        }

        tokens.remove(t_idx - 1);

        let target_t_idx = (t_idx - 1).saturating_sub(1);

        let new_c_idx = match tokens.get(target_t_idx) {
            Some(RichTextSpan::Plain(text)) => text.len(),
            Some(RichTextSpan::Link { url, .. }) => url.len(),
            Some(_) => 1,
            None => 0,
        };

        set_caret_position.set((target_t_idx, new_c_idx));
    }
}

pub fn input_delete(caret_position: (usize, usize), tokens: &mut Vec<RichTextSpan>) -> () {
    let (t_idx, c_idx) = caret_position;

    if tokens.is_empty() || (t_idx >= tokens.len() && c_idx == 0) {
        return;
    }

    if t_idx < tokens.len() {
        if let Some(token) = tokens.get_mut(t_idx) {
            match token {
                RichTextSpan::Plain(text) => {
                    if c_idx < text.len() {
                        text.remove(c_idx);
                    } else if t_idx < tokens.len() - 1 {
                        tokens.remove(t_idx + 1);
                    }
                    return;
                }
                RichTextSpan::Link { url, .. } => {
                    if c_idx < url.len() {
                        url.remove(c_idx);
                    } else if t_idx < tokens.len() - 1 {
                        tokens.remove(t_idx + 1);
                    }
                    return;
                }
                RichTextSpan::UserMention { .. } | RichTextSpan::RoomMention => {
                    if c_idx == 0 {
                        tokens.remove(t_idx);
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    if t_idx < tokens.len() - 1 {
        tokens.remove(t_idx + 1);
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
            RichTextSpan::Link { url, .. } if c != ' ' => {
                url.insert(c_idx, c);
                set_caret_position.set((t_idx, c_idx + 1));
            }
            RichTextSpan::UserMention { .. }
            | RichTextSpan::RoomMention
            | RichTextSpan::Newline => {
                let insert_idx = if c_idx == 0 { t_idx } else { t_idx + 1 };
                tokens.insert(insert_idx, RichTextSpan::Plain(c.to_string()));
                set_caret_position.set((insert_idx, 1));
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

fn caret_abs_offset(tokens: &Vec<RichTextSpan>, t_idx: usize, c_idx: usize) -> usize {
    tokens.iter().take(t_idx).map(|t| t.len()).sum::<usize>() + c_idx
}

fn caret_from_abs_offset(tokens: &Vec<RichTextSpan>, abs_offset: usize) -> (usize, usize) {
    let mut current_sum = 0;

    for (idx, token) in tokens.iter().enumerate() {
        let length = token.len();
        let end = current_sum + length;

        if abs_offset <= end {
            let new_c_idx = if abs_offset < end {
                abs_offset - current_sum
            } else {
                length
            };

            return (idx, new_c_idx);
        }

        current_sum = end;
    }

    if let Some(last) = tokens.last() {
        (tokens.len().saturating_sub(1), last.len())
    } else {
        (0, 0)
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

        let mut abs_offset = caret_abs_offset(tokens, *t_idx, *c_idx);

        if abs_offset == 0 {
            return;
        }

        abs_offset -= 1;

        let (new_t_idx, new_c_idx) = caret_from_abs_offset(tokens, abs_offset);
        *t_idx = new_t_idx;
        *c_idx = new_c_idx;

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
        if tokens.is_empty() {
            return;
        }

        let total_len = tokens.iter().map(|t| t.len()).sum::<usize>();
        let mut abs_offset = caret_abs_offset(tokens, *t_idx, *c_idx);

        if abs_offset >= total_len {
            return;
        }

        abs_offset += 1;

        let (new_t_idx, new_c_idx) = caret_from_abs_offset(tokens, abs_offset);
        *t_idx = new_t_idx;
        *c_idx = new_c_idx;

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
