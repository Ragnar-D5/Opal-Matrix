use crate::state::{AppState, MemberProfileHandle, MemberStore, RoomHeader};
use crate::tauri_functions::send_marker;
use leptos::prelude::*;
use log::error;
use shared::user_profile::PresenceStatus;
use std::collections::{HashMap, HashSet};

use crate::app::{call_tauri, openUrl};
use crate::components::presence::PresenceBadge;
use crate::components::FloatingTile;
use crate::hooks::use_tauri_event;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use leptos::html::Div;
use leptos::leptos_dom::logging::console_error;
use leptos::task::spawn_local;
use leptos_use::{use_intersection_observer, UseIntersectionObserverReturn};
use serde::Serialize;
use shared::messages::{
    MembershipAction, MessageContent, MessageKind, Reaction, RepliesTo, RichTextSpan,
    SystemMessage, UiMessage, UserMessage,
};
use shared::sidebar::{RoomKind, RoomNode, SidebarState};
use web_sys::{DomParser, MouseEvent, SupportedType};

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

fn render_span(span: RichTextSpan) -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let Some(room_id) = state.active_room_id.get_untracked() else {
        return view! {}.into_any();
    };

    match span {
        RichTextSpan::Plain(text) => view! { <span class="cursor-text">{text}</span> }.into_any(),

        RichTextSpan::UserMention {
            user_id,
            display_name,
        } => {
            let profile_sig = store.get_profile(&room_id, &user_id);

            let color = Memo::new(move |_| {
                let profile = profile_sig.get().unwrap_or_default();
                profile.get_user_color().to_css_string()
            });

            view! {
                <span class="relative p-[2px] group cursor-pointer">
                    <span
                        class="absolute inset-0 rounded -z-10 opacity-10 group-hover:opacity-40 transition-opacity duration-200"
                        style:background-color=move || color.get()
                    />

                    <span class="relative" style:color=move || color.get() title=user_id>
                        "@"
                        {display_name}
                    </span>
                </span>
            }
            .into_any()
        }

        RichTextSpan::RoomMention => view! {
            <span class="bg-[#FEE75C]/30 text-[#FEE75C] px-1 mx-0.5 rounded font-medium">
                "@room"
            </span>
        }
        .into_any(),

        RichTextSpan::Link { url, .. } => {
            let clone = url.clone();

            let on_click = move |ev: MouseEvent| {
                ev.prevent_default(); // Stop the webview from navigating
                let u = clone.clone();
                spawn_local(async move {
                    let _ = openUrl(&u);
                });
            };

            view! {
                <a
                    href=url.clone()
                    target="_blank"
                    class="text-[#00A8FC] hover:underline"
                    on:click=on_click
                >
                    {url.clone()}
                </a>
            }
            .into_any()
        }
    }
}

#[derive(Serialize)]
struct FetchArgs {
    url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OgData {
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
}

pub fn parse_og_tags_in_browser(html: &str) -> Option<OgData> {
    let parser = DomParser::new().ok()?;
    let doc = parser
        .parse_from_string(html, SupportedType::TextHtml)
        .ok()?;

    let get_meta = |property: &str| -> Option<String> {
        let selector = format!("meta[property='{}'], meta[name='{}']", property, property);
        if let Ok(Some(element)) = doc.query_selector(&selector) {
            element.get_attribute("content")
        } else {
            None
        }
    };

    let title = get_meta("og:title").or_else(|| {
        doc.query_selector("title")
            .ok()
            .flatten()
            .and_then(|t| t.text_content())
    })?;

    let description = get_meta("og:description").or_else(|| get_meta("description"));
    let image = get_meta("og:image");

    Some(OgData {
        title,
        description,
        image,
    })
}

async fn fetch_preview_data(url: String) -> Result<OgData, String> {
    // Convert our arguments to a JS object for Tauri
    let args = serde_wasm_bindgen::to_value(&FetchArgs { url }).map_err(|e| e.to_string())?;

    // Call the Rust backend to bypass CORS
    let html_js = call_tauri("fetch_raw_html", args)
        .await
        .map_err(|e| e.as_string().unwrap_or_default())?;

    // Extract the string and parse it
    let html_str = html_js.as_string().ok_or("Failed to fetch HTML")?;
    parse_og_tags_in_browser(&html_str).ok_or("No OpenGraph tags found".to_string())
}

fn render_link(span: RichTextSpan) -> impl IntoView {
    let RichTextSpan::Link { url, .. } = span else {
        return view! {}.into_any();
    };

    let preview = LocalResource::new(move || {
        let fetch_url = url.clone();
        async move { fetch_preview_data(fetch_url).await }
    });

    view! {
        <Suspense fallback=move || {
            view! {
                <div class="animate-pulse bg-white/5 w-full max-w-sm h-24 rounded-md mt-2"></div>
            }
        }>
            {move || {
                preview
                    .get()
                    .and_then(|res| res.ok())
                    .map(|data| {
                        view! {
                            <div class="flex flex-col max-w-sm rounded-md bg-white/5 border border-white/10 overflow-hidden mt-2 cursor-pointer hover:bg-white/10">
                                {data
                                    .image
                                    .map(|img| {
                                        view! { <img src=img class="w-full h-32 object-cover" /> }
                                    })} <div class="p-3">
                                    <span class="text-sm font-bold text-bright line-clamp-1">
                                        {data.title}
                                    </span>
                                    <span class="text-xs text-muted line-clamp-2 mt-1">
                                        {data.description}
                                    </span>
                                </div>
                            </div>
                        }
                    })
            }}
        </Suspense>
    }.into_any()
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
                        {text.into_iter().map(render_span).collect_view()}
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

        let own_user_id = state.user_id.get();

        match &self.kind {
            TimelineItemKind::MessageGroup(group) => {
                let first_msg = group.contents.first();
                let first_message_mentions_user = first_msg
                    .and_then(|msg| msg.mentions.as_ref())
                    .map(|mentions| mentions.user_ids.contains(&own_user_id))
                    .unwrap_or(false);

                let first_reply_data = first_msg.and_then(|m| m.replies_to.clone());
                let highlight_reply_preview = first_message_mentions_user && first_reply_data.is_some();

                view! {
                    <div class="flex flex-col gap-1 py-3 rounded-md">
                        <div class="flex flex-col w-full">
                            {group
                                .contents
                                .iter()
                                .enumerate()
                                .map(|(idx, msg)| {
                                    let is_first = idx == 0;
                                    let message_mentions_user = msg
                                        .mentions
                                        .as_ref()
                                        .map(|mentions| mentions.user_ids.contains(&own_user_id))
                                        .unwrap_or(false);
                                    let reply_data = if is_first {
                                        first_reply_data.clone()
                                    } else {
                                        None
                                    };
                                    let show_highlight = message_mentions_user
                                        || (is_first && highlight_reply_preview);
                                    let (hovered, set_hovered) = signal(false);
                                    let mut reaction_counts: HashMap<String, usize> = HashMap::new();
                                    for r in &msg.reactions {
                                        *reaction_counts.entry(r.reaction.clone()).or_insert(0)
                                            += 1;
                                    }
                                    let content = match &msg.content {
                                        MessageContent::Text { spans, is_edited } => {

                                            view! {
                                                <div class="text-normal leading-relaxed break-words">
                                                    {spans.clone().into_iter().map(render_span).collect_view()}
                                                    {if *is_edited {
                                                        view! {
                                                            <span class="text-xs text-muted ml-2 italic">
                                                                "(edited)"
                                                            </span>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! {}.into_any()
                                                    }}
                                                    {spans.clone().into_iter().map(render_link).collect_view()}
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
                                                <div class="text-muted italic leading-relaxed break-words">
                                                    "This message was deleted"
                                                </div>
                                            }
                                                .into_any()
                                        }
                                    };

                                    view! {
                                        <div
                                            class="group/msg relative flex flex-col gap-[var(--gap)] hover:bg-black/20 ml-1 pl-4 py-[2px] rounded-md"
                                            style:background=move || {
                                                let hovered = hovered.get();
                                                if show_highlight {
                                                    format!(
                                                        "linear-gradient(in oklch to right, oklch(from var(--accent-color) l c h / {}) 20%, oklch(from var(--accent-color) l c h / 0) 100%)",
                                                        if hovered { "0.05" } else { "0.15" },
                                                    )
                                                } else if hovered {
                                                    "rgba(0, 0, 0, 0.2)".to_string()
                                                } else {
                                                    "transparent".to_string()
                                                }
                                            }
                                            on:mouseenter=move |_| set_hovered.set(true)
                                            on:mouseleave=move |_| set_hovered.set(false)
                                        >
                                            {if show_highlight {
                                                view! {
                                                    <div class="absolute left-1 top-1 bottom-1 w-1 rounded-full bg-[var(--accent-color)] pointer-events-none"></div>
                                                }
                                                    .into_any()
                                            } else {
                                                view! {}.into_any()
                                            }}

                                            {if is_first {
                                                view! { <ReplyPreview replies_to=reply_data.clone() /> }
                                                    .into_any()
                                            } else {
                                                view! {}.into_any()
                                            }}

                                            <div class="flex gap-[var(--gap)]">
                                                <div class="shrink-0 mr-2 w-[40px] self-center">
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
                                                                <span class="text-muted text-xs">
                                                                    {format_date(self.date)}
                                                                </span>
                                                            </div>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! {}.into_any()
                                                    }}
                                                    <div>
                                                        {content}
                                                        {if !reaction_counts.is_empty() {
                                                            view! {
                                                                <div class="flex flex-wrap gap-1 mt-1 mb-2">
                                                                    {reaction_counts
                                                                        .into_iter()
                                                                        .map(|(emoji, count)| {
                                                                            view! {
                                                                                <div class="flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-white/10 border border-white/5 hover:bg-white/20 cursor-pointer">
                                                                                    <span class="text-sm leading-none">{emoji}</span>
                                                                                    <span class="text-[10px] font-medium text-muted">
                                                                                        {count}
                                                                                    </span>
                                                                                </div>
                                                                            }
                                                                        })
                                                                        .collect_view()}
                                                                </div>
                                                            }
                                                                .into_any()
                                                        } else {
                                                            view! {}.into_any()
                                                        }}
                                                    </div>
                                                </div>
                                            </div>
                                        </div>
                                    }
                                })
                                .collect_view()}
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
                        <span class="text-muted text-sm">{label}</span>
                        <div class="flex-1 border-t-1 border-[var(--muted-text-color)]"></div>
                    </div>
                }
            }
            .into_any(),
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
                        <span class="text-muted text-xxl">{text}</span>
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

    let state: AppState = expect_context();

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

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
            .max_by_key(|m| m.timestamp)
            .map(|m| m.event_id.clone())
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
                        new_spans,
                    }) => {
                        edits.insert(event_id.clone(), new_spans.clone());
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
            if let Some(new_spans) = edits.get(&msg.event_id) {
                msg.edit(new_spans.clone());
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
        set_read_marker_id.set(None);

        let Some(room_id) = rid else {
            return;
        };

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&ReceiptArgs {
                room_id: room_id.clone(),
            })
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
                    RoomHeader::Channel { .. } => {
                        view! {
                            <div class="w-8 text-end">
                                <span class="text-lg text-bright self-center align-middle">
                                    "#"
                                </span>
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
                                            <PresenceBadge presence=presence>
                                                {profile.render_icon(32)}
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
fn ChatInput(header: Memo<RoomHeader>) -> impl IntoView {
    view! {
        <div class="p-2 w-full rounded-full">
            <input
                type="text"
                placeholder=move || match header.get() {
                    RoomHeader::Channel(node) => {
                        format!("Message #{}", node.name.clone().unwrap_or(node.room_id.clone()))
                    }
                    RoomHeader::DM(handle) => {
                        format!(
                            "Message @{}",
                            handle
                                .profile
                                .get()
                                .unwrap_or_default()
                                .display_name
                                .unwrap_or(handle.user_id.clone()),
                        )
                    }
                    RoomHeader::Unknown => "Message someone".to_string(),
                }
                class="w-full h-13 border-1 border-[var(--tile-border-color)] outline-none text-[var(--text-color)] p-3 rounded-lg bg-[rgba(0, 0, 0, 0.6)]"
                style="background-color: rgba(0, 0, 0, 0.2);"
                autofocus
            />
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
            <ChatHeader header=header set_chat_sidebar_open=set_chat_sidebar_open />
            <div class="flex flex-row h-full min-h-0">
                <FloatingTile class="flex-1 min-h-0, overflow-hidden">
                    <TimeLine />
                    <ChatInput header=header />
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
            let mut online = Vec::new();
            let mut offline = Vec::new();

            let args = serde_wasm_bindgen::to_value(&MembersForRoom {
                room_id: room_id.clone(),
            })
            .expect("Failed to serialize members request");

            match call_tauri("get_members_for_room", args).await {
                Ok(js_val) => match serde_wasm_bindgen::from_value::<Vec<String>>(js_val) {
                    Ok(members) => members.iter().for_each(|user_id| {
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
                    }),
                    Err(e) => {
                        error!("Failed to parse members response: {:?}", e);
                    }
                },
                Err(e) => {
                    error!("Tauri get_members_for_room call failed: {:?}", e);
                }
            }

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
