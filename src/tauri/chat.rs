use crate::state::{AppState, MemberProfileHandle, MemberStore, RoomHeader};
use std::collections::{HashMap, HashSet};

use crate::app::call_tauri;
use crate::components::presence::PresenceBadge;
use crate::components::FloatingTile;
use crate::hooks::use_tauri_event;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use leptos::html::Div;
use leptos::task::spawn_local;
use leptos::{leptos_dom::logging::console_error, prelude::*};
use leptos_use::{use_intersection_observer, UseIntersectionObserverReturn};
use serde::Serialize;
use shared::messages::{
    MembershipAction, MessageContent, MessageKind, Reaction, SystemMessage, UiMessage, UserMessage,
};
use shared::sidebar::{RoomKind, RoomNode, SidebarState};

use crate::components::user_profile::{UserProfileExt, UserProfileMaybeExt};

#[derive(PartialEq, Clone)]
struct TimelineMessageGroup {
    contents: Vec<UserMessage>,
}

#[derive(PartialEq, Clone)]
enum TimelineItemKind {
    MessageGroup(TimelineMessageGroup),
    DateSeparator,
    SystemMessage(SystemMessage),
    NewMessageIndicator,
}

#[derive(PartialEq, Clone)]
struct TimelineItem {
    date: DateTime<Local>,
    sender: String,
    id: String,

    kind: TimelineItemKind,
}

fn format_date(date: DateTime<Local>) -> String {
    match (date.date_naive() - Local::now().date_naive()).num_days() {
        0 => date.format("Today, %H:%M").to_string(),
        -1 => date.format("Yesterday, %H:%M").to_string(),
        _ => date.format("%d/%m/%Y, %H:%M").to_string(),
    }
}

impl TimelineItem {
    fn render(&self) -> impl IntoView {
        let state: AppState = expect_context();
        let store: MemberStore = expect_context();

        let Some(room_id) = state.active_room_id.get() else {
            return view! {}.into_any();
        };

        let profile_sig = store.get_profile(&room_id, &self.sender);

        let name_sig = profile_sig.clone();
        let sender_id = self.sender.clone();

        match &self.kind {
            TimelineItemKind::MessageGroup(group) => {
                let first_msg = group.contents.first();
                let reply_data = first_msg.and_then(|m| m.replies_to.clone());

                view! {
                    <div class="flex flex-col gap-1 p-3 rounded-md">
                        {move || {
                            if let Some(reply) = &reply_data {
                                let Some(reply_sender) = reply.sender_id.clone() else {
                                    return view! {}.into_any();
                                };

                                let reply_text = reply.text.clone().unwrap_or_default();

                                let reply_profile_sig = store.get_profile(&room_id, &reply_sender);
                                let reply_profile_sig_icon = reply_profile_sig.clone();
                                let reply_profile_sig_name = reply_profile_sig.clone();

                                view! {
                                    <div class="flex items-center gap-1 ml-[52px] mb-1 cursor-pointer text-xs relative group/reply">
                                        <div class="absolute -left-[32px] top-[calc(50%-1px)] w-[28px] h-4.5 border-l-2 border-t-2 border-white/20 rounded-tl-md"></div>

                                        <div class="shrink-0">
                                            {move || reply_profile_sig_icon.get().render_icon(16)}
                                        </div>

                                        <span class="font-semibold text-bright hover:underline">
                                            {move || reply_profile_sig_name.get().render_name(12)}
                                        </span>

                                        <span class="truncate text-bright line-clamp-1">
                                            {reply_text}
                                        </span>
                                    </div>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }
                        }}
                        <div class="flex flex-col w-full">
                            {group.contents.iter().enumerate().map(|(idx, msg)| {
                                let is_first = idx == 0;

                                let mut reaction_counts: HashMap<String, usize> = HashMap::new();
                                for r in &msg.reactions {
                                    *reaction_counts.entry(r.reaction.clone()).or_insert(0) += 1;
                                }

                                let content = match &msg.content {
                                    MessageContent::Text { text, is_edited } => view! {
                                        <div class="text-normal leading-relaxed break-words cursor-text">
                                            {text.clone()}
                                            {if *is_edited {
                                                view! { <span class="text-xs text-muted ml-2 italic">"(edited)"</span> }.into_any()
                                            } else {
                                                view! {}.into_any()
                                            }}
                                        </div>
                                    }.into_any(),
                                    MessageContent::Image { url, name, encryption_info, .. } => {
                                        let final_url = if let Some(enc) = encryption_info {
                                            let encoded_key = urlencoding::encode(&enc.key);
                                            let encoded_iv = urlencoding::encode(&enc.iv);

                                            format!("{}?key={}&iv={}", url, encoded_key, encoded_iv)
                                        } else {
                                            url.clone()
                                        };
                                        view! {
                                            <div class="mt-1">
                                                <img src=final_url alt=name.clone() class="max-w-sm rounded-md border border-[var(--tile-border-color)]" />
                                            </div>
                                        }.into_any()
                                    },
                                    MessageContent::File { url, filename, size } => view! {
                                        <div class="flex items-center gap-2 mt-1 p-2 rounded-md bg-white/5 border border-[var(--tile-border-color)] inline-flex">
                                            <span class="text-xl">"📄"</span>
                                            <a href=url.clone() target="_blank" class="text-blue-400 hover:underline truncate max-w-xs">
                                                {filename.clone()}
                                            </a>
                                            <span class="text-xs text-muted">
                                                {format!("{:.1} KB", *size as f64 / 1024.0)}
                                            </span>
                                        </div>
                                    }.into_any(),
                                    MessageContent::Encrypted => view! {
                                        <div class="text-red-300 bold leading-relaxed break-words text-muted">
                                            "Encrypted message"
                                        </div>
                                    }.into_any(),
                                    MessageContent::Deleted => view! {
                                        <div class="text-muted italic leading-relaxed break-words">
                                            "This message was deleted"
                                        </div>
                                    }.into_any(),
                                };

                                view! {
                                    <div class="group/msg relative flex gap-[var(--gap)] hover:bg-black/20 px-3 py-[2px] -mx-3 rounded-md">

                                        <div class="shrink-0 mr-2 w-[40px]">
                                            {if is_first {
                                                let profile_sig = profile_sig.clone();
                                                view! {
                                                    {move || {
                                                        let profile = profile_sig.get();
                                                        profile.render_icon(40)
                                                    }}
                                                }.into_any()
                                            } else {
                                                view! {}.into_any()
                                            }}
                                        </div>

                                        <div class="flex flex-col min-w-0 flex-1">

                                            {if is_first {
                                                let name_sig = name_sig.clone();
                                                view! {
                                                    <div class="flex items-baseline gap-2">
                                                        <span class="text-bright truncate cursor-pointer">
                                                            {move || name_sig.get().render_name(16)}
                                                        </span>
                                                        <span class="text-muted text-xs">
                                                            {format_date(self.date)}
                                                        </span>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {}.into_any()
                                            }}

                                            // Message Content & Reactions
                                            <div>
                                                {content}

                                                {if !reaction_counts.is_empty() {
                                                    view! {
                                                        <div class="flex flex-wrap gap-1 mt-1 mb-2">
                                                            {reaction_counts.into_iter().map(|(emoji, count)| {
                                                                view! {
                                                                    <div class="flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-white/10 border border-white/5 hover:bg-white/20 cursor-pointer">
                                                                        <span class="text-sm leading-none">{emoji}</span>
                                                                        <span class="text-[10px] font-medium text-muted">{count}</span>
                                                                    </div>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {}.into_any()
                                                }}
                                            </div>
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                }
                .into_any()
            }
            TimelineItemKind::DateSeparator => {
                let is_today = self.date.date_naive() == Local::now().date_naive();
                let is_yesterday = self.date.date_naive()
                    == (Local::now().date_naive() - chrono::Duration::days(1));

                let label = if is_today {
                    "Today".to_string()
                } else if is_yesterday {
                    "Yesterday".to_string()
                } else {
                    self.date.format("%d %B %Y").to_string()
                };

                view! {
                    <div class="flex items-center gap-2 my-4">
                        <div class="flex-1 border-t-1 border-[var(--muted-text-color)]"></div>
                        <span class="text-muted text-sm">
                            {label}
                        </span>
                        <div class="flex-1 border-t-1 border-[var(--muted-text-color)]"></div>
                    </div>
                }
            }
            .into_any(),
            // Discord style new message indicator
            TimelineItemKind::NewMessageIndicator => view! {
                <div class="flex items-center w-full pr-4">
                    <div class="flex-1 border-2 border-[#00ffff] rounded-full"></div>

                    <span class="relative flex items-center h-[20px] bg-[#00ffff] text-[var(--bg-color)] text-[10px] font-bold px-2 rounded-r-[3px] ml-1 uppercase tracking-wider select-none">
                        // The left-pointing arrow (<) built using CSS borders
                        <div class="absolute -left-[6px] top-0 w-0 h-0 border-y-[10px] border-y-transparent border-r-[6px] border-r-[#00ffff]"></div>
                        "New"
                    </span>
                </div>
            }
            .into_any(),
            TimelineItemKind::SystemMessage(sys_msg) => {
                let display_name = profile_sig.get().unwrap_or_default().display_name.unwrap_or(sender_id);

                let text = match sys_msg {
                    SystemMessage::RoomCreation => format!("{} created the room", display_name),
                    SystemMessage::RoomNameChange { new_name } => {
                        format!("{} changed the room name to '{}'", display_name, new_name)
                    }
                    SystemMessage::TopicChange { new_topic } => {
                        format!("{} changed the topic to '{}'", display_name, new_topic)
                    }
                    SystemMessage::MembershipChange(action) => match action {
                        MembershipAction::Joined => format!("{} joined the room", display_name),
                        MembershipAction::Left => format!("{} left the room", display_name),
                        MembershipAction::Invited { .. } => {
                            format!("{} was invited to the room", display_name)
                        }
                        MembershipAction::Kicked { target_id, reason } => format!(
                            "{} kicked {}{}",
                            display_name,
                            target_id,
                            if let Some(r) = reason {
                                format!(": {}", r)
                            } else {
                                "".to_string()
                            }
                        ),
                        MembershipAction::Banned { target_id, reason } => format!(
                            "{} banned {}{}",
                            display_name,
                            target_id,
                            if let Some(r) = reason {
                                format!(": {}", r)
                            } else {
                                "".to_string()
                            }
                        ),
                    },
                    // _ => "Test".to_string()
                    SystemMessage::EncryptionEnabled { algorithm } => {
                        format!("{} enabled encryption ({})", display_name, algorithm)
                    }
                    SystemMessage::PowerlevelChange => {
                        format!("{} changed the power levels", display_name)
                    }
                    SystemMessage::JoinRuleChange { new_rule } => {
                        format!("{} changed the join rules to '{}'", display_name, new_rule)
                    }
                    SystemMessage::HistoryVisibilityChange { new_visibility } => format!(
                        "{} changed the history visibility to '{}'",
                        display_name, new_visibility
                    ),
                    SystemMessage::GuestAccessChange { new_access } => format!(
                        "{} changed the guest access to '{}'",
                        display_name, new_access
                    ),
                    SystemMessage::CallJoined { intent } => {
                        format!("{} joined a call ({})", display_name, intent)
                    }
                    SystemMessage::CallLeft => format!("{} left a call", display_name),

                    _ => format!("{} performed an action", display_name),
                };

                view! {
                    <div class="flex items-center justify-center my-2">
                        <span class="text-muted text-xxl">
                            {text}
                        </span>
                    </div>
                }
                .into_any()
            }
        }
    }
}

fn get_date_from_ts(ts: i64) -> DateTime<Local> {
    Local
        .timestamp_opt(ts, 0)
        .latest()
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.with_timezone(&Local))
}

fn room_has_notifications(state: &SidebarState, room_id: &str) -> bool {
    fn find_in_nodes(nodes: &[RoomNode], room_id: &str) -> Option<u32> {
        for node in nodes {
            if node.room_id == room_id {
                return Some(node.notification_count);
            }

            if let RoomKind::Space { children } = &node.kind {
                if let Some(count) = find_in_nodes(children, room_id) {
                    return Some(count);
                }
            }
        }

        None
    }

    find_in_nodes(&state.dms, room_id)
        .or_else(|| find_in_nodes(&state.orphaned_rooms, room_id))
        .or_else(|| find_in_nodes(&state.servers, room_id))
        .unwrap_or(0)
        > 0
}

#[derive(Serialize)]
struct FetchMessagesRequest {
    room_id: String,
    oldest_id: Option<String>,
}

#[derive(Serialize)]
struct ReceiptArgs {
    room_id: String,
}

#[component]
fn TimeLine() -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<UiMessage>::new());
    let (read_marker_id, set_read_marker_id) = signal::<Option<String>>(None);

    let state = expect_context::<AppState>();

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    // Listen to messages_update
    let messages_update_event: ReadSignal<Option<HashMap<String, Vec<UiMessage>>>> =
        use_tauri_event("messages_update");

    Effect::new(move |_| {
        if let Some(update) = messages_update_event.get() {
            let room_id = expect_context::<AppState>().active_room_id.get();
            if let Some(rid) = room_id {
                if let Some(new_msgs) = update.get(&rid) {
                    set_messages.update(|existing| {
                        let mut seen_ids: HashSet<String> =
                            existing.iter().map(|m| m.event_id.clone()).collect();

                        let unique_new: Vec<UiMessage> = new_msgs
                            .iter()
                            .filter(|m| seen_ids.insert(m.event_id.clone()))
                            .cloned()
                            .collect();

                        existing.extend(unique_new);
                    });
                }
            }
        }
    });

    let has_unread_notifications = Memo::new(move |_| {
        let Some(room_id) = state.active_room_id.get() else {
            return false;
        };

        let Some(sidebar_state) = sidebar_update_event.get() else {
            return false;
        };

        room_has_notifications(&sidebar_state, &room_id)
    });

    let flattened_items = Memo::new(move |_| {
        let mut msgs = messages.get();
        msgs.sort_by_key(|m| m.timestamp);

        let mut items: Vec<TimelineItem> = Vec::new();
        let mut last_day: Option<NaiveDate> = None;
        let marker_id = read_marker_id.get();
        let has_unread = has_unread_notifications.get();

        let mut edits = HashMap::new();
        let mut redactions = HashSet::new();
        let mut reactions_map = HashMap::new();

        let mut processed_messages = HashMap::new();

        let marker_ts = marker_id
            .as_ref()
            .and_then(|mid| msgs.iter().find(|m| m.event_id == *mid))
            .map(|m| m.timestamp);

        msgs = msgs
            .into_iter()
            .filter(|msg| {
                let result = match &msg.kind {
                    MessageKind::SystemMessage(SystemMessage::MessageEdited {
                        event_id,
                        new_text,
                    }) => {
                        edits.insert(event_id.clone(), new_text.clone());
                        false
                    }
                    MessageKind::SystemMessage(SystemMessage::MessageRedacted { event_id }) => {
                        redactions.insert(event_id.clone());
                        false
                    }
                    MessageKind::SystemMessage(SystemMessage::MessageReacted {
                        event_id,
                        ref reaction,
                    }) => {
                        reactions_map.insert(
                            event_id.clone(),
                            Reaction {
                                sender_id: msg.sender_id.clone(),
                                reaction: reaction.clone(),
                            },
                        );
                        false
                    }
                    _ => true,
                };

                processed_messages.insert(msg.event_id.clone(), msg.clone());
                result
            })
            .collect();

        let mut seen_marker_id = false;
        let mut inserted_marker = false;

        for msg in msgs.iter_mut() {
            if let MessageKind::SystemMessage(
                SystemMessage::MessageEdited { .. }
                | SystemMessage::MessageRedacted { .. }
                | SystemMessage::MessageReacted { .. },
            ) = &msg.kind
            {
                continue;
            }

            if redactions.contains(&msg.event_id) {
                msg.delete();
            }
            if let Some(new_text) = edits.get(&msg.event_id) {
                msg.edit(new_text.clone());
            }
            if let Some(reaction) = reactions_map.get(&msg.event_id) {
                msg.add_reaction(reaction.clone());
            }

            let current_date = get_date_from_ts(msg.timestamp);
            let current_day = current_date.date_naive();

            if Some(current_day) != last_day {
                items.push(TimelineItem {
                    date: current_date,
                    sender: String::new(),
                    id: format!("date-sep-{}", current_day),
                    kind: TimelineItemKind::DateSeparator,
                });
                last_day = Some(current_day);
            }

            if let Some(m_id) = &marker_id {
                if !inserted_marker && has_unread {
                    let is_after_marker =
                        seen_marker_id || marker_ts.is_some_and(|ts| msg.timestamp > ts);

                    if is_after_marker {
                        items.push(TimelineItem {
                            date: current_date,
                            sender: String::new(),
                            id: "new-message-indicator".to_string(),
                            kind: TimelineItemKind::NewMessageIndicator,
                        });
                        inserted_marker = true;
                    }

                    if msg.event_id == *m_id {
                        seen_marker_id = true;
                    }
                }
            }

            let maybe_item = match msg.kind.clone() {
                MessageKind::UserMessage(mut user_msg) => {
                    if let Some(replies_to) = &user_msg.replies_to {
                        if let Some(original_msg) = processed_messages.get(&replies_to.event_id) {
                            let original_sender = &original_msg.sender_id;
                            let text = match &original_msg.kind {
                                MessageKind::UserMessage(um) => match &um.content {
                                    MessageContent::Text { text, .. } => text.clone(),
                                    _ => "[non-text content]".to_string(),
                                },
                                MessageKind::SystemMessage(_) => "[system message]".to_string(),
                            };

                            user_msg.set_reply_sender(original_sender.clone());
                            user_msg.set_reply_text(text);
                        }
                    }

                    let mut grouped = false;
                    if let Some(last_item) = items.last_mut() {
                        if let TimelineItemKind::MessageGroup(ref mut group) = last_item.kind {
                            let same_sender = last_item.sender == msg.sender_id;
                            let same_minute = (current_date.timestamp() / 60)
                                == (last_item.date.timestamp() / 60);

                            let current_is_reply = user_msg.replies_to.is_some();
                            let last_is_reply = group
                                .contents
                                .last()
                                .map(|m| m.replies_to.is_some())
                                .unwrap_or(false);

                            if same_sender && same_minute && !current_is_reply && !last_is_reply {
                                group.contents.push(user_msg.clone());
                                last_item.id = format!("{}_{}", last_item.id, msg.event_id);
                                grouped = true;
                            }
                        }
                    }

                    if !grouped {
                        Some(TimelineItem {
                            date: current_date,
                            sender: msg.sender_id.clone(),
                            id: msg.event_id.clone(),
                            kind: TimelineItemKind::MessageGroup(TimelineMessageGroup {
                                contents: vec![user_msg.clone()],
                            }),
                        })
                    } else {
                        None
                    }
                }
                MessageKind::SystemMessage(sys_msg) => Some(TimelineItem {
                    date: current_date,
                    sender: msg.sender_id.clone(),
                    id: msg.event_id.clone(),
                    kind: TimelineItemKind::SystemMessage(sys_msg.clone()),
                }),
            };

            if let Some(item) = maybe_item {
                items.push(item);
            }
        }
        items.reverse();

        items
    });

    let (is_loading, set_is_loading) = signal(false);
    let (has_more, set_has_more) = signal(true);
    let (initial_loaded, set_initial_loaded) = signal(false);

    let fetch_more = move |_: ()| {
        if is_loading.get_untracked() {
            return;
        }
        if !has_more.get_untracked() {
            return;
        }

        let room_id = state.active_room_id.get_untracked();
        if room_id.is_none() {
            return;
        }

        set_is_loading.set(true);

        spawn_local(async move {
            let result = async {
                let rid = room_id?; // Safe return if None
                let oldest_id = messages
                    .get_untracked()
                    .iter()
                    .min_by_key(|m| m.timestamp)
                    .map(|m| m.event_id.clone());

                let request = FetchMessagesRequest {
                    room_id: rid,
                    oldest_id,
                };
                let args = serde_wasm_bindgen::to_value(&request).ok()?;

                let res = call_tauri("fetch_messages", args).await;

                match res {
                    Ok(js_val) => Some(js_val),
                    Err(e) => {
                        console_error(&format!("Error fetching messages: {:?}", e));
                        None
                    }
                }
            }
            .await;

            if let Some(js_val) = result {
                if let Ok((new_messages, has_more)) =
                    serde_wasm_bindgen::from_value::<(Vec<UiMessage>, bool)>(js_val)
                {
                    set_has_more.set(has_more);

                    set_messages.update(|existing| {
                        let mut seen_ids: HashSet<String> =
                            existing.iter().map(|m| m.event_id.clone()).collect();

                        let mut unique_new: Vec<UiMessage> = new_messages
                            .into_iter()
                            .filter(|m| seen_ids.insert(m.event_id.clone()))
                            .collect();

                        unique_new.append(existing);
                        *existing = unique_new;
                    });

                    set_initial_loaded.set(true);
                }
            }

            set_is_loading.set(false);
        });
    };

    let sentinel_ref = NodeRef::<Div>::new();

    let UseIntersectionObserverReturn { .. } =
        use_intersection_observer(sentinel_ref, move |entries, _| {
            if entries[0].is_intersecting()
                && initial_loaded.get()
                && !is_loading.get()
                && has_more.get()
            {
                fetch_more(());
            }
        });

    Effect::new(move |_| {
        let rid = state.active_room_id.get();

        if rid.is_some() {
            set_messages.set(Vec::new());
            set_has_more.set(true);
            set_initial_loaded.set(false);
            set_read_marker_id.set(None);

            set_timeout(
                move || {
                    fetch_more(());
                },
                std::time::Duration::from_millis(1),
            );
        }
    });

    Effect::new(move |_| {
        let rid = state.active_room_id.get();
        let has_unread = has_unread_notifications.get();

        if let Some(room_id) = rid {
            if !has_unread {
                set_read_marker_id.set(None);
                return;
            }

            spawn_local(async move {
                let args = match serde_wasm_bindgen::to_value(&ReceiptArgs {
                    room_id: room_id.clone(),
                }) {
                    Ok(a) => a,
                    Err(e) => {
                        console_error(&format!("Failed to serialize receipt args: {:?}", e));
                        return;
                    }
                };

                match call_tauri("get_receipt", args).await {
                    Ok(marker) => match serde_wasm_bindgen::from_value::<Option<String>>(marker) {
                        Ok(parsed_marker) => {
                            set_read_marker_id.set(parsed_marker);
                        }
                        Err(e) => console_error(&format!("Failed to parse receipt: {:?}", e)),
                    },
                    Err(e) => console_error(&format!("Tauri get_receipt call failed: {:?}", e)),
                }
            });
        } else {
            set_read_marker_id.set(None);
        }
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse p-2 overflow-anchor-auto">
            <For
                each=move || flattened_items.get()
                key=|item| item.id.clone()
                children=|item| item.render()
            />

            <Show
                when=move || !is_loading.get()
                fallback=|| view! { <div class="text-center p-4 text-muted">"Loading..."</div> }
            >
                <div node_ref=sentinel_ref class="h-2 w-full shrink-0" />
            </Show>
        </div>
    }
}

#[component]
fn ChatHeader(header: Memo<RoomHeader>, set_chat_sidebar_open: WriteSignal<bool>) -> impl IntoView {
    let member_store: MemberStore = expect_context();

    view! {
        <FloatingTile class="h-12 items-start flex-row gap-1 pl-1">
            <div class="w-10 self-center flex items-center justify-center">
                {move || match header.get() {
                    RoomHeader::Channel { .. } => view! {
                        <div class="w-8 text-end">
                            <span class="text-lg text-bright self-center align-middle">"#"</span>
                        </div>
                    }.into_any(),
                    RoomHeader::DM(handle) => {
                        let presence = member_store.get_presence(&handle.user_id);
                        let profile_sig = handle.profile;

                        view! {
                            <PresenceBadge presence=presence>
                                {move || profile_sig.get().render_icon(32)}
                            </PresenceBadge>
                        }
                    }.into_any(),
                    RoomHeader::Unknown => view! {
                        <div class="w-8 text-end">
                            <span class="text-lg text-bright self-center align-middle">"?"</span>
                        </div>
                    }.into_any()
                }}
            </div>
            <div class="flex-1 flex flex-col self-center text-bright text-m font-semibold">
                {move || match header.get() {
                    RoomHeader::Channel(node) => view! {
                        <span>
                            {node.name}
                        </span>
                    }.into_any(),
                    RoomHeader::DM(handle) => {
                        let profile_sig = handle.profile;
                        view! { {move || profile_sig.get().render_name(16)} }.into_any()
                    }.into_any(),
                    RoomHeader::Unknown => view! {
                        <span>
                            "Unknown Room"
                        </span>
                    }.into_any()
                }}
            </div>
            <div class="self-center">
                <button
                    class="text-bright hover:text-white transition-opacity"
                    on:click=move |_| set_chat_sidebar_open.update(|v| *v = !*v)
                >
                    "Toggle Sidebar"
                </button>
            </div>
        </FloatingTile>
    }
}

#[component]
pub fn Chat() -> impl IntoView {
    let state: AppState = expect_context();
    let member_store: MemberStore = expect_context();

    let header = Memo::new({
        let member_store = member_store.clone();
        move |_| state.get_room_header(member_store.clone())
    });

    let (chat_sidebar_open, set_chat_sidebar_open) = signal(true);

    view! {
        <div class="flex-1 h-full flex gap-[var(--gap)] flex-col overflow-hidden">
            <ChatHeader header=header set_chat_sidebar_open=set_chat_sidebar_open />
            <div class="flex flex-row h-full min-h-0">
                <FloatingTile class="flex-1 min-h-0, overflow-hidden">
                    <TimeLine/>
                    <input
                        type="text"
                        placeholder="Type a message..."
                               class="w-full h-15 border-1 border-[var(--tile-border-color)] bg-[rgba(0, 0, 0, 1)] outline-none text-[var(--text-color)]"
                    />
                </FloatingTile>
                <Show when=move || chat_sidebar_open.get()>
                    <div class="flex-shrink-0 h-full w-[20rem] ml-[var(--gap)]">
                        <FloatingTile class="w-full h-full overflow-hidden">
                            <ChatInfo header=header />
                        </FloatingTile>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[component]
fn ChatInfo(header: Memo<RoomHeader>) -> impl IntoView {
    let state: AppState = expect_context();

    view! {
        <div class="flex flex-col w-full overflow-visible">
            {move || match header.get() {
                RoomHeader::DM(handle) => {
                    let members: MemberStore = expect_context();
                    let presence = members.get_presence(&handle.user_id);
                    let profile_sig = handle.profile;
                    let banner_color = profile_sig
                        .get()
                        .map(|profile| profile.get_user_color().to_css_string())
                        .unwrap_or_else(|| "transparent".to_string());

                    let banner_height = 108.0;
                    let icon_size = 70.0;
                    let icon_radius = icon_size / 2.0;
                    let ring_width = 6.0;
                    let left_offset = 16.0;

                    let cutout_radius = icon_radius + ring_width;
                    let smooth_cutout_radius = cutout_radius + 0.5;

                    let cx = left_offset + icon_radius;
                    let cy = banner_height;

                    let banner_mask = format!(
                        "-webkit-mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
                         mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
                         -webkit-mask-composite: destination-out; \
                         mask-composite: exclude;"
                    );

                    let profile_sig_icon = profile_sig.clone();
                    let profile_sig_name = profile_sig.clone();

                    view! {
                        <div class="relative flex flex-col w-full">
                            <div
                                class="h-30 w-full"
                                style=format!("background-color: {banner_color}; {banner_mask}")
                            ></div>

                            <div class="absolute top-[73px] left-4">
                                <PresenceBadge presence=presence size=25.0>
                                    {move || profile_sig_icon.get().render_icon(icon_size as usize)}
                                </PresenceBadge>
                            </div>

                            <div class="px-4 pt-10 pb-6">
                                <h2 class="text-xl font-bold text-bright">
                                    {move || profile_sig_name.get().render_name(16)}
                                </h2>
                                <p class="text-sm text-muted">"Direct Message"</p>
                            </div>
                        </div>
                    }.into_any()
                },
                RoomHeader::Channel(..) => view! {
                    <MemberList room_id=state.active_room_id />
                }.into_any(),
                _ => view! { <div class="px-4 py-4">"..."</div> }.into_any()
            }}
        </div>
    }
}

#[derive(Serialize)]
struct MembersForRoom {
    room_id: String,
}

#[component]
fn MemberList(room_id: RwSignal<Option<String>>) -> impl IntoView {
    let member_store: MemberStore = expect_context();
    let store_clone = member_store.clone();

    let members = LocalResource::new(move || {
        let room_id = room_id.get().unwrap_or_default();
        let store = store_clone.clone();

        async move {
            let args = serde_wasm_bindgen::to_value(&MembersForRoom {
                room_id: room_id.clone(),
            })
            .expect("Failed to serialize members request");

            match call_tauri("get_members_for_room", args).await {
                Ok(js_val) => match serde_wasm_bindgen::from_value::<Vec<String>>(js_val) {
                    Ok(members) => members
                        .iter()
                        .map(|user_id| MemberProfileHandle {
                            user_id: user_id.clone(),
                            profile: store.get_profile(&room_id, user_id),
                        })
                        .collect(),
                    Err(e) => {
                        console_error(&format!("Failed to parse members response: {:?}", e));
                        Vec::new()
                    }
                },
                Err(e) => {
                    console_error(&format!("Tauri get_members_for_room call failed: {:?}", e));
                    Vec::new()
                }
            }
        }
    });

    view! {
        <div class="flex flex-col gap-2 p-3">
            <For
                each=move || members.get().unwrap_or_default()
                key=|member| member.user_id.clone()
                children=move |member| {
                    let presence = member_store.get_presence(&member.user_id);
                    let profile_sig = member.profile;
                    let sig_clone = profile_sig.clone();

                    view! {
                        <div class="flex items-center gap-2">
                            <PresenceBadge presence=presence size=15.5>
                                {move || profile_sig.get().render_icon(32)}
                            </PresenceBadge>
                            <span class="text-bright">
                                {move || sig_clone.get().render_name(16)}
                            </span>
                        </div>
                    }
                }
            />
        </div>
    }
}
