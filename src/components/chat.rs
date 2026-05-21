use crate::{
    components::{
        FloatingTile, TextCircle, TextCircleProps, get_color,
        input::{
            get_active_filter, get_caret_position, handle_input, handle_keydown,
            menu::{MenuType, SelectionMenu},
        },
        presence::PresenceBadge,
        previews::render_link,
        text::RichTextExt,
        user_profile::{
            UserProfileExt, UserProfileMaybeExt, render_profile_icon, render_profile_name,
        },
    },
    hooks::use_tauri_event,
    state::{AppState, MemberProfileHandle, MemberStore, RoomHeader},
    tauri_functions::{get_members, get_timeline, scroll_up},
};

use colorsys::Hsl;
use phosphor_leptos::{HASH, INFO, Icon, IconWeight, PHONE, TRASH, UPLOAD_SIMPLE, WARNING_CIRCLE};

use chrono::{DateTime, Local, TimeZone};
use leptos::{ev, html::Div, prelude::*, task::spawn_local};
use leptos_use::{UseIntersectionObserverReturn, use_event_listener, use_intersection_observer};
use shared::{
    commands::Command,
    timeline::{
        Change, DetailState, EventContent, MessageContent, SystemMessage, TimelineEvent,
        UiMessageType, UiTimelineDiff, UiTimelineItem, UiTimelineItemKind,
    },
    user_profile::{PresenceStatus, UserProfile},
};
use std::collections::HashMap;
use web_sys::IntersectionObserverEntry;

fn format_date(date: DateTime<Local>) -> String {
    match (date.date_naive() - Local::now().date_naive()).num_days() {
        0 => date.format("Today, %H:%M").to_string(),
        -1 => date.format("Yesterday, %H:%M").to_string(),
        _ => date.format("%d/%m/%Y, %H:%M").to_string(),
    }
}

// #[component]
// fn ReplyPreview(replies_to: Option<RepliesTo>) -> impl IntoView {
//     let state: AppState = expect_context();
//     let store: MemberStore = expect_context();

//     let Some(room_id) = state.active_room_id.get_untracked() else {
//         return ().into_any();
//     };

//     if let Some(replies_to) = replies_to {
//         if let Some(sender_id) = &replies_to.sender_id {
//             let profile_sig = store.get_profile(&room_id, sender_id);
//             let sig_clone = profile_sig.clone();

//             let text = replies_to.text.clone().unwrap_or_default();

//             view! {
//                 <div class="flex items-center gap-1 ml-[52px] mb-1 cursor-pointer text-xs relative group/reply">
//                     <div class="absolute -left-[32px] top-[calc(50%-1px)] w-[28px] h-4.5 border-l-2 border-t-2 border-white/20 rounded-tl-md"></div>

//                     <div class="shrink-0">{move || profile_sig.get().render_icon(16)}</div>

//                     <span class="font-semibold text-bright hover:underline">
//                         {move || sig_clone.get().render_name(12)}
//                     </span>

//                     <span class="truncate text-bright line-clamp-1">
//                         {text
//                             .into_iter()
//                             .map(|v| v.render(store.clone(), room_id.clone()))
//                             .collect_view()}
//                     </span>
//                 </div>
//             }
//             .into_any()
//         } else {
//             ().into_any()
//         }
//     } else {
//         ().into_any()
//     }
// }

fn render_message_content(
    content: MessageContent,
    store: MemberStore,
    room_id: String,
) -> impl IntoView {
    let spans = content.body;

    match content.msg_type {
        UiMessageType::Audio { source, filename, duration } => {
            view! {
                <div class="flex items-center gap-2 mt-1 p-2 rounded-md bg-white/5 border border-[var(--tile-border-color)] inline-flex">
                    <span class="text-xl">"🎵"</span>
                    <a
                        href=source.url()
                        target="_blank"
                        class="text-blue-400 hover:underline truncate max-w-xs"
                    >
                        {filename.clone()}
                    </a>
                    {duration
                        .map(|d| {
                            let mins = d / 60;
                            let secs = d % 60;
                            view! {
                                <span class="text-xs text-muted">
                                    {format!("{}:{:02}", mins, secs)}
                                </span>
                            }
                                .into_any()
                        })}
                </div>
            }
                .into_any()
        }
        UiMessageType::Emote => view! {
            <div class="text-normal leading-relaxed break-words italic">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}
            </div>
        }
            .into_any(),
        UiMessageType::FailedToDecrypt => view! {
            <div class="text-red-300 bold leading-relaxed break-words text-muted">
                "Encrypted message"
            </div>
        }
            .into_any(),
        UiMessageType::File { filename, size, source, .. } =>  view! {
            <div class="flex items-center gap-2 mt-1 p-2 rounded-md bg-white/5 border border-[var(--tile-border-color)] inline-flex">
                <span class="text-xl">"📄"</span>
                <a
                    href=source.url()
                    target="_blank"
                    class="text-blue-400 hover:underline truncate max-w-xs"
                >
                    {filename.clone()}
                </a>
                <span class="text-xs text-muted">
                    {format!("{:.1} KB", size.unwrap_or_default() as f64 / 1024.0)}
                </span>
            </div>
        }
            .into_any(),
        // Not implemented yet
        UiMessageType::Gallery => view! {
            <div class="text-normal leading-relaxed break-words italic">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}["Gallery message not supported yet"]
            </div>
        }
            .into_any(),
        UiMessageType::Image {
            filename,
            source,
            ..
        } => view! {
            <div class="mt-1">
                <img
                    src=source.url()
                    alt=filename.clone()
                    class="max-w-sm rounded-md border border-[var(--tile-border-color)]"
                />
            </div>
        }
            .into_any(),
        // Not implemented yet
        UiMessageType::Location(_) => view! {
            <div class="text-normal leading-relaxed break-words italic">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}["Location message not supported yet"]
            </div>
        }
            .into_any(),
        // Not implemented yet
        UiMessageType::LiveLocation { .. } => view! {
            <div class="text-normal leading-relaxed break-words italic">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}["Live location sharing not supported yet"]
            </div>
        }
            .into_any(),
        // Not implemented yet
        UiMessageType::Notice => view! {
            <div class="text-normal leading-relaxed break-words italic text-muted">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}["Notice messages are not supported yet"]
            </div>
        }
            .into_any(),
        // Not implemented yet
        UiMessageType::Poll { .. } => view! {
            <div class="text-normal leading-relaxed break-words italic text-muted">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}["Polls are not supported yet"]
            </div>
        }
            .into_any(),
        UiMessageType::Redacted => view! {
            <div class="text-muted italic leading-relaxed break-words flex flex-row items-center gap-1">
                <Icon icon=TRASH size="20px" />
                "This message was deleted"
            </div>
        }
            .into_any(),
        // Not implemented yet
        UiMessageType::ServerNotice { .. } =>  view! {
            <div class="text-normal leading-relaxed break-words italic text-muted">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}["Server notice messages are not supported yet"]
            </div>
        }
            .into_any(),
        UiMessageType::Sticker { source, .. } => view! {
            <div class="mt-1">
                <img
                    src=source.url()
                    class="max-w-sm rounded-md border border-[var(--tile-border-color)]"
                />
            </div>
        }
            .into_any(),
        UiMessageType::Text => view! {
            <div class="text-normal leading-relaxed break-words">
                {spans
                    .clone()
                    .into_iter()
                    .map(|v| v.render(store.clone(), room_id.clone()))
                    .collect_view()}
                {if content.is_edited {
                    view! { <span class="text-xs text-muted ml-2 italic">"(edited)"</span> }
                        .into_any()
                } else {
                    ().into_any()
                }} {spans.clone().into_iter().map(render_link).collect_view()}
            </div>
        }
            .into_any(),
        UiMessageType::Video { source, .. } => view! {
            <div class="mt-1">
                <video
                    src=source.url()
                    controls=true
                    class="max-w-sm rounded-md border border-[var(--tile-border-color)]"
                >
                    {format!(
                        "Your browser does not support the video tag. You can download the video here: {}",
                        source.url(),
                    )}
                </video>
            </div>
        }
            .into_any(),
        UiMessageType::VerificationRequest => view! {
            <div class="text-normal leading-relaxed break-words italic text-muted">
                "Verification request messages are not supported yet"
            </div>
        }
            .into_any()
    }
}

fn render_reactions(
    reactions: Option<HashMap<String, Vec<String>>>,
    store: MemberStore,
    room_id: &str,
    user_id: &str,
) -> impl IntoView {
    let Some(reactions) = reactions else {
        return ().into_any();
    };

    if reactions.is_empty() {
        return ().into_any();
    }

    let content = reactions
        .iter()
        .map(|(emoji, reactors)| {
            let emoji = emoji.clone();
            let store = store.clone();

            let reactor_pics = move || {
                let mut pics = Vec::new();

                let mut all_pics: Vec<(String, _)> = reactors
                    .iter()
                    .filter_map(|user_id| {
                        store
                            .get_profile(room_id, user_id)
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

            let contains_user = reactors.iter().any(|v| v == user_id);

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

fn get_date_from_ts(ts: i64) -> DateTime<Local> {
    Local
        .timestamp_opt(ts, 0)
        .latest()
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.with_timezone(&Local))
}

fn render_system_message(
    content: SystemMessage,
    _store: MemberStore,
    _room_id: String,
) -> impl IntoView {
    let text = match content {
        SystemMessage::MembershipChange { user_id, change } => format!(
            "{} {}",
            user_id,
            change
                .map(|v| v.display_string())
                .unwrap_or_else(|| "changed membership".to_string()),
        ),
        SystemMessage::ProfileChange {
            user_id,
            display_name_change,
            avatar_url_changed,
        } => {
            let mut changes = Vec::new();

            if let Some(Change { old, new }) = display_name_change {
                if let Some(new) = new {
                    if let Some(old) = old {
                        changes.push(format!(
                            "changed their display name from '{}' to '{}'",
                            old, new
                        ));
                    } else {
                        changes.push(format!("set their display name to '{}'", new));
                    }
                } else {
                    changes.push("removed their display name".to_string());
                }
            }

            if let Some(Change { old, new }) = avatar_url_changed {
                if new.is_some() && old.is_none() {
                    changes.push("set a profile picture".to_string());
                } else if new.is_none() && old.is_some() {
                    changes.push("removed their profile picture".to_string());
                } else {
                    changes.push("changed their profile picture".to_string());
                }
            }

            format!("{} {}", user_id, changes.join(" and "))
        }
        SystemMessage::RtcNotification {
            call_intent,
            declined_by,
        } => format!(
            "{} Call declined by {})",
            call_intent.map(|v| v.to_string()).unwrap_or("".to_string()),
            declined_by.join(", ")
        ),
        SystemMessage::OtherEvent => "[unsupported message]".to_string(),
        SystemMessage::CallInvite => "Call started".to_string(),
    };

    view! {
        <div class="flex items-center justify-center my-2">
            <span class="text-muted text-xxl bdf-text">{text}</span>
        </div>
    }
}

fn render_timeline_event(
    store: MemberStore,
    room_id: &str,
    own_user_id: &str,
    event: TimelineEvent,
    show_header: bool,
) -> impl IntoView {
    let hovered = RwSignal::new(false);
    let show_highlight = event.flags.is_highlighted;

    let failed_message = event.get_failed_message();
    let pending = event.is_sending();

    let date = get_date_from_ts(event.timestamp as i64);
    let reactions = event.get_reactions();

    let sender_id = event.get_sender_id();
    let name = event
        .get_sender_name()
        .unwrap_or(sender_id.clone().unwrap_or("Unknown".to_string()));
    let avatar_url = event.get_sender_avatar_url();

    let color = sender_id
        .map(get_color)
        .unwrap_or(Hsl::new(0.0, 0.0, 70.0, None));

    let content = match event.content{
        EventContent::MsgLike(ev) => render_message_content(*ev, store.clone(), room_id.to_string()),
        EventContent::FailedToParseMessageLike { event_type, error } => return view! { <div class="text-red-500 italic">{format!("Failed to render {event_type}: {error}")}</div> }.into_any(),
        EventContent::FailedToParseState { event_type, state_key, error } => return view! {
            <div class="text-red-500 italic">
                {format!("Failed to render {event_type} with state key {state_key}: {error}")}
            </div>
        }.into_any(),
        EventContent::SystemMessage(ev) => return render_system_message(ev, store, room_id.to_string()).into_any(),
    };

    view! {
        <div
            class="group/msg relative flex flex-col gap-[var(--gap)] hover:bg-black/20 ml-1 pl-4 py-[2px] rounded-md"
            class=("mt-5", show_header)
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
                ().into_any()
            }}

            {move || {
                if hovered.get() && !show_header {
                    view! {
                        <div class="absolute text-xs text-muted mt-[5px] ml-[5px]">
                            {date.format("%H:%M").to_string()}
                        </div>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}

            // <ReplyPreview replies_to=reply_data.clone() />

            <div class="flex gap-[var(--gap)]">
                <div class="shrink-0 mr-2 w-[40px] mt-[5px]">
                    {if show_header {
                        render_profile_icon(avatar_url, name.clone(), 40, color.clone()).into_any()
                    } else {
                        ().into_any()
                    }}
                </div>

                <div class="flex flex-col min-w-0 flex-1">
                    {if show_header {
                        view! {
                            <div class="flex items-baseline gap-2">
                                <span class="text-bright truncate cursor-pointer">
                                    {render_profile_name(name, color, 16)}
                                </span>
                                <span class="text-muted text-xs">{format_date(date)}</span>
                            </div>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }} <div>
                        <div class=("opacity-50", pending)>{content}</div>
                        {failed_message
                            .map(|msg| {
                                view! {
                                    <div class="flex items-center gap-1 mt-1 text-red-500 text-xs">
                                        <Icon
                                            icon=WARNING_CIRCLE
                                            weight=IconWeight::Duotone
                                            size="16px"
                                        />
                                        "Failed to send: "
                                        {msg}
                                    </div>
                                }
                            })}
                        {render_reactions(reactions, store, room_id, own_user_id)}
                    </div>
                </div>
            </div>
        </div>
    }.into_any()
}

fn render_timeline_item(item: UiTimelineItem, show_header: bool) -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let Some(room_id) = state.active_room_id.get_untracked() else {
        return ().into_any();
    };

    let user_id = state.user_id.get_untracked();

    match item.kind {
        UiTimelineItemKind::DateDivider(date)=> {
            let date = get_date_from_ts(date as i64);

            let is_today = date.date_naive() == Local::now().date_naive();
            let is_yesterday = date.date_naive()
                == (Local::now().date_naive() - chrono::Duration::days(1));

            let label = if is_today {
                "Today".to_string()
            } else if is_yesterday {
                "Yesterday".to_string()
            } else {
                date.format("%d %B %Y").to_string()
            };

            view! {
                <div class="flex items-center gap-2 mt-4 drop-shadow">
                    <div class="flex-1 border-t-1 border-[var(--muted-text-color)] bdf"></div>
                    <span class="text-muted text-sm bdf-text">{label}</span>
                    <div class="flex-1 border-t-1 border-[var(--muted-text-color)] bdf"></div>
                </div>
            }
        }
        .into_any(),
        UiTimelineItemKind::ReadMarker => view! {
            <div class="flex items-center w-full pr-4">
                <div class="flex-1 border-2 border-[#00ffff] rounded-full"></div>

                <span class="relative flex items-center h-[20px] bg-[#00ffff] text-[var(--bg-color)] text-[10px] font-bold px-2 rounded-r-[3px] ml-1 uppercase tracking-wider select-none">
                    <div class="absolute -left-[6px] top-0 w-0 h-0 border-y-[10px] border-y-transparent border-r-[6px] border-r-[#00ffff]"></div>
                    "New"
                </span>
            </div>
        }
        .into_any(),
        UiTimelineItemKind::TimelineStart => view! {
            <div class="flex items-center gap-2 my-4 drop-shadow">
                <div class="flex-1 border-t-1 border-[var(--muted-text-color)] bdf"></div>
                <span class="text-muted text-sm bdf-text">"Start of Timeline"</span>
                <div class="flex-1 border-t-1 border-[var(--muted-text-color)] bdf"></div>
            </div>
        }.into_any(),
        UiTimelineItemKind::Event(event) => render_timeline_event(store, &room_id, &user_id, *event, show_header).into_any()
    }
}

#[component]
fn TimeLine() -> impl IntoView {
    let state: AppState = expect_context();

    let messages_update_event: ReadSignal<Option<Vec<UiTimelineDiff>>> =
        use_tauri_event("timeline_update");

    let messages: RwSignal<Vec<UiTimelineItem>> = RwSignal::new(Vec::new());

    Effect::new(move |_| {
        let Some(diffs) = messages_update_event.get() else {
            return;
        };

        messages.update(|msgs| {
            for diff in diffs {
                match diff {
                    UiTimelineDiff::Append { values } => {
                        msgs.extend(values);
                    }
                    UiTimelineDiff::Set { index, value } => {
                        if let Some(item) = msgs.get_mut(index) {
                            *item = value;
                        }
                    }
                    UiTimelineDiff::PushBack { value } => msgs.push(value),
                    UiTimelineDiff::Remove { index } => {
                        if index < msgs.len() {
                            msgs.remove(index);
                        }
                    }
                    UiTimelineDiff::Clear => msgs.clear(),
                    UiTimelineDiff::Insert { index, value } => {
                        if index <= msgs.len() {
                            msgs.insert(index, value);
                        }
                    }
                    UiTimelineDiff::PopBack => {
                        msgs.pop();
                    }
                    UiTimelineDiff::PopFront => {
                        if !msgs.is_empty() {
                            msgs.remove(0);
                        }
                    }
                    UiTimelineDiff::PushFront { value } => msgs.insert(0, value),
                    UiTimelineDiff::Reset { values } => {
                        msgs.clear();
                        msgs.extend(values);
                    }
                    UiTimelineDiff::Truncate { length } => msgs.truncate(length),
                }
            }
        });
    });

    let timeline = Memo::new(move |_| {
        let msgs = messages.get();
        let mut processed_items = Vec::with_capacity(msgs.len());

        let mut last_sender: Option<String> = None;
        let mut last_timestamp: Option<u64> = None;

        for (idx, item) in msgs.iter().enumerate() {
            let mut show_header = true;

            // Check if the previous chronological item was a divider
            if idx > 0
                && let UiTimelineItemKind::DateDivider(_) | UiTimelineItemKind::ReadMarker =
                    msgs[idx - 1].kind
            {
                show_header = true;
                if let UiTimelineItemKind::Event(event) = &item.kind
                    && let DetailState::Ready(sender) = &event.sender
                {
                    last_sender = Some(sender.id.to_string());
                    last_timestamp = Some(event.timestamp);
                }
            } else if let UiTimelineItemKind::Event(event) = &item.kind {
                if let EventContent::MsgLike(msg) = &event.content
                    && msg.in_reply_to.is_some()
                {
                    show_header = true;
                } else if let DetailState::Ready(sender) = &event.sender {
                    if let Some(last_sender_id) = &last_sender
                        && last_sender_id == &sender.id
                        && let Some(last_ts) = last_timestamp
                    {
                        // If this message is within 5 minutes of the previous message, hide header
                        if event.timestamp.saturating_sub(last_ts) < 5 * 60 {
                            show_header = false;
                        }
                    }
                    last_sender = Some(sender.id.to_string());
                    last_timestamp = Some(event.timestamp);
                } else {
                    show_header = true;
                    last_sender = None;
                    last_timestamp = None;
                }
            } else {
                show_header = true;
                last_sender = None;
                last_timestamp = None;
            }

            processed_items.push((item.clone(), show_header));
        }

        processed_items.reverse();
        processed_items
    });

    let is_loading = RwSignal::new(false);
    let has_more = RwSignal::new(true);
    let initial_loaded = RwSignal::new(false);

    let sentinel_ref = NodeRef::<Div>::new();

    let fetch_more = move || {
        let Some(room_id) = state.active_room_id.get_untracked() else {
            log::error!("No active room ID, cannot fetch more messages");
            return;
        };

        is_loading.set(true);
        spawn_local(async move {
            match scroll_up(&room_id).await {
                Ok(new_has_more) => {
                    log::info!("Fetched more messages");
                    has_more.set(new_has_more);
                    if !new_has_more {
                        log::info!("No more messages to load");
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch more messages: {}", e);
                }
            };
            is_loading.set(false);
        });
    };

    let UseIntersectionObserverReturn { .. } = use_intersection_observer(
        sentinel_ref,
        move |entries: Vec<IntersectionObserverEntry>, _| {
            if entries[0].is_intersecting()
                && initial_loaded.get()
                && has_more.get()
                && !is_loading.get()
            {
                fetch_more();
            };
        },
    );

    Effect::new(move |_| {
        if let Some(room_id) = state.active_room_id.get() {
            log::info!(
                "Effect: Loading room {}, resetting messages to empty",
                room_id
            );
            messages.set(Vec::new());
            initial_loaded.set(false);
            has_more.set(true);
            is_loading.set(true);

            let current_room_id = room_id.clone();

            spawn_local(async move {
                match get_timeline(&current_room_id).await {
                    Ok(tl) => {
                        if state.active_room_id.get_untracked() == Some(current_room_id.clone()) {
                            log::info!("Effect: Received {} items", tl.len());
                            messages.set(tl);
                            initial_loaded.set(true);
                            is_loading.set(false);
                        }
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("Cancelled by newer request") {
                            log::debug!(
                                "Frontend ignored cancelled request for room {}",
                                current_room_id
                            );
                        } else {
                            log::error!("Failed to load timeline: {}", e);
                            if state.active_room_id.get_untracked() == Some(current_room_id) {
                                is_loading.set(false);
                            }
                        }
                    }
                }
            });
        }
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse py-2 overflow-anchor-auto">
            <div class="mb-5"></div>
            <For
                each=move || timeline.get()
                key=|(item, _)| item.render_key
                children=|(item, show_header)| render_timeline_item(item.clone(), show_header)
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
                                        ().into_any()
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
            <div class="self-center h-full">
                <button
                    class="transition-opacity h-full mr-1"
                    class=("text-(--ui-hover-color)", move || info_hovered.get())
                    class=("text-(--ui-base-color)", move || !info_hovered.get())
                    on:click=move |_| log::info!("John Pork is calling...")
                    // on:mouseenter=move |_| set_info_hovered.set(true)
                    // on:mouseleave=move |_| set_info_hovered.set(false)
                >
                    <div class="h-full justify-center items-center flex cursor-pointer">
                        <Icon
                            icon=PHONE
                            size="80%"
                            color="currentColor"
                            weight=IconWeight::Duotone
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

    let mention_matches = RwSignal::new(Vec::<UserProfile>::new());
    let command_matches = RwSignal::new(Vec::<Command>::new());

    provide_context(mention_matches);
    provide_context(command_matches);

    let input_ref = NodeRef::<Div>::new();

    let _ = use_event_listener(document(), ev::selectionchange, move |_| {
        let Some(el) = input_ref.get() else {
            return;
        };

        let win = window();
        if let Ok(Some(sel)) = win.get_selection()
            && let Some(focus_node) = sel.focus_node()
        {
            if el.contains(Some(&focus_node)) {
                let caret_pos = get_caret_position(&el);

                if let Some(filter) = get_active_filter(&el, caret_pos, '@') {
                    menu.set(MenuType::UserAutocomplete { filter });
                    return;
                }
                if let Some(filter) = get_active_filter(&el, caret_pos, '/') {
                    menu.set(MenuType::CommandAutocomplete { filter });
                } else {
                    menu.set(MenuType::None);
                }
            } else {
                if menu.get_untracked() != MenuType::None {
                    menu.set(MenuType::None);
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

        if let Ok(Some(sel)) = win.get_selection()
            && let Ok(range) = doc.create_range()
        {
            let _ = range.select_node_contents(&el);
            // collapse_with_to_start(false) collapses the selection to the END
            range.collapse_with_to_start(false);

            let _ = sel.remove_all_ranges();
            let _ = sel.add_range(&range);
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
                                        ().into_any()
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
                                                ().into_any()
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
                    ().into_any()
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
                                                ().into_any()
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
                    ().into_any()
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
