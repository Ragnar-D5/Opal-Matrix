use crate::{
    app::call_tauri,
    components::{
        input::{
            get_active_filter, get_caret_position, handle_input, handle_keydown,
            menu::{MenuType, SelectionMenu},
        },
        presence::PresenceBadge,
        previews::render_link,
        text::RichTextExt,
        user_profile::{UserProfileExt, UserProfileMaybeExt},
        FloatingTile, TextCircle, TextCircleProps,
    },
    hooks::use_tauri_event,
    state::{AppState, MemberProfileHandle, MemberStore, RoomHeader},
    tauri_functions::{fetch_messages, get_members, send_marker, MemberShip},
};

use colorsys::Hsl;
use log::error;
use phosphor_leptos::{Icon, IconWeight, HASH, INFO, TRASH, UPLOAD_SIMPLE, WARNING_CIRCLE};

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use leptos::{ev, html::Div, leptos_dom::logging::console_error, prelude::*, task::spawn_local};
use leptos_use::{use_event_listener, use_intersection_observer, UseIntersectionObserverReturn};
use serde_json::json;
use shared::{
    commands::Command,
    messages::{
        MembershipAction, MessageContent, MessageKind, MessageState, RepliesTo, RichTextSpan,
        SystemMessage, UiMessage, UserMessage,
    },
    sidebar::{RoomKind, RoomNode, SidebarState},
    user_profile::{PresenceStatus, UserProfile},
};
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
};

#[derive(PartialEq, Clone, Hash)]
struct GroupedMessage {
    message: UserMessage,
    state: MessageState,
    event_id: String,
}

#[derive(PartialEq, Clone)]
enum TimelineItemKind {
    MessageGroup(Vec<GroupedMessage>),
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

#[component]
fn ReplyPreview(replies_to: Option<RepliesTo>) -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let Some(room_id) = state.active_room_id.get_untracked() else {
        return view! {}.into_any();
    };

    if let Some(replies_to) = replies_to {
        if let Some(sender_id) = &replies_to.sender_id {
            let profile_sig = store.get_profile(&room_id, sender_id);
            let sig_clone = profile_sig.clone();

            let text = replies_to.text.clone().unwrap_or_default();

            view! {
                <div class="flex items-center gap-1 ml-[52px] mb-1 cursor-pointer text-xs relative group/reply">
                    <div class="absolute -left-[32px] top-[calc(50%-1px)] w-[28px] h-4.5 border-l-2 border-t-2 border-white/20 rounded-tl-md"></div>

                    <div class="shrink-0">{move || profile_sig.get().render_icon(16)}</div>

                    <span class="font-semibold text-bright hover:underline">
                        {move || sig_clone.get().render_name(12)}
                    </span>

                    <span class="truncate text-bright line-clamp-1">
                        {text
                            .into_iter()
                            .map(|v| v.render(store.clone(), room_id.clone()))
                            .collect_view()}
                    </span>
                </div>
            }
            .into_any()
        } else {
            view! {}.into_any()
        }
    } else {
        view! {}.into_any()
    }
}

fn render_user_message(
    message: &UserMessage,
    store: MemberStore,
    room_id: String,
) -> impl IntoView {
    match &message.content {
        MessageContent::Text { spans, is_edited } => {
            view! {
                <div class="text-normal leading-relaxed break-words">
                    {spans
                        .clone()
                        .into_iter()
                        .map(|v| v.render(store.clone(), room_id.clone()))
                        .collect_view()}
                    {if *is_edited {
                        view! { <span class="text-xs text-muted ml-2 italic">"(edited)"</span> }
                            .into_any()
                    } else {
                        view! {}.into_any()
                    }} {spans.clone().into_iter().map(render_link).collect_view()}
                </div>
            }
                .into_any()
        }
        MessageContent::Image {
            url,
            name,
            encryption_info,
            ..
        } => {
            let final_url = if let Some(enc) = encryption_info {
                let encoded_key = urlencoding::encode(&enc.key);
                let encoded_iv = urlencoding::encode(&enc.iv);
                format!("{}?key={}&iv={}", url, encoded_key, encoded_iv)
            } else {
                url.clone()
            };

            view! {
                <div class="mt-1">
                    <img
                        src=final_url
                        alt=name.clone()
                        class="max-w-sm rounded-md border border-[var(--tile-border-color)]"
                    />
                </div>
            }
                .into_any()
        }
        MessageContent::File { url, filename, size } => {
            view! {
                <div class="flex items-center gap-2 mt-1 p-2 rounded-md bg-white/5 border border-[var(--tile-border-color)] inline-flex">
                    <span class="text-xl">"📄"</span>
                    <a
                        href=url.clone()
                        target="_blank"
                        class="text-blue-400 hover:underline truncate max-w-xs"
                    >
                        {filename.clone()}
                    </a>
                    <span class="text-xs text-muted">
                        {format!("{:.1} KB", *size as f64 / 1024.0)}
                    </span>
                </div>
            }
                .into_any()
        }
        MessageContent::Encrypted => {
            view! {
                <div class="text-red-300 bold leading-relaxed break-words text-muted">
                    "Encrypted message"
                </div>
            }
                .into_any()
        }
        MessageContent::Deleted => {
            view! {
                <div class="text-muted italic leading-relaxed break-words flex flex-row items-center gap-1">
                    <Icon icon=TRASH size="20px" />
                    "This message was deleted"
                </div>
            }
                .into_any()
        }
    }
}

fn render_reactions(
    reactions: &HashMap<String, HashSet<String>>,
    store: MemberStore,
    room_id: &String,
    user_id: &String,
) -> impl IntoView {
    if reactions.is_empty() {
        return view! {}.into_any();
    }

    let content = reactions
        .iter()
        .map(|(emoji, reactors)| {
            let reactors_vec: Vec<String> = reactors.iter().cloned().collect();
            let emoji = emoji.clone();
            let store = store.clone();
            let room_id = room_id.clone();

            let reactor_pics = move || {
                let mut pics = Vec::new();

                let mut all_pics: Vec<(String, _)> = reactors_vec
                    .iter()
                    .filter_map(|user_id| {
                        store
                            .get_profile(&room_id, user_id)
                            .get()
                            .map(|p| (p.get_name(), p.render_icon(20).into_any()))
                    })
                    .collect();

                all_pics.sort_by_key(|(name, _)| name.clone());

                let len = all_pics.len();
                pics.extend(all_pics.into_iter().map(|(_, pic)| pic).take(4));

                if len > 4 {
                    pics.push(
                        TextCircle(TextCircleProps::builder().text(format!("+{}", len - 4)).class("w-[30px] h-[20px] rounded-full").color(Hsl::new(0.0, 0.0, 60.0, None)).build()).into_any()
                    );
                }

                pics.collect_view()
            };

            let contains_user = reactors.contains(user_id);

            view! {
                <div
                    class="flex items-center p-0.5 pr-1 rounded-lg border cursor-pointer transition-colors select-none"
                    class=("bg-(--ui-solid-bg)", !contains_user)
                    class=("hover:bg-(--ui-solid-hover-bg)", !contains_user)
                    class=("border-(--tile-border-color)", !contains_user)
                    class=("bg-(--accent-bg-color)", contains_user)
                    class=("border-(--accent-color)", contains_user)
                >
                    <span class="text-sm leading-none pl-1">{emoji.clone()}</span>

                    <span
                        class="pl-1 pr-1.5 font-bold text-sm min-w-[2ch] tabular-nums text-center -space-x-1.5"
                        class=("text-bright", contains_user)
                        class=("text-dim", !contains_user)
                    >
                        {reactors.len()}
                    </span>

                    <div class="flex flex-row items-center pl-0.5">{reactor_pics()}</div>
                </div>
            }
        })
        .collect_view();

    view! { <div class="flex flex-wrap gap-1 mt-1 mb-2">{content}</div> }.into_any()
}

fn render_message_group(
    group: Vec<GroupedMessage>,
    store: MemberStore,
    room_id: String,
    date: DateTime<Local>,
    own_user_id: String,
    profile_sig: ArcRwSignal<Option<UserProfile>>,
) -> impl IntoView {
    let first_msg = group.first();
    let first_message_mentions_user = first_msg
        .map(|msg| msg.message.mentions_user(&own_user_id))
        .unwrap_or(false);

    let first_reply_data = first_msg.and_then(|m| m.message.replies_to.clone());
    let highlight_reply_preview = first_message_mentions_user && first_reply_data.is_some();

    let name_sig = profile_sig.clone();

    group
    .iter()
    .enumerate()
    .map(|(idx, grouped_msg)| {
        let msg = &grouped_msg.message;
        let state = &grouped_msg.state;
        let is_first = idx == 0;
        let message_mentions_user = msg.mentions_user(&own_user_id)
            || msg.mentions.room;
        let reply_data = if is_first {
            first_reply_data.clone()
        } else {
            None
        };
        let show_highlight = message_mentions_user
            || (is_first && highlight_reply_preview);
        let hovered = RwSignal::new(false);
        let content = render_user_message(
            msg,
            store.clone(),
            room_id.clone(),
        );
        let failed = state == &MessageState::Failed;
        let reactions = msg.reactions.clone();

        let store = store.clone();

        view! {
            <div
                class="group/msg relative flex flex-col gap-[var(--gap)] hover:bg-black/20 ml-1 pl-4 py-[2px] rounded-md"
                style:background=move || {
                    let hovered = hovered.get();
                    if show_highlight {
                        format!(
                            "linear-gradient(in oklch to right, oklch(from var(--accent-color) l c h / {}) 20%, oklch(from var(--accent-color) l c h / 0) 100%)",
                            if hovered { "0.10" } else { "0.15" },
                        )
                    } else if hovered {
                        "rgba(0, 0, 0, 0.2)".to_string()
                    } else {
                        "transparent".to_string()
                    }
                }
                on:mouseenter=move |_| hovered.set(true)
                on:mouseleave=move |_| hovered.set(false)
            >
                {if show_highlight {
                    view! {
                        <div class="absolute left-1 top-1 bottom-1 w-1 rounded-full bg-[var(--accent-color)] pointer-events-none"></div>
                    }
                        .into_any()
                } else {
                    view! {}.into_any()
                }}

                {move || {
                    if hovered.get() && !is_first {
                        view! {
                            <div class="absolute text-xs text-muted mt-[5px] ml-[5px]">
                                {date.format("%H:%M").to_string()}
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}

                {if is_first {
                    view! { <ReplyPreview replies_to=reply_data.clone() /> }.into_any()
                } else {
                    view! {}.into_any()
                }}

                <div class="flex gap-[var(--gap)]">
                    <div class="shrink-0 mr-2 w-[40px] mt-[5px]">
                        {if is_first {
                            let profile_sig = profile_sig.clone();
                            view! {
                                {move || {
                                    let profile = profile_sig.get();
                                    profile.render_icon(40)
                                }}
                            }
                                .into_any()
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
                                    <span class="text-muted text-xs">{format_date(date)}</span>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {}.into_any()
                        }} <div>
                            <div class=("opacity-50", state != &MessageState::Sent)>{content}</div>
                            <Show when=move || failed>
                                <div class="flex items-center gap-1 mt-1 text-red-500 text-xs">
                                    <Icon
                                        icon=WARNING_CIRCLE
                                        weight=IconWeight::Duotone
                                        size="16px"
                                    />
                                    "Failed to send"
                                </div>
                            </Show>
                            {render_reactions(&reactions, store, &room_id, &own_user_id)}
                        </div>
                    </div>
                </div>
            </div>
        }
    })
    .collect_view()
}

impl TimelineItem {
    fn render_key(&self) -> String {
        match &self.kind {
            TimelineItemKind::MessageGroup(group) => {
                let mut hasher = DefaultHasher::new();
                for msg in group {
                    msg.hash(&mut hasher);
                }

                format!("{}-{:x}", self.id, hasher.finish())
            }
            _ => self.id.clone(),
        }
    }

    fn render(&self) -> impl IntoView {
        let state: AppState = expect_context();
        let store: MemberStore = expect_context();

        let Some(room_id) = state.active_room_id.get() else {
            return view! {}.into_any();
        };

        let profile_sig = store.get_profile(&room_id, &self.sender);

        let sender_id = self.sender.clone();

        let own_user_id = state.user_id.get();

        match &self.kind {
            TimelineItemKind::MessageGroup(group) => {
                view! {
                    <div class="flex flex-col gap-1 py-3 rounded-md">
                        <div class="flex flex-col w-full">
                            {render_message_group(
                                group.clone(),
                                store.clone(),
                                room_id.clone(),
                                self.date,
                                own_user_id.clone(),
                                profile_sig.clone(),
                            )}
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
                    <div class="flex items-center gap-2 my-4 drop-shadow">
                        <div class="flex-1 border-t-1 border-[var(--muted-text-color)] bdf"></div>
                        <span class="text-muted text-sm bdf-text">{label}</span>
                        <div class="flex-1 border-t-1 border-[var(--muted-text-color)] bdf"></div>
                    </div>
                }
            }
            .into_any(),
            TimelineItemKind::NewMessageIndicator => view! {
                <div class="flex items-center w-full pr-4">
                    <div class="flex-1 border-2 border-[#00ffff] rounded-full"></div>

                    <span class="relative flex items-center h-[20px] bg-[#00ffff] text-[var(--bg-color)] text-[10px] font-bold px-2 rounded-r-[3px] ml-1 uppercase tracking-wider select-none">
                        <div class="absolute -left-[6px] top-0 w-0 h-0 border-y-[10px] border-y-transparent border-r-[6px] border-r-[#00ffff]"></div>
                        "New"
                    </span>
                </div>
            }
            .into_any(),
            TimelineItemKind::SystemMessage(sys_msg) => {
                let sys_msg = sys_msg.clone();
                let sender_id = sender_id.clone();
                let room_id = room_id.clone();
                let store = store.clone();

                let text = move || {
                    let display_name = profile_sig.get()
                        .unwrap_or_default()
                        .display_name
                        .unwrap_or_else(|| sender_id.clone());

                    match &sys_msg {
                        SystemMessage::RoomCreated { .. } => format!("{} created the room", display_name),
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
                        SystemMessage::EncryptionEnabled { algorithm } => {
                            format!("{} enabled encryption ({})", display_name, algorithm)
                        }
                        SystemMessage::PowerlevelChange => {
                            format!("{} changed the power levels", display_name)
                        }
                        SystemMessage::JoinRuleChange { new_rule } => {
                            format!("{} changed the join rules to '{}'", display_name, new_rule.as_str())
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
                        SystemMessage::MessageEdited { .. } => format!("{} edited a message", display_name),
                        SystemMessage::MessageReacted { .. } => format!("{} reacted to a message", display_name),
                        SystemMessage::MessageRedacted { .. } => format!("{} redacted a message", display_name),
                        SystemMessage::RemoveMessage { .. } => format!("{} removed a message", display_name),
                        SystemMessage::RoomAvatarChange { new_avatar_url } => {
                            if new_avatar_url.is_some() {
                                format!("{} changed the room avatar", display_name)
                            } else {
                                format!("{} removed the room avatar", display_name)
                            }
                        }
                        SystemMessage::VerificationRequest { from_user_id, .. } => {
                            let from_display_name = store
                                .get_profile(&room_id, from_user_id)
                                .get()
                                .and_then(|p| p.display_name)
                                .unwrap_or_else(|| from_user_id.clone());

                            format!(
                                "{} sent a verification request to {}",
                                display_name, from_display_name,
                            )
                        }
                        SystemMessage::Unknown => format!("{} sent an unknown message", display_name),
                    }
                };

                view! {
                    <div class="flex items-center justify-center my-2">
                        <span class="text-muted text-xxl bdf-text">{text}</span>
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

#[component]
fn TimeLine() -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<UiMessage>::new());
    let (read_marker_id, set_read_marker_id) = signal::<Option<String>>(None);

    let state: AppState = expect_context();

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    let messages_update_event: ReadSignal<Option<HashMap<String, Vec<UiMessage>>>> =
        use_tauri_event("messages_update");

    Effect::new(move |_| {
        let Some(update) = messages_update_event.get() else {
            return;
        };

        let Some(room_id) = state.active_room_id.get() else {
            return;
        };

        let Some(new_msgs) = update.get(&room_id) else {
            return;
        };

        set_messages.update(|existing| {
            let mut seen_ids: HashSet<String> =
                existing.iter().map(|m| m.event_id.clone()).collect();

            for msg in new_msgs {
                match &msg.kind {
                    MessageKind::SystemMessage(SystemMessage::RemoveMessage { event_id }) => {
                        existing.retain(|m| &m.event_id != event_id);
                    }
                    _ => {
                        if seen_ids.contains(&msg.event_id) {
                            existing.retain(|m| m.event_id != msg.event_id);
                        }
                        existing.push(msg.clone());
                        seen_ids.insert(msg.event_id.clone());
                    }
                }
            }
        });
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

    let (last_sent_event_id, set_last_sent_event_id) = signal::<Option<String>>(None);

    Effect::new(move |_| {
        if !state.is_focused.get() {
            return;
        }

        let Some(rid) = state.active_room_id.get() else {
            return;
        };
        let Some(newest) = messages
            .get()
            .iter()
            .filter_map(|m| {
                if m.state != MessageState::Sent {
                    None
                } else {
                    Some((m.timestamp, m.event_id.clone()))
                }
            })
            .max_by_key(|m| m.0)
            .map(|m| m.1)
        else {
            return;
        };

        if last_sent_event_id.get_untracked().as_deref() == Some(&newest) {
            return;
        }

        set_last_sent_event_id.set(Some(newest.clone()));
        send_marker(rid, newest);
    });

    let (show_unread_indicator, set_show_unread_indicator) = signal(false);

    Effect::new(move |_| {
        state.active_room_id.get();
        set_show_unread_indicator.set(false);
    });

    Effect::new(move |_| {
        if has_unread_notifications.get() {
            set_show_unread_indicator.set(true);
        }
    });

    let flattened_items = Memo::new(move |_| {
        let mut msgs = messages.get();
        msgs.sort_by_key(|m| m.timestamp);

        let mut items: Vec<TimelineItem> = Vec::new();
        let mut last_day: Option<NaiveDate> = None;
        let marker_id = read_marker_id.get();
        let has_unread = show_unread_indicator.get();

        let mut edits = HashMap::new();
        let mut redactions = HashSet::new();
        let mut reactions_map: HashMap<String, HashMap<String, HashSet<String>>> = HashMap::new();

        let mut processed_messages = HashMap::new();

        let marker_ts = marker_id
            .as_ref()
            .and_then(|mid| msgs.iter().find(|m| m.event_id == *mid))
            .map(|m| m.timestamp);

        msgs = msgs
            .into_iter()
            .filter(|msg| match &msg.kind {
                MessageKind::SystemMessage(SystemMessage::MessageRedacted { event_id, .. }) => {
                    redactions.insert(event_id.clone());
                    false
                }
                _ => true,
            })
            .collect();

        msgs = msgs
            .into_iter()
            .filter(|msg| {
                if redactions.contains(&msg.event_id) {
                    if matches!(msg.kind, MessageKind::SystemMessage(_)) {
                        return false;
                    }
                }

                let result = match &msg.kind {
                    MessageKind::SystemMessage(SystemMessage::MessageEdited {
                        event_id,
                        new_spans,
                    }) => {
                        edits.insert(event_id.clone(), new_spans.clone());
                        false
                    }
                    MessageKind::SystemMessage(SystemMessage::MessageReacted {
                        event_id,
                        ref reaction,
                    }) => {
                        // Add user id to vec of hashmap at key user id
                        reactions_map
                            .entry(event_id.clone())
                            .or_default()
                            .entry(reaction.clone())
                            .or_default()
                            .insert(msg.sender_id.clone());
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
            if let Some(new_spans) = edits.get(&msg.event_id) {
                msg.edit(new_spans.clone());
            }
            if let Some(reactions) = reactions_map.get(&msg.event_id) {
                msg.add_reactions(reactions);
            }

            let current_date = get_date_from_ts(msg.timestamp as i64);
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
                                    MessageContent::Text { spans, .. } => spans.clone(),
                                    _ => {
                                        vec![RichTextSpan::Plain("[non-text content]".to_string())]
                                    }
                                },
                                MessageKind::SystemMessage(_) => {
                                    vec![RichTextSpan::Plain("[system message]".to_string())]
                                }
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
                                .last()
                                .map(|m| m.message.replies_to.is_some())
                                .unwrap_or(false);

                            if same_sender && same_minute && !current_is_reply && !last_is_reply {
                                group.push(GroupedMessage {
                                    message: user_msg.clone(),
                                    state: msg.state.clone(),
                                    event_id: msg.event_id.clone(),
                                });
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
                            kind: TimelineItemKind::MessageGroup(vec![GroupedMessage {
                                message: user_msg.clone(),
                                state: msg.state.clone(),
                                event_id: msg.event_id.clone(),
                            }]),
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

        let Some(room_id) = state.active_room_id.get_untracked() else {
            return;
        };

        set_is_loading.set(true);

        spawn_local(async move {
            let oldest_id = messages
                .get_untracked()
                .iter()
                .min_by_key(|m| m.timestamp)
                .map(|m| m.event_id.clone());

            let result = fetch_messages(room_id, oldest_id).await;

            match result {
                Ok(res) => {
                    set_has_more.set(res.has_more);

                    set_messages.update(|existing| {
                        let mut seen_ids: HashSet<String> =
                            existing.iter().map(|m| m.event_id.clone()).collect();

                        let mut unique_new: Vec<UiMessage> = res
                            .messages
                            .into_iter()
                            .filter(|m| seen_ids.insert(m.event_id.clone()))
                            .collect();

                        unique_new.append(existing);
                        *existing = unique_new;
                    });

                    set_initial_loaded.set(true);
                }
                Err(e) => {
                    error!("Failed to fetch messages: {}", e);
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
        if state.active_room_id.get().is_some() {
            set_messages.set(Vec::new());
            set_is_loading.set(false);
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
        set_read_marker_id.set(None);

        let Some(room_id) = rid else {
            return;
        };

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&json!({
                "room_id": room_id
            }))
            .expect("Failed to serialize args");

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
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse py-2 overflow-anchor-auto">
            <div class="mb-5"></div>
            <For
                each=move || flattened_items.get()
                key=|item| item.render_key()
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
fn ChatHeader(
    header: Memo<RoomHeader>,
    chat_sidebar_open: ReadSignal<bool>,
    set_chat_sidebar_open: WriteSignal<bool>,
) -> impl IntoView {
    let member_store: MemberStore = expect_context();

    let (info_hovered, set_info_hovered) = signal(false);

    view! {
        <FloatingTile class="h-(--header-height) items-start flex-row gap-1 pl-[5px]">
            <div class="w-8 self-center flex items-center justify-center">
                {move || match header.get() {
                    RoomHeader::Channel { .. } => {
                        view! {
                            <div class="text-(--ui-base-color) w-full justify-center flex">
                                <Icon icon=HASH color="currentColor" size="70%" />
                            </div>
                        }
                            .into_any()
                    }
                    RoomHeader::DM(handle) => {
                        {
                            let presence = member_store.get_presence(&handle.user_id);
                            let profile_sig = handle.profile;

                            view! {
                                {move || {
                                    if let Some(profile) = profile_sig.get() {
                                        let presence = presence.clone();
                                        view! {
                                            <PresenceBadge presence=presence size=14.0>
                                                {profile.render_icon(30)}
                                            </PresenceBadge>
                                        }
                                            .into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}
                            }
                        }
                            .into_any()
                    }
                    RoomHeader::Unknown => {
                        view! {
                            <div class="w-8 text-end">
                                <span class="text-lg text-bright self-center align-middle">
                                    "?"
                                </span>
                            </div>
                        }
                            .into_any()
                    }
                }}
            </div>
            <div class="flex-1 flex flex-col self-center text-bright text-m font-semibold">
                {move || match header.get() {
                    RoomHeader::Channel(node) => view! { <span>{node.name}</span> }.into_any(),
                    RoomHeader::DM(handle) => {
                        {
                            let profile_sig = handle.profile;
                            view! { {move || profile_sig.get().render_name(16)} }.into_any()
                        }
                            .into_any()
                    }
                    RoomHeader::Unknown => view! { <span>"Unknown Room"</span> }.into_any(),
                }}
            </div>
            <div class="self-center h-full">
                <button
                    class="transition-opacity h-full mr-1"
                    class=("text-(--ui-hover-color)", move || info_hovered.get())
                    class=("text-(--ui-base-color)", move || !info_hovered.get())
                    on:click=move |_| set_chat_sidebar_open.update(|v| *v = !*v)
                    on:mouseenter=move |_| set_info_hovered.set(true)
                    on:mouseleave=move |_| set_info_hovered.set(false)
                >
                    <div class="h-full justify-center items-center flex cursor-pointer">
                        <Icon
                            icon=INFO
                            size="80%"
                            color="currentColor"
                            weight=move || {
                                if chat_sidebar_open.get() {
                                    IconWeight::Fill
                                } else {
                                    IconWeight::Light
                                }
                            }
                        />
                    </div>
                </button>
            </div>
        </FloatingTile>
    }
}

#[component]
fn ChatInput() -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let menu = RwSignal::new(MenuType::None);
    let selected_index = RwSignal::new(0);

    provide_context(selected_index);

    let mention_matches = RwSignal::new(Vec::<MemberShip>::new());
    let command_matches = RwSignal::new(Vec::<Command>::new());

    provide_context(mention_matches);
    provide_context(command_matches);

    let input_ref = NodeRef::<Div>::new();

    let _ = use_event_listener(document(), ev::selectionchange, move |_| {
        let Some(el) = input_ref.get() else {
            return;
        };

        let win = window();
        if let Ok(Some(sel)) = win.get_selection() {
            if let Some(focus_node) = sel.focus_node() {
                if el.contains(Some(&focus_node)) {
                    let caret_pos = get_caret_position(&el);

                    if let Some(filter) = get_active_filter(&el, caret_pos, '@') {
                        menu.set(MenuType::UserAutocomplete { filter });
                        return;
                    }
                    if let Some(filter) = get_active_filter(&el, caret_pos, '/') {
                        menu.set(MenuType::CommandAutocomplete { filter });
                        return;
                    } else {
                        menu.set(MenuType::None);
                    }
                } else {
                    if menu.get_untracked() != MenuType::None {
                        menu.set(MenuType::None);
                    }
                }
            }
        }
    });

    // Focus the input when the component mounts or when the active room changes
    Effect::new(move |_| {
        state.active_room_id.get();

        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });

    let is_empty = RwSignal::new(true);

    // Load on room change
    Effect::new(move |_| {
        let room_id = state.active_room_id.get();
        let draft = room_id.and_then(|rid| state.drafts.with_untracked(|d| d.get(&rid).cloned()));

        let Some(el) = input_ref.get() else {
            return;
        };
        el.set_inner_html(draft.clone().unwrap_or("<br>".into()).as_str());

        is_empty.set(draft.is_none() || draft.as_deref() == Some("<br>"));

        let win = window();
        let doc = document();

        if let Ok(Some(sel)) = win.get_selection() {
            if let Ok(range) = doc.create_range() {
                let _ = range.select_node_contents(&el);
                // collapse_with_to_start(false) collapses the selection to the END
                range.collapse_with_to_start(false);

                let _ = sel.remove_all_ranges();
                let _ = sel.add_range(&range);
            }
        }

        let _ = el.focus();
    });

    view! {
        <div class="p-2 pt-0 w-full rounded-full relative">
            <SelectionMenu menu=menu input_ref=input_ref />
            <div class="text-(--bright-text-color) w-full min-h-13 border-1 border-[var(--tile-border-color)] rounded-(--ui-border-radius) bg-[rgba(0, 0, 0, 0.6)] flex flex-row bg-(--ui-floating-bg) items-center gap-3 px-3">
                <Icon icon=UPLOAD_SIMPLE size="20px" color="var(--ui-base-color)" />
                <div class="relative flex-1 min-w-0 flex items-center">
                    <Show when=move || is_empty.get()>
                        <div class="text-muted absolute left-0 top-0 pointer-events-none select-none py-3">
                            "Type a message..."
                        </div>
                    </Show>
                    <div
                        node_ref=input_ref
                        contenteditable="true"
                        class="text-(--bright-text-color) outline-none w-full whitespace-pre-wrap break-words py-3 max-h-100 overflow-y-auto"
                        on:input=move |_| handle_input(input_ref, is_empty, state)
                        on:keydown=move |ev| handle_keydown(
                            ev,
                            input_ref,
                            menu,
                            selected_index,
                            mention_matches,
                            command_matches,
                            state,
                            store.clone(),
                            is_empty,
                        )
                    ></div>
                </div>
            </div>
        </div>
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
            <ChatHeader
                header=header
                chat_sidebar_open=chat_sidebar_open
                set_chat_sidebar_open=set_chat_sidebar_open
            />
            <div class="flex flex-row h-full min-h-0">
                <FloatingTile class="flex-1 flex flex-col h-full min-h-0 overflow-hidden">
                    <TimeLine />
                    <ChatInput />
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
                        .map(|profile| profile.get_color().to_css_string())
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
                         mask-composite: exclude;",
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
                                {move || {
                                    if let Some(profile) = profile_sig_icon.get() {
                                        let presence = presence.clone();
                                        view! {
                                            <PresenceBadge presence=presence size=25.0>
                                                {profile.render_icon(icon_size as usize)}
                                            </PresenceBadge>
                                        }
                                            .into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}
                            </div>

                            <div class="px-4 pt-10 pb-6">
                                <h2 class="text-xl font-bold text-bright">
                                    {move || profile_sig_name.get().render_name(16)}
                                </h2>
                                <p class="text-sm text-muted">"Direct Message"</p>
                            </div>
                        </div>
                    }
                        .into_any()
                }
                RoomHeader::Channel(..) => {
                    view! { <MemberList room_id=state.active_room_id /> }.into_any()
                }
                _ => view! { <div class="px-4 py-4">"..."</div> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn MemberList(room_id: RwSignal<Option<String>>) -> impl IntoView {
    let member_store: MemberStore = expect_context();
    let store_clone = member_store.clone();

    let members = LocalResource::new(move || {
        let room_id = room_id.get().unwrap_or_default();
        let store = store_clone.clone();

        async move {
            let mut online = Vec::new();
            let mut offline = Vec::new();

            let Ok(members) = get_members(room_id.clone()).await else {
                return (Vec::new(), Vec::new());
            };

            members.iter().for_each(|member| {
                let user_id = &member.user_id;
                let presence = store.get_presence(user_id);

                let el = (
                    MemberProfileHandle {
                        user_id: user_id.clone(),
                        profile: store.get_profile(&room_id, user_id),
                    },
                    presence.clone(),
                );

                if presence.get().status == PresenceStatus::Offline {
                    offline.push(el);
                } else {
                    online.push(el);
                }
            });

            (online, offline)
        }
    });

    view! {
        <div class="flex flex-col gap-2 p-3">
            {move || {
                let (online, offline) = members.get().unwrap_or_default();
                let online_i = online.len();
                let offline_i = offline.len();
                let header = view! { <div class="flex flex-row"></div> }.into_any();
                let online_view = if online_i > 0 {

                    view! {
                        <h3 class="text-sm text-muted font-semibold">
                            {move || {
                                format!("Online — {}", members.get().unwrap_or_default().0.len())
                            }}
                        </h3>

                        <For
                            each=move || members.get().unwrap_or_default().0
                            key=|(member, _)| member.user_id.clone()
                            children=move |(member, presence)| {
                                let profile_sig = member.profile;
                                let sig_clone = profile_sig.clone();

                                view! {
                                    <div class="flex items-center gap-2">
                                        {move || {
                                            if let Some(profile) = profile_sig.get() {
                                                let presence = presence.clone();
                                                view! {
                                                    <PresenceBadge presence=presence size=15.5>
                                                        {profile.render_icon(32)}
                                                    </PresenceBadge>
                                                }
                                                    .into_any()
                                            } else {
                                                view! {}.into_any()
                                            }
                                        }}
                                        <span class="text-bright">
                                            {move || sig_clone.get().render_name(16)}
                                        </span>
                                    </div>
                                }
                            }
                        />

                        <div class="h-3"></div>
                    }
                        .into_any()
                } else {
                    view! {}.into_any()
                };
                let offline_view = if offline_i > 0 {

                    view! {
                        <h3 class="text-sm text-muted font-semibold">
                            {move || {
                                format!("Offline — {}", members.get().unwrap_or_default().1.len())
                            }}
                        </h3>

                        <For
                            each=move || members.get().unwrap_or_default().1
                            key=|(member, _)| member.user_id.clone()
                            children=move |(member, presence)| {
                                let profile_sig = member.profile;
                                let sig_clone = profile_sig.clone();

                                view! {
                                    <div class="flex items-center gap-2 opacity-30">
                                        {move || {
                                            if let Some(profile) = profile_sig.get() {
                                                let presence = presence.clone();
                                                view! {
                                                    <PresenceBadge presence=presence size=15.5>
                                                        {profile.render_icon(32)}
                                                    </PresenceBadge>
                                                }
                                                    .into_any()
                                            } else {
                                                view! {}.into_any()
                                            }
                                        }}
                                        <span class="text-bright">
                                            {move || sig_clone.get().render_name(16)}
                                        </span>
                                    </div>
                                }
                            }
                        />
                    }
                        .into_any()
                } else {
                    view! {}.into_any()
                };

                view! {
                    {header}
                    {online_view}
                    {offline_view}
                }
                    .into_any()
            }}

        </div>
    }
}
