use leptos::prelude::*;
use shared::messages::RichTextSpan;

pub mod menu;

pub fn input_backspace(
    caret_position: (usize, usize),
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &mut Vec<RichTextSpan>,
) -> () {
    let (t_idx, c_idx) = caret_position;

    if t_idx == 0 && c_idx == 0 {
        return;
    }

    if let Some(token) = tokens.get_mut(t_idx) {
        match token {
            RichTextSpan::Plain(ref mut text) => {
                if !text.is_empty() {
                    text.remove(caret_position.1 - 1);

                    let token_idx = tokens.len() - 1;
                    set_caret_position.update(|(t_idx, c_idx)| {
                        *t_idx = token_idx;
                        *c_idx = caret_position.1 - 1;
                    });
                } else {
                    tokens.pop();
                    set_caret_position.update(|(t_idx, c_idx)| {
                        if *t_idx > 0 {
                            *t_idx -= 1;
                            if let Some(RichTextSpan::Plain(prev_text)) = tokens.get(*t_idx) {
                                *c_idx = prev_text.len();
                            } else {
                                *c_idx = 0;
                            }
                        } else {
                            *c_idx = 0;
                        }
                    });
                }
            }
            RichTextSpan::Link { ref mut url, .. } => {
                if !url.is_empty() {
                    url.remove(caret_position.1 - 1);

                    let token_idx = tokens.len() - 1;
                    set_caret_position.update(|(t_idx, c_idx)| {
                        *t_idx = token_idx;
                        *c_idx = caret_position.1 - 1;
                    });
                } else {
                    tokens.pop();
                    set_caret_position.update(|(t_idx, c_idx)| {
                        if *t_idx > 0 {
                            *t_idx -= 1;
                            if let Some(RichTextSpan::Plain(prev_text)) = tokens.get(*t_idx) {
                                *c_idx = prev_text.len();
                            } else {
                                *c_idx = 0;
                            }
                        } else {
                            *c_idx = 0;
                        }
                    });
                }
            }
            _ => {
                tokens.pop();
                set_caret_position.update(|(t_idx, c_idx)| {
                    if *t_idx > 0 {
                        *t_idx -= 1;
                        if let Some(RichTextSpan::Plain(prev_text)) = tokens.get(*t_idx) {
                            *c_idx = prev_text.len();
                        } else {
                            *c_idx = 0;
                        }
                    } else {
                        *c_idx = 0;
                    }
                });
            }
        }
    }
}

pub fn input_char(
    c: char,
    caret_position: (usize, usize),
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &mut Vec<RichTextSpan>,
) -> () {
    if let Some(token) = tokens.get_mut(caret_position.0) {
        match token {
            RichTextSpan::Plain(ref mut text) => {
                text.insert(caret_position.1, c);

                let token_idx = tokens.len() - 1;

                set_caret_position.update(|(t_idx, c_idx)| {
                    *t_idx = token_idx;
                    *c_idx = caret_position.1 + 1;
                });
            }
            _ => {
                tokens.push(RichTextSpan::Plain(c.to_string()));
                set_caret_position.update(|(t_idx, c_idx)| {
                    *t_idx = tokens.len() - 1;
                    *c_idx = 1;
                });
            }
        }
    } else {
        tokens.push(RichTextSpan::Plain(c.to_string()));
        set_caret_position.update(|(t_idx, c_idx)| {
            *t_idx = tokens.len() - 1;
            *c_idx = 1;
        });
    }
}

pub fn input_arrow_left(
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &Vec<RichTextSpan>,
) -> () {
    set_caret_position.update(|(t_idx, c_idx)| {
        if *t_idx == 0 && *c_idx == 0 {
            return;
        }

        if *c_idx > 0 {
            *c_idx -= 1;
        } else if *t_idx > 0 {
            *t_idx -= 1;
            if let Some(RichTextSpan::Plain(text)) = tokens.get(*t_idx) {
                *c_idx = text.len();
            } else {
                *c_idx = 0;
            }
        }
    });
}

pub fn input_arrow_right(
    set_caret_position: WriteSignal<(usize, usize)>,
    tokens: &Vec<RichTextSpan>,
) -> () {
    set_caret_position.update(|(t_idx, c_idx)| {
        if let Some(token) = tokens.get(*t_idx) {
            let token_len = match token {
                RichTextSpan::Plain(text) => text.len(),
                RichTextSpan::Link { url, .. } => url.len(),
                _ => 0,
            };

            if *c_idx < token_len {
                *c_idx += 1;
            } else if *t_idx < tokens.len() - 1 {
                *t_idx += 1;
                *c_idx = 0;
            }
        }
    });
}
