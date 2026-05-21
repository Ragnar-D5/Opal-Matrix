use leptos::task::spawn_local;
use leptos::{html::Div, prelude::*, tachys::dom::document};
use log::warn;
use regex::Regex;
use shared::commands::Command;
use shared::user_profile::UserProfile;
use wasm_bindgen::JsCast;
use web_sys::{HtmlDivElement, HtmlElement, KeyboardEvent};
use web_sys::{Node, window};

use crate::components::input::menu::MenuType;
use crate::components::input::menu::{SelectedItem, commit_selection};
use crate::state::{AppState, MemberStore};
use crate::tauri_functions::commit_message;

pub(crate) mod menu;

fn check_if_empty(el: &HtmlElement, is_empty: RwSignal<bool>) -> bool {
    let raw_html = el.inner_html().to_lowercase();

    let clean_html = raw_html
        .replace("<!---->", "")
        .replace("\n", "")
        .replace("\r", "")
        .trim()
        .to_string();

    let text = el.text_content().unwrap_or_default().trim().to_string();

    let empty = text.is_empty()
        && (clean_html.is_empty()
            || clean_html == "<br>"
            || clean_html == "<br><br>"
            || clean_html == "<br type=\"_moz\">");

    if empty && !raw_html.is_empty() && raw_html != "<br>" {
        el.set_inner_html("");
    }

    is_empty.set(empty);

    empty
}

pub fn handle_input(input_ref: NodeRef<Div>, is_empty: RwSignal<bool>, state: AppState) {
    let Some(el) = input_ref.get() else { return };
    let doc = document();

    let caret_pos = get_caret_position(&el);

    cleanup_link_spaces(&el);

    let link_regex = Regex::new(r"(https?://[^\s]+)").expect("Failed to create regex");

    let mut text_nodes = Vec::new();
    let child_nodes = el.child_nodes();
    for i in 0..child_nodes.length() {
        let node = child_nodes.item(i).unwrap();
        if node.node_type() == Node::TEXT_NODE {
            text_nodes.push(node);
        }
    }

    for text_node in text_nodes {
        let text_content = text_node.text_content().unwrap_or_default();
        if !link_regex.is_match(&text_content) {
            continue;
        }

        let fragment = doc.create_document_fragment();
        let mut last_end = 0;

        for mat in link_regex.find_iter(&text_content) {
            let start = mat.start();
            let end = mat.end();
            let url = mat.as_str();

            if start > last_end {
                let pre_text = &text_content[last_end..start];
                let pre_node = doc.create_text_node(pre_text);
                fragment.append_child(&pre_node).unwrap();
            }

            let span = doc.create_element("span").unwrap();
            span.set_class_name("text-blue-500 underline link cursor-pointer");
            span.set_attribute("data-url", url).unwrap();

            span.set_text_content(Some(url));
            fragment.append_child(&span).unwrap();

            last_end = end;
        }

        if last_end < text_content.len() {
            let post_text = &text_content[last_end..];
            let post_node = doc.create_text_node(post_text);
            fragment.append_child(&post_node).unwrap();
        }

        let parent = text_node.parent_node().unwrap();
        parent.replace_child(&fragment, &text_node).unwrap();
    }

    restore_caret_position(&el, caret_pos);

    let empty = check_if_empty(&el, is_empty);

    if let Some(room_id) = state.active_room_id.get_untracked() {
        state.drafts.update(|drafts| {
            if empty {
                drafts.remove(&room_id);
            } else {
                drafts.insert(room_id, el.inner_html());
            }
        });
    }
}

fn cleanup_link_spaces(editor: &HtmlElement) {
    let doc = document();
    let links = editor.query_selector_all(".link").unwrap();

    for i in 0..links.length() {
        let el: HtmlElement = links.item(i).unwrap().dyn_into().unwrap();
        let text = el.text_content().unwrap_or_default();
        let data_url = el.get_attribute("data-url").unwrap_or_default();

        if text != data_url {
            let parent = el.parent_node().unwrap();
            let text_node = doc.create_text_node(&text);
            parent.replace_child(&text_node, &el).unwrap();
            continue;
        }

        if text.ends_with(' ') || text.ends_with('\u{00A0}') {
            let clean_text = text.trim_end();
            el.set_inner_text(clean_text);
            el.set_attribute("data-url", clean_text).unwrap();

            let space_node = doc.create_text_node("\u{00A0}");
            let parent = el.parent_node().unwrap();
            if let Some(next_sibling) = el.next_sibling() {
                parent
                    .insert_before(&space_node, Some(&next_sibling))
                    .unwrap();
            } else {
                parent.append_child(&space_node).unwrap();
            }
        }
    }
}

pub fn get_active_filter(
    el: &HtmlDivElement,
    caret_pos: u32,
    starting_character: char,
) -> Option<String> {
    let (node, local_offset) = get_node_and_offset(el, caret_pos)?;

    if let Some(parent) = node.parent_element()
        && parent.tag_name().to_uppercase() == "SPAN"
    {
        return None;
    }

    let text = node.text_content().unwrap_or_default();
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let offset = local_offset as usize;

    if offset > utf16.len() {
        return None;
    }

    let mut start_idx = offset;
    while start_idx > 0 {
        let prev_char = utf16[start_idx - 1];
        // 32 = Space, 160 = Non-breaking space (&nbsp;), 10 = Newline (\n)
        // 8203 = Zero-width space (\u{200B})
        if prev_char == 32 || prev_char == 160 || prev_char == 10 || prev_char == 8203 {
            break;
        }
        start_idx -= 1;
    }

    let target_utf16 = starting_character as u16;

    if start_idx == utf16.len() || utf16[start_idx] != target_utf16 {
        return None;
    }

    let mut end_idx = offset;
    while end_idx < utf16.len() {
        let next_char = utf16[end_idx];
        if next_char == 32 || next_char == 160 || next_char == 10 || next_char == 8203 {
            break;
        }
        end_idx += 1;
    }

    let filter_utf16 = &utf16[(start_idx + 1)..end_idx];

    Some(String::from_utf16(filter_utf16).unwrap_or_default())
}

pub fn handle_keydown(
    ev: KeyboardEvent,
    input_ref: NodeRef<Div>,
    menu: RwSignal<MenuType>,
    selected_index: RwSignal<usize>,
    mention_matches: RwSignal<Vec<UserProfile>>,
    command_matches: RwSignal<Vec<Command>>,
    state: AppState,
    store: MemberStore,
    is_empty: RwSignal<bool>,
) {
    let Some(el) = input_ref.get() else { return };

    let key = ev.key();
    let current_menu = menu.get_untracked();

    match key.as_str() {
        "Enter" => {
            if ev.shift_key() {
                return;
            }
            ev.prevent_default();

            if current_menu != MenuType::None {
                let mentions = mention_matches.get_untracked();
                let commands = command_matches.get_untracked();

                let selected = match current_menu {
                    MenuType::UserAutocomplete { .. } => {
                        let Some(selected) = mentions.get(selected_index.get()) else {
                            return;
                        };

                        SelectedItem::from(selected.clone())
                    }
                    MenuType::CommandAutocomplete { .. } => {
                        let Some(selected) = commands.get(selected_index.get()) else {
                            return;
                        };

                        SelectedItem::from(selected.clone())
                    }
                    MenuType::None => return,
                };

                commit_selection(&el, selected, state, store);
                menu.set(MenuType::None);
            } else {
                let message = el.inner_html();
                let Some(room_id) = state.active_room_id.get_untracked() else {
                    warn!("No active room to send message to");
                    return;
                };

                el.set_inner_html("<br>");
                if let Some(room_id) = state.active_room_id.get_untracked() {
                    state.drafts.update(|drafts| {
                        drafts.remove(&room_id);
                    });
                }

                spawn_local(async move {
                    if let Err(e) = commit_message(message, room_id).await {
                        warn!("Failed to commit message: {e}");
                    };
                });
            }
        }
        "ArrowUp" | "ArrowDown" if current_menu != MenuType::None => {
            ev.prevent_default();

            let len = match current_menu {
                MenuType::UserAutocomplete { .. } => mention_matches.get_untracked().len(),
                MenuType::CommandAutocomplete { .. } => command_matches.get_untracked().len(),
                MenuType::None => return,
            };

            match (current_menu, key.as_str()) {
                (MenuType::UserAutocomplete { .. }, "ArrowUp") => {
                    selected_index.update(|idx| {
                        if *idx == 0 {
                            *idx = len - 1;
                        } else {
                            *idx -= 1;
                        }
                    });
                }
                (MenuType::UserAutocomplete { .. }, "ArrowDown") => {
                    selected_index.update(|idx| {
                        *idx = (*idx + 1) % len;
                    });
                }
                (MenuType::CommandAutocomplete { .. }, "ArrowUp") => {
                    selected_index.update(|idx| {
                        if *idx == 0 {
                            *idx = len - 1;
                        } else {
                            *idx -= 1;
                        }
                    });
                }
                (MenuType::CommandAutocomplete { .. }, "ArrowDown") => {
                    selected_index.update(|idx| {
                        *idx = (*idx + 1) % len;
                    });
                }
                _ => return,
            };
        }
        "Escape" if current_menu != MenuType::None => {
            menu.set(MenuType::None);
        }
        _ => {}
    }

    let _ = check_if_empty(&el, is_empty);
}

pub fn get_caret_position(el: &HtmlElement) -> u32 {
    let Some(window) = window() else {
        warn!("Could not get window object");
        return 0;
    };
    let Ok(Some(selection)) = window.get_selection() else {
        warn!("Could not get selection object");
        return 0;
    };
    if selection.range_count() == 0 {
        return 0;
    }

    let Ok(range) = selection.get_range_at(0) else {
        warn!("Could not get range from selection");
        return 0;
    };
    let pre_caret_range = range.clone_range();

    pre_caret_range
        .select_node_contents(el)
        .unwrap_or_else(|_| {});
    pre_caret_range
        .set_end(&range.end_container().unwrap(), range.end_offset().unwrap())
        .unwrap();

    let fragment = pre_caret_range.clone_contents().unwrap();

    let text = fragment.text_content().unwrap_or_default();

    text.encode_utf16().count() as u32
}

fn restore_caret_position(el: &HtmlElement, absolute_pos: u32) {
    let window = window().unwrap();
    let selection = window.get_selection().unwrap().unwrap();
    let document = document();

    let tree_walker = document
        .create_tree_walker_with_what_to_show(el, 4)
        .unwrap();
    let mut current_pos = 0;
    let mut target_node: Option<Node> = None;
    let mut target_offset = 0;

    while let Ok(Some(node)) = tree_walker.next_node() {
        let text_content = node.text_content().unwrap_or_default();
        let node_len = text_content.encode_utf16().count() as u32;
        if current_pos + node_len >= absolute_pos {
            target_node = Some(node);
            target_offset = absolute_pos - current_pos;
            break;
        }
        current_pos += node_len;
    }

    if let Some(node) = target_node {
        let range = document.create_range().unwrap();
        range.set_start(&node, target_offset).unwrap();
        range.collapse_with_to_start(true);
        selection.remove_all_ranges().unwrap();
        selection.add_range(&range).unwrap();
    }
}

fn get_node_and_offset(el: &HtmlElement, absolute_pos: u32) -> Option<(Node, u32)> {
    let document = document();
    let tree_walker = document
        .create_tree_walker_with_what_to_show(el, 4)
        .unwrap();
    let mut current_pos = 0;

    while let Ok(Some(node)) = tree_walker.next_node() {
        let text_content = node.text_content().unwrap_or_default();
        let node_len = text_content.encode_utf16().count() as u32;

        if current_pos + node_len >= absolute_pos {
            return Some((node, absolute_pos - current_pos));
        }
        current_pos += node_len;
    }
    None
}
