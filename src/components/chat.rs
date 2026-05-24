use crate::{
    components::{
        FloatingTile, TextCircle, TextCircleProps,
        input::{
            get_active_filter, get_caret_position, handle_input, handle_keydown,
            menu::{MenuType, SelectionMenu}, move_caret_to_end,
        },
        presence::PresenceBadge,
        previews::render_link,
        text::{RichTextExt, richt_text_spans_to_html},
        user_profile::{
            UserProfileExt, UserProfileMaybeExt, render_profile_icon, render_profile_name,
        },
    },
    hooks::use_tauri_event,
    state::{AppState, MemberProfileHandle, MemberStore, RoomHeader},
    tauri_functions::{get_members_for_room, get_timeline, scroll_up, toggle_reaction},
};

use colorsys::Hsl;
use phosphor_leptos::{ARROW_BEND_UP_LEFT, HASH, INFO, Icon, IconWeight, PENCIL_SIMPLE, PHONE, SMILEY, TRASH, UPLOAD_SIMPLE, WARNING_CIRCLE, X_CIRCLE};

use chrono::{DateTime, Local, TimeZone};
use leptos::{ev, html::Div, prelude::*, task::spawn_local};
use leptos_use::{UseIntersectionObserverReturn, use_event_listener, use_intersection_observer};
use shared::{
    get_color,
    timeline::{
        DetailState, EventContent, MessageContent, ReactionInfo, ReplyInfo, RichTextSpan, SystemMessage, UiMessageType, UiTimelineDiff, UiTimelineItem, UiTimelineItemKind
    },
    user_profile::PresenceStatus,
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

#[component]
fn ReplyPreview(reply_info: Option<ReplyInfo>) -> impl IntoView {
    let Some(reply_info) = reply_info else {
        return ().into_any();
    };

    let store: MemberStore = expect_context();
    let state: AppState = expect_context();

    log::debug!("Rendering reply: {:?}", reply_info);

    let mut content = vec![RichTextSpan::Plain("click to go to event".to_string())];

    let (sender_name, avatar_url, color) = match reply_info.event {
        DetailState::Error(e) => (format!("! {e}"), None, Hsl::new(0.0, 0.0, 70.0, None)),
        DetailState::Pending => (
            "Loading...".to_string(),
            None,
            Hsl::new(0.0, 0.0, 70.0, None),
        ),
        DetailState::Ready(preview) => {
            let sender = preview.sender;

            content = preview.content;

            (sender.display_name(), sender.avatar_url(), sender.color())
        }
        DetailState::Unavailable => (
            "Event not found".to_string(),
            None,
            Hsl::new(0.0, 0.0, 70.0, None),
        ),
    };

    view! {
        <div class="flex items-center gap-1 ml-[52px] mb-1 cursor-pointer text-xs relative group/reply cursor-pointer">
            <div class="absolute -left-[32px] top-[calc(50%-1px)] w-[28px] h-4.5 border-l-2 border-t-2 border-white/20 rounded-tl-md"></div>

            <div class="shrink-0">
                {render_profile_icon(avatar_url, sender_name.clone(), 16, color.clone())}
            </div>

            <span class="font-semibold text-bright hover:underline">
                {render_profile_name(sender_name, color, 12)}
            </span>

            <span class="truncate text-bright line-clamp-1">
                {content
                    .into_iter()
                    .map(|v| {
                        v.render(
                            store.clone(),
                            state.active_room_id.get_untracked().unwrap_or_default(),
                        )
                    })
                    .collect_view()}
            </span>
        </div>
    }
    .into_any()
}

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
    reactions: Option<HashMap<String, Vec<ReactionInfo>>>,
    store: MemberStore,
    room_id: String,
    user_id: String,
    event_id: Option<String>,
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

            let btn_room_id = room_id.clone();
            let btn_event_id = event_id.clone();

            let prof_room_id = room_id.clone();

            let contains_user = reactors.iter().any(|v| *v.sendere_id == user_id);

            let reactor_pics = move || {
                let mut pics = Vec::new();

                let all_pics: Vec<(String, _)> = reactors
                    .iter()
                    .filter_map(|info| {
                        store
                            .get_profile(&prof_room_id, &info.sendere_id)
                            .get()
                            .map(|p| {
                                let icon = p.clone().render_icon(20);

                                let wrapped = view! {
                                    <div
                                        class="rounded-full ring-2 shrink-0 flex items-center justify-center transition-shadow"
                                        class=("hover:ring-(--ui-solid-hover-bg)", !contains_user)
                                        class=("ring-(--ui-solid-bg)", !contains_user)
                                        class=("ring-(--accent-bg-color)", contains_user)
                                        class=(
                                            "group-hover:ring-(--ui-solid-hover-bg)",
                                            !contains_user,
                                        )
                                    >
                                        {icon}
                                    </div>
                                };
                                (p.get_name(), wrapped.into_any())
                            })
                    })
                    .collect();

                let len = all_pics.len();
                pics.extend(all_pics.into_iter().map(|(_, pic)| pic).take(4));

                if len > 4 {
                    pics.push(
                        TextCircle(TextCircleProps::builder().text(format!("+{}", len - 4)).class("-ml-1.5 first:ml-0 w-[30px] h-[20px] rounded-full").color(Hsl::new(0.0, 0.0, 60.0, None)).build()).into_any()
                    );
                }

                pics.collect_view()
            };

            let contains_user = reactors.iter().any(|v| *v.sendere_id == user_id);

            view! {
                <button
                    class="flex items-center p-0.5 pr-1 rounded-lg border cursor-pointer transition-colors select-none group"
                    class=("bg-(--ui-solid-bg)", !contains_user)
                    class=("hover:bg-(--ui-solid-hover-bg)", !contains_user)
                    class=("border-(--tile-border-color)", !contains_user)
                    class=("bg-(--accent-bg-color)", contains_user)
                    class=("border-(--accent-color)", contains_user)
                    on:click=move |_| {
                        let Some(e_id) = btn_event_id.clone() else {
                            return;
                        };
                        let r_id = btn_room_id.clone();
                        let async_emoji = emoji.clone();
                        leptos::task::spawn_local(async move {
                            let _ = toggle_reaction(&r_id, &e_id, &async_emoji)
                                .await
                                .map_err(|e| {
                                    log::error!("Failed to toggle reaction: {}", e);
                                });
                        });
                    }
                >
                    <span class="text-sm leading-none pl-1">{emoji.clone()}</span>

                    <span
                        class="pl-1 pr-1.5 font-bold text-sm min-w-[2ch] tabular-nums text-center -space-x-1.5"
                        class=("text-bright", contains_user)
                        class=("text-dim", !contains_user)
                    >
                        {reactors.len()}
                    </span>

                    <div class="flex flex-row items-center pl-0.5 -space-x-2.5">
                        {reactor_pics()}
                    </div>
                </button>
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
        SystemMessage::ProfileChange(change) => change.display_string(),
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
    item_sig: RwSignal<UiTimelineItem>,
    show_header: bool,
) -> impl IntoView {
    let hovered = RwSignal::new(false);

    let (show_highlight, date, sender_id, name, avatar_url, color, reply_info, event_id) = item_sig.with_untracked(|item| {
        if let UiTimelineItemKind::Event(event) = &item.kind {
            let sender_id = event.get_sender_id();
            let name = event.get_sender_name().unwrap_or(sender_id.clone().unwrap_or("Unknown".to_string()));
            (
                event.flags.is_highlighted,
                get_date_from_ts(event.timestamp as i64),
                sender_id,
                name,
                event.get_sender_avatar_url(),
                event.get_sender_id().map(|v| get_color(&v)).unwrap_or(Hsl::new(0.0, 0.0, 70.0, None)),
                event.in_reply_to(),
                event.event_id.clone(),
            )
        } else {
            unreachable!("Must be an event")
        }
    });

    let item_id = item_sig.get_untracked().id.clone();

    let room_id_for_content = room_id.to_string();
    let store_for_content = store.clone();

    let rendered_content = move || {
        item_sig.with(|item| {
            if let UiTimelineItemKind::Event(event) = &item.kind {
                match &event.content {
                    EventContent::MsgLike(ev) => render_message_content(
                        *ev.clone(),
                        store_for_content.clone(),
                        room_id_for_content.clone(),
                    ).into_any(),
                    EventContent::FailedToParseMessageLike { event_type, error } => view! { <div class="text-red-500 italic">{format!("Failed to render {event_type}: {error}")}</div> }.into_any(),
                    EventContent::FailedToParseState { event_type, state_key, error } => view! {
                        <div class="text-red-500 italic">
                            {format!(
                                "Failed to render {event_type} with state key {state_key}: {error}",
                            )}
                        </div>
                    }.into_any(),
                    EventContent::SystemMessage(ev) => render_system_message(
                        ev.clone(),
                        store_for_content.clone(),
                        room_id_for_content.clone()
                    ).into_any(),
                }
            } else {
                ().into_any()
            }
        })
    };

    let store_clone = store.clone();
    let event_id_clone = event_id.clone();
    let room_id = room_id.to_string();
    let own_user_id = own_user_id.to_string();
    let edit_room_id = room_id.clone();
    let flags_own_user_id = own_user_id.clone();

    let reactions_view = move || {
        render_reactions(
            item_sig.with(|i| {
                if let UiTimelineItemKind::Event(e) = &i.kind {
                    e.get_reactions()
                } else {
                    None
                }
            }),
            store_clone.clone(),
            room_id.clone(),
            own_user_id.clone(),
            event_id_clone.clone(),
        )
    };

    let input_info: RwSignal<Option<ChatInputInfo>> = expect_context();
    let input_ref: NodeRef<Div> = expect_context();

    let current_highlight = Memo::new({
        let item_id = item_id.clone();
        move |_| {
            match input_info.get() {
                Some(ChatInputInfo::ReplyingTo { item_id: reply_id, .. })
                    if *reply_id == item_id => return Some("white".to_string()),
                Some(ChatInputInfo::Editing { item_id: edit_id, .. })
                    if *edit_id == item_id => return Some("white".to_string()),
                _ => (),
            }
            if show_highlight {
                return Some("var(--accent-color)".to_string());
            }
            None
        }
    });

    let edit_event_id = event_id.clone();
    let edit_item_id = item_id.clone();
    let edit_store = store.clone();

    let is_empty: RwSignal<bool> = expect_context();

    let flags_sender_id = sender_id.clone().unwrap_or_default();

    let flags = Memo::new(move |_| {
        let item = item_sig.get();

        let mut flags = item.flags();
        let is_own_message = flags_sender_id == flags_own_user_id;

        flags.is_editable = flags.is_editable && is_own_message;

        flags
    });

    view! {
        <div
            class="group/msg relative flex flex-col gap-[var(--gap)] hover:bg-black/20 ml-1 pl-4 py-[2px] rounded-md"
            class=("mt-5", show_header)
            style:background=move || {
                let hovered = hovered.get();
                if let Some(color) = current_highlight.get() {
                    format!(
                        "linear-gradient(in oklch to right, oklch(from {color} l c h / {}) 20%, oklch(from {color} l c h / 0) 100%)",
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
            {move || {
                if let Some(color) = current_highlight.get() {
                    view! {
                        <div
                            class="absolute left-1 top-1 bottom-1 w-1 rounded-full pointer-events-none"
                            style=format!("background-color: {color}")
                        ></div>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
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

            <div class="absolute -top-4 right-4 flex items-center gap-1 bg-(--ui-solid-bg) p-1 rounded-(--gap) text-muted text-xs border border-(--tile-border-color) opacity-0 group-hover/msg:opacity-100">
                <button class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-normal">
                    <Icon icon=SMILEY size="20px"></Icon>
                </button>
                <Show when=move || {
                    flags.get().can_be_replied_to
                }>
                    {
                        let reply_event_id = event_id.clone();
                        let sender_id = sender_id.clone();
                        let item_id = item_id.clone();
                        let input_info = input_info;
                        let input_ref = input_ref;

                        view! {
                            <button
                                class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-normal"
                                on:click=move |_| {
                                    let Some(event_id) = reply_event_id.clone() else {
                                        return;
                                    };
                                    let Some(sender_id) = sender_id.clone() else {
                                        return;
                                    };
                                    input_info
                                        .set(
                                            Some(ChatInputInfo::ReplyingTo {
                                                event_id,
                                                sender_id,
                                                item_id: item_id.clone(),
                                            }),
                                        );
                                    if let Some(el) = input_ref.get() {
                                        el.focus().ok();
                                    }
                                }
                            >
                                <Icon icon=ARROW_BEND_UP_LEFT size="20px"></Icon>
                            </button>
                        }
                    }
                </Show>
                <Show when=move || {
                    flags.get().is_editable
                }>
                    {
                        let event_id = edit_event_id.clone();
                        let item_id = edit_item_id.clone();
                        let store = edit_store.clone();
                        let room_id = edit_room_id.clone();

                        view! {
                            <button
                                class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-normal"
                                on:click=move |_| {
                                    let Some(event_id) = event_id.clone() else {
                                        return;
                                    };
                                    input_info
                                        .set(
                                            Some(ChatInputInfo::Editing {
                                                event_id,
                                                item_id: item_id.clone(),
                                            }),
                                        );
                                    if let Some(el) = input_ref.get() {
                                        el.focus().ok();
                                        let spans = item_sig.get_untracked().body();
                                        el.set_inner_html(
                                            &richt_text_spans_to_html(
                                                &spans,
                                                store.clone(),
                                                room_id.clone(),
                                            ),
                                        );
                                        is_empty.set(false);
                                        move_caret_to_end(&el);
                                    }
                                }
                            >
                                <Icon icon=PENCIL_SIMPLE size="20px"></Icon>
                            </button>
                        }
                    }
                </Show>
            </div>

            <ReplyPreview reply_info=reply_info />

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
                        <div class=(
                            "opacity-50",
                            move || {
                                item_sig
                                    .with(|i| {
                                        if let UiTimelineItemKind::Event(e) = &i.kind {
                                            e.is_sending()
                                        } else {
                                            false
                                        }
                                    })
                            },
                        )>{rendered_content}</div>
                        {move || {
                            let failed = item_sig
                                .with(|i| {
                                    if let UiTimelineItemKind::Event(e) = &i.kind {
                                        e.get_failed_message()
                                    } else {
                                        None
                                    }
                                });
                            failed
                                .map(|msg| {
                                    view! {
                                        <div class="flex items-center gap-1 mt-1 text-red-500 text-xs">
                                            <Icon
                                                icon=WARNING_CIRCLE
                                                weight=IconWeight::Duotone
                                                size="16px"
                                            />
                                            {msg}
                                        </div>
                                    }
                                })
                        }}
                        {reactions_view}
                    </div>
                </div>
            </div>
        </div>
    }.into_any()
}

fn render_timeline_item(item_sig: RwSignal<UiTimelineItem>, show_header: bool) -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let Some(room_id) = state.active_room_id.get_untracked() else {
        return ().into_any();
    };

    let user_id = state.user_id.get_untracked();

    let kind = item_sig.with_untracked(|i| i.kind.clone());

    match kind {
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
        UiTimelineItemKind::Event(_) => render_timeline_event(store, &room_id, &user_id, item_sig, show_header).into_any()
    }
}

#[component]
fn TimeLine() -> impl IntoView {
    let state: AppState = expect_context();

    let messages_update_event: ReadSignal<Option<Vec<UiTimelineDiff>>> =
        use_tauri_event("timeline_update");

    let messages: RwSignal<Vec<RwSignal<UiTimelineItem>>> = RwSignal::new(Vec::new());

    Effect::new(move |_| {
        let Some(diffs) = messages_update_event.get() else {
            return;
        };

        messages.update(|msgs| {
            for diff in diffs {
                match diff {
                    UiTimelineDiff::Append { values } => {
                        let extention: Vec<RwSignal<UiTimelineItem>> = values
                            .iter()
                            .map(|v| RwSignal::new(v.clone()))
                            .collect();
                        msgs.extend(extention);
                    }
                    UiTimelineDiff::Set { index, value } => {
                        if let Some(item) = msgs.get_mut(index) {
                            item.set(value);
                        }
                    }
                    UiTimelineDiff::PushBack { value } => msgs.push(RwSignal::new(value)),
                    UiTimelineDiff::Remove { index } => {
                        if index < msgs.len() {
                            msgs.remove(index);
                        }
                    }
                    UiTimelineDiff::Clear => msgs.clear(),
                    UiTimelineDiff::Insert { index, value } => {
                        if index <= msgs.len() {
                            msgs.insert(index, RwSignal::new(value));
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
                    UiTimelineDiff::PushFront { value } => msgs.insert(0, RwSignal::new(value)),
                    UiTimelineDiff::Reset { values } => {
                        msgs.clear();

                        let extention: Vec<RwSignal<UiTimelineItem>> = values
                            .iter()
                            .map(|v| RwSignal::new(v.clone()))
                            .collect();
                        msgs.extend(extention);
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

        for (idx, item_sig) in msgs.clone().into_iter().enumerate() {
            let mut show_header = true;

            let Some(item) = item_sig.try_get_untracked() else {
                continue;
            };

            let is_event = if let UiTimelineItemKind::Event(ev) = &item.kind {
                matches!(ev.content, EventContent::MsgLike(_))
            } else {
                false
            };

            if !is_event {
                processed_items.push((item_sig, false));
                continue
            }

            let prev_was_divider = if idx > 0 {
                if let Some(prev_item) = msgs[idx - 1].try_get_untracked() {
                    matches!(
                        prev_item.kind,
                        UiTimelineItemKind::DateDivider(_) | UiTimelineItemKind::ReadMarker
                    )
                } else {
                    false
                }
            } else {
                false
            };

            // Check if the previous chronological item was a divider
            if prev_was_divider {
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

            processed_items.push((item_sig, show_header));
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
                    log::debug!("Fetched more messages");
                    has_more.set(new_has_more);
                    if !new_has_more {
                        log::debug!("No more messages to load");
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
            log::debug!(
                "Loading room {}, resetting messages to empty",
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
                            messages.set(tl.into_iter().map(RwSignal::new).collect());
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
                key=|(item_sig, _)| {
                    item_sig
                        .try_with_untracked(|item| { item.id.clone() })
                        .unwrap_or_else(|| "disposed_fallback_key".to_string())
                }
                children=|(item_sig, show_header)| { render_timeline_item(item_sig, show_header) }
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
                >
                    // on:mouseenter=move |_| set_info_hovered.set(true)
                    // on:mouseleave=move |_| set_info_hovered.set(false)
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

#[derive(Clone, Debug)]
pub enum ChatInputInfo {
    ReplyingTo { event_id: String, sender_id: String, item_id: String },
    Editing { event_id: String, item_id: String },
}

#[component]
fn ChatInput() -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let menu = RwSignal::new(MenuType::None);
    let selected_index = RwSignal::new(0);
    let input_info = RwSignal::new(None);

    provide_context(selected_index);
    provide_context(input_info);

    let mention_matches = RwSignal::new(Vec::new());
    let command_matches = RwSignal::new(Vec::new());

    provide_context(mention_matches);
    provide_context(command_matches);

    let input_ref: NodeRef<Div> = NodeRef::new();

    provide_context(input_ref);

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
    provide_context(is_empty);

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

    let store_clone = store.clone();

    let input_info_content = move || {
        let Some(info) = input_info.get() else {
            return ().into_any();
        };

        let content = match info {
            ChatInputInfo::ReplyingTo { sender_id, .. } => {
                let profile = store_clone.get_profile(&state.active_room_id.get().unwrap_or_default(), &sender_id);
                view! {
                    <span class="text-sm text-bright">
                        "Replying to " {move || profile.get().render_name(14)}
                    </span>
                }.into_any()
            }
            ChatInputInfo::Editing { .. } => view! { <span class="text-sm text-bright">"Editing message"</span> }.into_any(),
        };

        view! {
            <div class="w-full bg-(--ui-floating-bg) rounded-t-(--ui-border-radius) border border-(--tile-border-color) border-b-0 px-2 py-1 flex flex-row justify-between items-center">
                {content}
                <button
                    class="text-muted hover:text-bright cursor-pointer"
                    on:click=move |_| input_info.set(None)
                >
                    <Icon icon=X_CIRCLE size="18px" color="currentColor" weight=IconWeight::Fill />
                </button>
            </div>
        }.into_any()
    };

    view! {
        <div class="p-2 pt-0 w-full relative">
            {move || input_info_content()} <SelectionMenu menu=menu input_ref=input_ref />
            <div
                class="text-(--bright-text-color) w-full min-h-13 border-1 border-(--tile-border-color) rounded-b-(--ui-border-radius) bg-[rgba(0, 0, 0, 0.6)] flex flex-row bg-(--ui-floating-bg) items-center gap-3 px-3 cursor-text"
                class=("rounded-t-(--ui-border-radius)", move || input_info.get().is_none())
            >
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
                            input_info,
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
                RoomHeader::Channel(..) => view! { <MemberList /> }.into_any(),
                _ => view! { <div class="px-4 py-4">"..."</div> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn MemberList() -> impl IntoView {
    let state: AppState = expect_context();
    let store: MemberStore = expect_context();

    let members = LocalResource::new(move || {
        let store = store.clone();

        async move {
            let Some(room_id) = state.active_room_id.get() else {
                return (Vec::new(), Vec::new());
            };

            let mut online = Vec::new();
            let mut offline = Vec::new();

            let Ok(members) = get_members_for_room(&room_id).await else {
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
