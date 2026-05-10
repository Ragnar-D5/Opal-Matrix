use leptos::{html::Span, prelude::*};
use log::{info, warn};
use shared::messages::RichTextSpan;
use web_sys::KeyboardEvent;

use crate::{
    components::{
        input::menu::{enter_selection, MenuType},
        normalize_tokens, RichTextExt,
    },
    tauri_functions::MemberShip,
};

pub mod menu;

#[derive(Clone, Copy)]
pub struct InputState {
    pub tokens: RwSignal<Vec<RichTextSpan>>,
    pub caret_position: RwSignal<(usize, usize)>,
    pub menu_type: RwSignal<MenuType>,
    pub selected_index: RwSignal<usize>,
    pub matches: RwSignal<Vec<MemberShip>>,
}

pub fn handle_keydown(ev: KeyboardEvent, input: InputState, caret_ref: NodeRef<Span>) -> () {
    ev.prevent_default();

    let key = ev.key();
    let mut tokens = input.tokens.get_untracked();
    let menu_type = input.menu_type;
    let selected_index = input.selected_index;
    let caret_position = input.caret_position;
    let matches = input.matches;

    let new_tokens = match key.as_str() {
        string if string.len() == 1 => {
            let Some(char) = string.chars().next() else {
                warn!("No character found in key: {key}");
                return;
            };

            if ev.ctrl_key() || ev.alt_key() || ev.meta_key() {
                return;
            }

            if menu_type.get_untracked() != MenuType::None {
                menu_type.update(|menu| match menu {
                    MenuType::Mentions { ref mut filter, .. }
                    | MenuType::Commands { ref mut filter, .. } => {
                        filter.push(char);
                    }
                    MenuType::None => (),
                });
            }

            match char {
                '@' => {
                    menu_type.set(MenuType::Mentions {
                        filter: "".to_string(),
                    });
                    input.selected_index.set(0);
                }
                '/' => {
                    menu_type.set(MenuType::Commands {
                        filter: "".to_string(),
                    });
                    input.selected_index.set(0);
                }
                ' ' => menu_type.set(MenuType::None),
                _ => (),
            };

            Some(input_char(char, input))
        }
        "Backspace" if ev.ctrl_key() => Some(input_ctrl_backspace(input)),
        "Delete" if ev.ctrl_key() => Some(input_ctrl_delete(input)),
        "Backspace" => Some(input_backspace(input)),
        "Delete" => Some(input_delete(input)),
        "ArrowRight" => {
            input_arrow_right(input.caret_position, &tokens);
            None
        }
        "ArrowLeft" => {
            input_arrow_left(input.caret_position, &tokens);
            None
        }
        "ArrowDown" => {
            if menu_type.get_untracked() != MenuType::None {
                selected_index.update(|i| {
                    let max = matches.get().len();

                    if *i >= max - 1 {
                        *i = 0;
                    } else {
                        *i += 1;
                    };
                });
            } else if let Some(caret_el) = caret_ref.get() {
                let rect = caret_el.get_bounding_client_rect();
                let x = rect.x() as f32;
                let target_y = (rect.y() + 20.0) as f32;

                if let Some((t_idx, c_idx)) = get_carat_index(x, target_y) {
                    caret_position.set((t_idx, c_idx));
                }
            };
            None
        }
        "ArrowUp" => {
            if menu_type.get_untracked() != MenuType::None {
                selected_index.update(|i| {
                    let max = matches.get().len();

                    if *i == 0 {
                        *i = max - 1;
                    } else {
                        *i -= 1;
                    };
                });
            } else {
                if let Some(caret_el) = caret_ref.get() {
                    let rect = caret_el.get_bounding_client_rect();
                    let x = rect.x() as f32;
                    let target_y = (rect.y() - 20.0) as f32;

                    if let Some((t_idx, c_idx)) = get_carat_index(x, target_y) {
                        caret_position.set((t_idx, c_idx));
                    }
                }
            };
            None
        }
        "Escape" if menu_type.get_untracked() != MenuType::None => {
            menu_type.set(MenuType::None);
            None
        }
        "Enter" if ev.shift_key() => Some(input_newline(input)),

        "Enter" if menu_type.get_untracked() != MenuType::None => {
            enter_selection(input);
            Some(input.tokens.get_untracked())
        }
        "Enter" => {
            // TODO: Actually send the message
            info!("Sending message with tokens: {:?}", tokens);
            Some(Vec::new())
        }
        "Shift" | "Control" | "Alt" | "AltGraph" => None,
        _ => {
            warn!("Unhandled key: {:?}", key);
            None
        }
    };

    if let Some(new_tokens) = new_tokens {
        tokens = new_tokens;
    }

    let (new_tokens, new_pos) = normalize_tokens(tokens, caret_position.get_untracked());
    input.tokens.set(new_tokens);
    caret_position.set(new_pos);
}

pub fn input_backspace(input: InputState) -> Vec<RichTextSpan> {
    let (t_idx, c_idx) = input.caret_position.get();
    let mut tokens = input.tokens.get_untracked();
    let set_caret_position = input.caret_position;

    if tokens.is_empty() || (t_idx == 0 && c_idx == 0) {
        return tokens;
    }

    if c_idx > 0 {
        if let Some(token) = tokens.get_mut(t_idx) {
            match token {
                RichTextSpan::Plain(text) => {
                    text.remove(c_idx - 1);
                    set_caret_position.update(|(_, c)| *c -= 1);
                    return tokens;
                }
                RichTextSpan::Link { url, .. } => {
                    url.remove(c_idx - 1);
                    set_caret_position.update(|(_, c)| *c -= 1);
                    return tokens;
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
                    return tokens;
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

    tokens
}

pub fn input_delete(input: InputState) -> Vec<RichTextSpan> {
    let (t_idx, c_idx) = input.caret_position.get();
    let mut tokens = input.tokens.get_untracked();

    if tokens.is_empty() || (t_idx >= tokens.len() && c_idx == 0) {
        return tokens;
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
                    return tokens;
                }
                RichTextSpan::Link { url, .. } => {
                    if c_idx < url.len() {
                        url.remove(c_idx);
                    } else if t_idx < tokens.len() - 1 {
                        tokens.remove(t_idx + 1);
                    }
                    return tokens;
                }
                RichTextSpan::UserMention { .. } | RichTextSpan::RoomMention => {
                    if c_idx == 0 {
                        tokens.remove(t_idx);
                        return tokens;
                    }
                }
                _ => {}
            }
        }
    }

    if t_idx < tokens.len() - 1 {
        tokens.remove(t_idx + 1);
    };

    tokens
}

fn word_range_backward(text: &str, c_idx: usize) -> Option<(usize, usize)> {
    let safe_c_idx = c_idx.min(text.len());
    if safe_c_idx == 0 {
        return None;
    }

    let mut iter = text[..safe_c_idx].char_indices().rev();

    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    let mut first_char: Option<char> = None;

    while let Some((i, ch)) = iter.next() {
        if ch.is_whitespace() {
            continue;
        }

        start = Some(i);
        end = Some(i + ch.len_utf8());
        first_char = Some(ch);
        break;
    }

    if start.is_none() {
        if safe_c_idx > 0 {
            return Some((0, safe_c_idx));
        }
        return None;
    }

    let (mut start, end, first_char) = match (start, end, first_char) {
        (Some(start), Some(end), Some(first_char)) => (start, end, first_char),
        _ => return None,
    };

    if first_char.is_alphanumeric() {
        for (i, ch) in iter {
            if ch.is_alphanumeric() {
                start = i;
            } else {
                break;
            }
        }
    }

    Some((start, end))
}

fn word_range_forward(text: &str, c_idx: usize) -> Option<(usize, usize)> {
    let safe_c_idx = c_idx.min(text.len());
    if safe_c_idx >= text.len() {
        return None;
    }

    let mut iter = text[safe_c_idx..].char_indices();

    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    let mut first_char: Option<char> = None;

    while let Some((i, ch)) = iter.next() {
        if ch.is_whitespace() {
            continue;
        }

        let abs_i = safe_c_idx + i;
        start = Some(abs_i);
        end = Some(abs_i + ch.len_utf8());
        first_char = Some(ch);
        break;
    }

    if start.is_none() {
        if safe_c_idx < text.len() {
            return Some((safe_c_idx, text.len()));
        }
        return None;
    }

    let (start, mut end, first_char) = match (start, end, first_char) {
        (Some(start), Some(end), Some(first_char)) => (start, end, first_char),
        _ => return None,
    };

    if first_char.is_alphanumeric() {
        for (i, ch) in iter {
            if ch.is_alphanumeric() {
                end = safe_c_idx + i + ch.len_utf8();
            } else {
                break;
            }
        }
    }

    Some((start, end))
}

pub fn input_ctrl_backspace(input: InputState) -> Vec<RichTextSpan> {
    let (t_idx, c_idx) = input.caret_position.get();
    let mut tokens = input.tokens.get_untracked();
    let set_caret_position = input.caret_position;

    if let Some(token) = tokens.get_mut(t_idx) {
        match token {
            RichTextSpan::Plain(text) => {
                if let Some((start, end)) = word_range_backward(text, c_idx) {
                    text.replace_range(start..end, "");
                    set_caret_position.set((t_idx, start));
                }
            }
            RichTextSpan::Link { url, .. } => {
                if let Some((start, end)) = word_range_backward(url, c_idx) {
                    url.replace_range(start..end, "");
                    set_caret_position.set((t_idx, start));
                }
            }
            RichTextSpan::UserMention { .. }
            | RichTextSpan::RoomMention
            | RichTextSpan::Newline => {
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
            }
            _ => {}
        }
    }

    tokens
}

pub fn input_ctrl_delete(input: InputState) -> Vec<RichTextSpan> {
    let (t_idx, c_idx) = input.caret_position.get();
    let mut tokens = input.tokens.get_untracked();
    let set_caret_position = input.caret_position;

    if let Some(token) = tokens.get_mut(t_idx) {
        match token {
            RichTextSpan::Plain(text) => {
                if let Some((start, end)) = word_range_forward(text, c_idx) {
                    text.replace_range(start..end, "");
                    let safe_c_idx = c_idx.min(text.len());
                    set_caret_position.set((t_idx, safe_c_idx));
                }
            }
            RichTextSpan::Link { url, .. } => {
                if let Some((start, end)) = word_range_forward(url, c_idx) {
                    url.replace_range(start..end, "");
                    let safe_c_idx = c_idx.min(url.len());
                    set_caret_position.set((t_idx, safe_c_idx));
                }
            }
            RichTextSpan::UserMention { .. }
            | RichTextSpan::RoomMention
            | RichTextSpan::Newline => {
                tokens.remove(t_idx);
                let safe_t_idx = t_idx.min(tokens.len());
                set_caret_position.set((safe_t_idx, 0));
            }
            _ => {}
        }
    }

    tokens
}

pub fn input_char(c: char, intput: InputState) -> Vec<RichTextSpan> {
    let mut tokens = intput.tokens.get_untracked();
    let (t_idx, c_idx) = intput.caret_position.get_untracked();

    let set_caret_position = intput.caret_position;

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
    };

    tokens
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
    set_caret_position: RwSignal<(usize, usize)>,
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
    set_caret_position: RwSignal<(usize, usize)>,
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

pub fn input_newline(input: InputState) -> Vec<RichTextSpan> {
    let (t_idx, c_idx) = input.caret_position.get_untracked();
    let mut tokens = input.tokens.get_untracked();

    let set_caret_position = input.caret_position;

    if tokens.is_empty() || t_idx >= tokens.len() {
        tokens.push(RichTextSpan::Newline);
        tokens.push(RichTextSpan::Plain(String::new()));
        set_caret_position.set((tokens.len() - 1, 0));
        return tokens;
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
    };

    tokens
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
