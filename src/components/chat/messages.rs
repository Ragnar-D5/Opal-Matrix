use csscolorparser::Color;
use std::collections::HashMap;

use chrono::{DateTime, Local, TimeZone};
use leptos::{html::Div, portal::Portal, prelude::*, task::spawn_local};
use phosphor_leptos::{
    ARROW_BEND_UP_LEFT, ARROW_RIGHT, HASH, Icon, IconWeight, PENCIL_SIMPLE, SMILEY, SPEAKER_HIGH,
    TRASH, WARNING_CIRCLE,
};
use shared::{
    profile::MemberProfile,
    sidebar::RoomKind,
    timeline::{
        DetailState, EventContent, EventFlags, MessageContent, ReactionInfo, ReplyInfo,
        RichTextSpan, SystemMessage, UiCallIntent, UiMembershipChange, UiMessageType,
        UiTimelineItem, UiTimelineItemKind,
    },
};
use wasm_bindgen::JsCast;
use web_sys::Element;

use crate::{
    app::{Settings, format_date},
    components::{
        CloseButton, TextCircle, TextCircleProps,
        blurhash::Blurhash,
        chat::{Attachment, ChatInputInfo},
        input::move_caret_to_end,
        overlays::emoji_picker::{EmojiPickerState, pick_emoji},
        previews::render_link,
        text::{RichTextExt, richt_text_spans_to_html},
        user_profile::MemberProfileExt,
    },
    state::{AppState, LighboxImage, ProfileStore},
    tauri_functions::{delete_message, toggle_reaction},
};

#[component]
fn ReplyPreview(
    reply_info: Option<ReplyInfo>,
    active_room_id: String,
    scroll_to_item: Callback<String>,
) -> impl IntoView {
    let Some(reply_info) = reply_info else {
        return ().into_any();
    };

    let store: ProfileStore = expect_context();
    let target_event_id = reply_info.event_id.clone();

    let preview = Memo::new(move |_| match &reply_info.event {
        DetailState::Error(e) => {
            log::error!("Failed to load event for reply preview: {e}");
            None
        }
        DetailState::Pending => None,
        DetailState::Ready(preview) => Some(preview.clone()),
        DetailState::Unavailable => None,
    });

    let profile_store = store.clone();
    let store_room_id = active_room_id.clone();
    let profile = Memo::new(move |_| {
        preview.get().map(|p| {
            profile_store
                .get_member_profile(&store_room_id, &p.sender_id)
                .get()
        })
    });

    let spans = Memo::new(move |_| {
        if let Some(preview) = preview.get() {
            preview.content
        } else {
            vec![RichTextSpan::Plain("click to go to event".to_string())]
        }
    });

    view! {
        <div
            class="flex items-center gap-1 mb-1 cursor-pointer [&_*]:pointer-events-none text-xs"
            on:click=move |_| scroll_to_item.run(target_event_id.clone())
        >
            {move || profile.get().render_icon("20px")}
            {move || profile.get().render_name_popup("12px")}
            <span class="truncate text-normal line-clamp-1">
                {move || {
                    let spans = spans.get();
                    spans
                        .into_iter()
                        .map(|v| v.clone().render(store.clone(), active_room_id.clone()))
                        .collect::<Vec<_>>()
                }}
            </span>
        </div>
    }
    .into_any()
}

#[component]
fn MessageHeader(
    reply_info: Option<ReplyInfo>,
    active_room_id: String,
    scroll_to_item: Callback<String>,
    show_header: bool,
    sender_profile_sig: ArcRwSignal<MemberProfile>,
    date: DateTime<Local>,
    current_highlight: Memo<Option<String>>,
    children: Children,
) -> impl IntoView {
    let has_reply = reply_info.is_some();
    let name_profile_sig = sender_profile_sig.clone();

    view! {
        <div class="flex gap-(--gap)">
            <div
                class="rounded-full w-1 m-1"
                style=move || {
                    if let Some(color) = current_highlight.get() {
                        format!("background-color: {color}")
                    } else {
                        "transparent".to_string()
                    }
                }
            ></div>

            <div class="shrink-0 mr-2 w-[40px] relative flex flex-col">
                <Show when=move || has_reply>
                    <div class="absolute left-[calc(50%-1px)] right-[-8px] top-2 h-4 border-l-2 border-t-2 border-white/20 rounded-tl-md -z-10"></div>
                </Show>

                <div
                    class="mb-[5px]"
                    class=("mt-[28px]", move || has_reply)
                    class=("mt-[5px]", move || !has_reply)
                >
                    {if show_header {
                        view! { {move || sender_profile_sig.get().render_icon("40px")} }.into_any()
                    } else {
                        ().into_any()
                    }}
                </div>
            </div>

            <div class="flex flex-col min-w-0 flex-1">
                <ReplyPreview
                    reply_info=reply_info
                    active_room_id=active_room_id
                    scroll_to_item=scroll_to_item
                />

                {move || {
                    if show_header {
                        view! {
                            <div class="flex items-baseline gap-2">
                                {name_profile_sig.get().render_name_popup("16px")}
                                <span class="text-muted text-xs">{format_date(date)}</span>
                            </div>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                }}

                {children()}
            </div>
        </div>
    }
}

async fn mxc_to_blob_url(mxc_url: String) -> Option<String> {
    use js_sys::{Array, Uint8Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Blob, BlobPropertyBag, Response, Url};

    let window = web_sys::window()?;
    let response: Response = JsFuture::from(window.fetch_with_str(&mxc_url))
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    let buffer = JsFuture::from(response.array_buffer().ok()?).await.ok()?;

    let uint8 = Uint8Array::new(&buffer);
    let arr = Array::new();
    arr.push(&uint8.buffer());

    let opts = BlobPropertyBag::new();
    opts.set_type("video/mp4");
    let blob = Blob::new_with_u8_array_sequence_and_options(&arr, &opts).ok()?;
    Url::create_object_url_with_blob(&blob).ok()
}

fn render_message_content(
    content: MessageContent,
    store: ProfileStore,
    room_id: String,
    sender_id: String,
    timestamp: u64,
) -> impl IntoView {
    let settings: Settings = expect_context();

    let spans = content.body;
    const MAX_W: u64 = 400;
    const MAX_H: u64 = 300;

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
        // TODO: Not implemented yet
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
            width,
            height,
            size,
            mime_type,
            blurhash
        } => {
            let (thumb_w, thumb_h) = shared::timeline::fit_dimensions(
                width.unwrap_or(MAX_W),
                height.unwrap_or(MAX_H),
                MAX_W,
                MAX_H,
            );
            let is_animated = matches!(
                mime_type.as_deref(),
                Some("image/gif") | Some("image/webp")
            );
            let thumb_src = if is_animated {
                source.url()
            } else {
                source.thumbnail_url(thumb_w, thumb_h)
            };
            let state: AppState = expect_context();

            let lightbox = state.lightbox_image;
            let lightbox_source = source.clone();
            let box_file_name = filename.clone();
            let box_width = width;
            let box_height = height;
            let loaded = RwSignal::new(false);
            let blurhash = StoredValue::new(blurhash);
            view! {
                <div class="mt-1">
                    <div class="relative inline-block group/image">
                        <img
                            src=thumb_src.clone()
                            alt=filename.clone()
                            width=thumb_w
                            height=thumb_h
                            class="rounded-md border border-[var(--tile-border-color)] relative group/image cursor-pointer"
                            on:load=move |_| loaded.set(true)
                            on:click=move |e: web_sys::MouseEvent| {
                                let origin_rect = e
                                    .target()
                                    .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                                    .map(|el| {
                                        let r = el.get_bounding_client_rect();
                                        (r.left(), r.top(), r.width(), r.height())
                                    });
                                lightbox
                                    .set(
                                        Some(LighboxImage {
                                            name: box_file_name.clone(),
                                            sender_id: sender_id.clone(),
                                            timestamp,
                                            size,
                                            source: lightbox_source.clone(),
                                            origin_rect,
                                            width: box_width,
                                            height: box_height,
                                        }),
                                    )
                            }
                            on:error=move |e| {
                                log::error!(
                                    "Image failed to load: {}, {}", source.url(), e.as_string().unwrap_or("Unknown error".to_string())
                                )
                            }
                        />
                        <div style=move || {
                            format!(
                                "position: absolute; inset: 0; border-radius: 6px; overflow: hidden; pointer-events: none; transition: opacity 0.4s ease; opacity: {};",
                                if loaded.get() { "0" } else { "1" },
                            )
                        }>
                            {move || match blurhash.get_value() {
                                Some(hash) => view! { <Blurhash hash=hash.clone() /> }.into_any(),
                                None => {
                                    view! {
                                        <div class="w-full h-full animate-pulse bg-(--ui-hover-bg)" />
                                    }
                                        .into_any()
                                }
                            }}
                        </div>
                        <div class="absolute bottom-1 left-1 bg-black/50 opacity-0 group-hover/image:opacity-100 transition-opacity rounded-md">
                            <span class="text-white text-sm px-1">{filename.clone()}</span>
                        </div>
                    </div>
                </div>
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
            .into_any()},
        // TODO: Not implemented yet
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
        // TODO: Not implemented yet
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
        // TODO: Not implemented yet
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
        // TODO: Not implemented yet
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
        // TODO: Not implemented yet
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
            <div class="text-normal leading-relaxed break-words pb-1">
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
                }}
                {move || if settings.url_previews_default.signal().get() {spans.clone().into_iter().map(render_link).collect_view().into_any()} else {().into_any()}}
            </div>
        }
            .into_any(),
        UiMessageType::Video { source, width, height, .. } => {
            const MAX_W: u64 = 400;
            const MAX_H: u64 = 300;
            let (vid_w, vid_h) = shared::timeline::fit_dimensions(
                width.unwrap_or(MAX_W),
                height.unwrap_or(MAX_H),
                MAX_W,
                MAX_H,
            );
            let mxc_url = source.url();
            let blob_url = LocalResource::new(move || {
                let url = mxc_url.clone();
                async move { mxc_to_blob_url(url).await }
            });
            view! {
                <Suspense fallback=move || {
                    view! {
                        <div
                            class="mt-1 rounded-md border border-[var(--tile-border-color)] flex items-center justify-center text-muted text-sm italic"
                            style=format!("width:{}px;height:{}px", vid_w, vid_h)
                        >
                            "Loading video..."
                        </div>
                    }
                }>
                    {move || {
                        blob_url
                            .get()
                            .flatten()
                            .map(|url| {
                                view! {
                                    <div class="mt-1">
                                        <video
                                            src=url
                                            controls=true
                                            preload="metadata"
                                            width=vid_w
                                            height=vid_h
                                            class="rounded-md border border-[var(--tile-border-color)]"
                                        />
                                    </div>
                                }
                            })
                    }}
                </Suspense>
            }.into_any()
        }
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
    store: ProfileStore,
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

            let contains_user = reactors.iter().any(|v| *v.sender_id == user_id);

            let reactor_pics = move || {
                let mut pics = Vec::new();

                let all_pics: Vec<(String, _)> = reactors
                    .iter()
                    .map(|info| {
                        let profile = store
                            .get_member_profile(&prof_room_id, &info.sender_id)
                            .get();
                        let icon = profile.clone().render_icon("20px");

                        let wrapped = view! {
                            <div
                                class="rounded-full ring-2 shrink-0 flex items-center justify-center transition-shadow"
                                class=("hover:ring-(--ui-solid-hover-bg)", !contains_user)
                                class=("ring-(--ui-solid-bg)", !contains_user)
                                class=("ring-(--accent-bg-color)", contains_user)
                                class=("group-hover:ring-(--ui-solid-hover-bg)", !contains_user)
                            >
                                {icon}
                            </div>
                        };
                        (profile.get_name(), wrapped.into_any())
                    })
                    .collect();

                let len = all_pics.len();
                pics.extend(all_pics.into_iter().map(|(_, pic)| pic).take(4));

                if len > 4 {
                    pics.push(
                        TextCircle(TextCircleProps::builder().text(format!("+{}", len - 4)).class("-ml-1.5 first:ml-0 w-[30px] h-[20px] rounded-full").color(Color::from_hsla(0.0, 0.0, 0.6, 1.0)).build()).into_any()
                    );
                }

                pics.collect_view()
            };

            let contains_user = reactors.iter().any(|v| *v.sender_id == user_id);

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
    sender_id: String,
    content: SystemMessage,
    store: ProfileStore,
    room_id: String,
) -> impl IntoView {
    let sender_id_str = sender_id.clone();

    let user_div = |user_id: &str| {
        let profile_sig = store.get_member_profile(&room_id, user_id);
        let name_sig = profile_sig.clone();

        view! {
            <div class="inline-flex items-center gap-1 pr-1 align-middle">
                {move || profile_sig.get().render_icon("20px")}
                {move || name_sig.get().render_name_popup("16px")}
            </div>
        }
    };

    let content = match content {
        SystemMessage::MembershipChange { user_id, change } => {
            let before = if let Some(change) = &change {
                match change {
                    UiMembershipChange::Joined => {
                        view! {
                            <span class="inline-flex align-middle">
                                <Icon icon=ARROW_RIGHT color="var(--online-color)" weight=IconWeight::Bold size="15px" />
                            </span>
                        }
                            .into_any()
                    }
                    _ => ().into_any(),
                }
            } else {
                ().into_any()
            };
            let text = if let Some(membership) = &change {
                match membership {
                    UiMembershipChange::None => "had no membership change",
                    UiMembershipChange::Banned => "was banned",
                    UiMembershipChange::Joined => "joined the room",
                    UiMembershipChange::Invited => "was invited",
                    UiMembershipChange::Left => "left the room",
                    UiMembershipChange::Kicked => "was kicked",
                    UiMembershipChange::Error => "had a membership change",
                    UiMembershipChange::InvitationAccepted => "accepted the invitation",
                    UiMembershipChange::InvitationRejected => "rejected the invitation",
                    UiMembershipChange::InvitationRevoked => "had their invitation revoked",
                    UiMembershipChange::KickedAndBanned => "was kicked and banned",
                    UiMembershipChange::KnockAccepted => "accepted the knock",
                    UiMembershipChange::KnockDenied => "denied the knock",
                    UiMembershipChange::KnockRetracted => "retracted the knock",
                    UiMembershipChange::Knocked => "knocked on the door",
                    UiMembershipChange::NotImplemented => "had a membership change",
                    UiMembershipChange::Unbanned => "was unbanned",
                }
            } else {
                "changed membership"
            };

            view! {
                <div>
                    {before} {user_div(&user_id)} <span>{text}</span>
                </div>
            }
            .into_any()
        }
        SystemMessage::CallInvite => view! {
            <div>
                {user_div(&sender_id_str)} <span>"started a call"</span>
            </div>
        }
        .into_any(),
        SystemMessage::CallMember => view! {
            <div>
                {user_div(&sender_id_str)} <span>"joined a call"</span>
            </div>
        }
        .into_any(),
        SystemMessage::PolicyRuleRoom => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed the room's policy"</span>
            </div>
        }
        .into_any(),
        SystemMessage::PolicyRuleServer => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed the server's policy"</span>
            </div>
        }
        .into_any(),
        SystemMessage::PolicyRuleUser => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed their policy"</span>
            </div>
        }
        .into_any(),
        SystemMessage::ProfileChange(change) => {
            let text = change.display_string();

            view! { <div>{user_div(&change.user_id)} <span>{text}</span></div> }
            .into_any()
        }
        SystemMessage::Redacted => view! {
            <div>
                {user_div(&sender_id_str)} <span>"had a message redacted"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomAvatar { .. } => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed the room avatar"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomCanonicalAlias { alias } => {
            let text = format!(
                "changed the room alias{}",
                if let Some(alias) = alias {
                    format!(" to {alias}")
                } else {
                    "".to_string()
                }
            );

            view! { <div>{user_div(&sender_id_str)} <span>{text}</span></div> }
        .into_any()
        }
        SystemMessage::RoomCreate {
            additional_creators,
            room_type,
        } => {
            let type_string = match room_type {
                None => "the room".to_string(),
                Some(room_type) => format!("the room ({})", room_type),
            };

            let additional = if additional_creators.is_empty() {
                "".to_string()
            } else {
                format!(" with {}", additional_creators.join(" ,"))
            };

            view! {
                <div>
                    {user_div(&sender_id_str)}
                    <span>{format!("created {type_string}{additional}")}</span>
                </div>
            }
            .into_any()
        }
        SystemMessage::RoomEncryption { algorithm } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("enabled encryption with {algorithm}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomGuestAccess { guest_access } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("changed guest access to: {guest_access}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomHistoryVisibility { visibility } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("changed history visibility to: {visibility}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomJoinRules { join_rule } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("changed join rules to: {join_rule}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomName { name } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("changed the room name to: {name}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomPinnedEvents { .. } => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed pinned events"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomPowerLevels => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed the room power levels"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomServerAcl => view! {
            <div>
                {user_div(&sender_id_str)} <span>"changed the room server ACL"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomThirdPartyInvite { display_name } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("invited {display_name} to the room")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomTombstone {
            body,
            replacement_room,
        } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>
                    {format!(
                        "closed the room. Reason: {body}. Replacement room: {replacement_room}",
                    )}
                </span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomTopic { topic } => view! {
            <div>
                {user_div(&sender_id_str)}
                <span>{format!("changed the room topic to: {topic}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RtcNotification {
            call_intent,
            declined_by,
        } => {
            let intent_string = if let Some(intent) = call_intent {
                match intent {
                    UiCallIntent::Audio => "started an audio call",
                    UiCallIntent::Video => "started a video call",
                    UiCallIntent::Unknown => "started a call",
                }
            } else {
                "started a call"
            };

            let declined_views = declined_by.iter().map(|user_id| {
                let profile_sig = store.get_member_profile(&room_id, user_id);
                let name_sig = profile_sig.clone();

                view! {
                    <div class="inline-flex items-center gap-1 pr-1 align-middle">
                        {move || profile_sig.get().render_icon("20px")}
                        {move || name_sig.get().render_name_popup("16px")}
                    </div>
                }
                .into_any()
            });

            let declined_view = if !declined_by.is_empty() {
                declined_views.collect_view().into_any()
            } else {
                ().into_any()
            };

            view! {
                <div>
                    {user_div(&sender_id_str)}
                    <span>
                        {format!(
                            "started {intent_string}{}",
                            if !declined_by.is_empty() { " which was declined by" } else { "" },
                        )}
                    </span> {declined_view}
                </div>
            }
            .into_any()
        }
        SystemMessage::SpaceChild {
            via,
            order,
            suggested,
        } => {
            let via_string = if !via.is_empty() {
                format!(" via {}", via.join(", "))
            } else {
                "".to_string()
            };

            let order_string = if let Some(order) = order {
                format!(" with order {}", order)
            } else {
                "".to_string()
            };

            let suggested_string = if suggested {
                " (suggested)".to_string()
            } else {
                "".to_string()
            };

            view! {
                <div>
                    {user_div(&sender_id_str)}
                    <span>
                        {format!(
                            "added this room as a child of a space{via_string}{order_string}{suggested_string}",
                        )}
                    </span>
                </div>
            }.into_any()
        }
        SystemMessage::SpaceParent { via, canonical } => {
            let via_string = if !via.is_empty() {
                format!(" via {}", via.join(", "))
            } else {
                "".to_string()
            };

            let canonical_string = if canonical {
                " (canonical)".to_string()
            } else {
                "".to_string()
            };

            view! {
                <div>
                    {user_div(&sender_id_str)}
                    <span>
                        {format!(
                            "added this room as a parent of a space{via_string}{canonical_string}",
                        )}
                    </span>
                </div>
            }
            .into_any()
        }
        SystemMessage::Unknown => view! {
            <div>
                {user_div(&sender_id_str)} <span>"performed an unknown system action"</span>
            </div>
        }
        .into_any(),
        SystemMessage::BeaconInfo => view! {
            <div>
                {user_div(&sender_id_str)} <span>"shared a live location"</span>
            </div>
        }
        .into_any(),
        SystemMessage::MemberHints => view! {
            <div>
                {user_div(&sender_id_str)} <span>"updated their member hints"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomImagePack => view! {
            <div>
                {user_div(&sender_id_str)} <span>"updated the room's image pack"</span>
            </div>
        }
        .into_any(),
    };

    view! { <div class="flex text-muted items-center justify-center my-2">{content.into_any()}</div> }
    .into_any()
}

#[component]
fn MesssageButtons(
    flags: Memo<EventFlags>,
    room_id: String,
    event_id: Option<String>,
    sender_id: String,
    item_sig: RwSignal<UiTimelineItem>,
    picker_open: RwSignal<bool>,
    show_delete_confirm: RwSignal<bool>,
) -> impl IntoView {
    let no_buttons = move || {
        let f = flags.get();
        !f.is_reactable && !f.can_be_replied_to && !f.is_editable
    };

    let important_event_id: RwSignal<Option<String>> = expect_context();

    let react_event_id = event_id.clone();
    let react_room_id = room_id.clone();
    let react = move |ev: web_sys::MouseEvent| {
        let emoji_state: EmojiPickerState = expect_context();
        let anchor: Element = ev.target().unwrap().unchecked_into();

        let Some(event_id) = react_event_id.clone() else {
            return;
        };

        let room_id = react_room_id.clone();

        picker_open.set(true);
        spawn_local(async move {
            let picked = pick_emoji(&anchor, emoji_state).await;
            picker_open.set(false);

            let Some(emoji) = picked else {
                return;
            };
            if let Err(e) = toggle_reaction(&room_id, &event_id, &emoji).await {
                log::error!("Failed to toggle reaction: {}", e);
            }
        });
    };

    let reply_event_id = event_id.clone();
    let reply = move |_| {
        let input_info: RwSignal<Option<ChatInputInfo>> = expect_context();
        let input_ref: NodeRef<Div> = expect_context();
        let sender_id = sender_id.clone();

        let Some(event_id) = reply_event_id.clone() else {
            return;
        };

        important_event_id.set(Some(event_id.clone()));
        input_info.set(Some(ChatInputInfo::ReplyingTo {
            event_id,
            sender_id,
        }));
        input_ref.get().map(|el| el.focus().ok());
    };

    let edit_room_id = room_id.clone();
    let edit_event_id = event_id.clone();
    let edit = move |_| {
        let input_info: RwSignal<Option<ChatInputInfo>> = expect_context();
        let attachments: RwSignal<Vec<Attachment>> = expect_context();
        let input_ref: NodeRef<Div> = expect_context();
        let store: ProfileStore = expect_context();
        let is_empty: RwSignal<bool> = expect_context();

        let Some(event_id) = edit_event_id.clone() else {
            return;
        };

        important_event_id.set(Some(event_id.clone()));
        input_info.set(Some(ChatInputInfo::Editing { event_id }));
        attachments.set(Vec::new());

        if let Some(el) = input_ref.get() {
            el.focus().ok();
            let spans = item_sig.get_untracked().body();
            el.set_inner_html(&richt_text_spans_to_html(
                &spans,
                store.clone(),
                edit_room_id.clone(),
            ));
            is_empty.set(false);
            move_caret_to_end(&el);
        }
    };

    let interacting = move || picker_open.get() || show_delete_confirm.get();

    let delete_event_id = event_id.clone();
    let delete_room_id = room_id.clone();
    let on_delete_confirm = Callback::new(move |_| {
        let Some(event_id) = delete_event_id.clone() else {
            return;
        };
        let room_id = delete_room_id.clone();

        spawn_local(async move {
            if let Err(e) = delete_message(&room_id, &event_id).await {
                log::error!("Failed to delete message: {}", e);
            }
        });
    });

    view! {
        <div
            class="absolute -top-4 right-4 z-10 transform-gpu flex items-center gap-1 bg-(--ui-solid-bg) p-1 rounded-(--gap) text-muted text-xs border border-(--tile-border-color) opacity-0 group-hover/msg:opacity-100"
            class=("hidden", no_buttons)
            style:opacity=move || interacting().then_some("1")
        >
            <Show when=move || { flags.get().is_reactable }>
                <button
                    class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-normal"
                    on:click=react.clone()
                >
                    <Icon icon=SMILEY size="20px"></Icon>
                </button>
            </Show>
            <Show when=move || { flags.get().can_be_replied_to }>
                <button
                    class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-normal"
                    on:click=reply.clone()
                >
                    <Icon icon=ARROW_BEND_UP_LEFT size="20px"></Icon>
                </button>
            </Show>
            <Show when=move || { flags.get().is_editable }>
                <button
                    class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-normal"
                    on:click=edit.clone()
                >
                    <Icon icon=PENCIL_SIMPLE size="20px"></Icon>
                </button>
            </Show>
            <Show when=move || { flags.get().is_deletable }>
                <button
                    class="hover:bg-(--ui-solid-hover-bg) cursor-pointer p-0.5 rounded-(--gap) hover:text-red-500"
                    on:click=move |_| show_delete_confirm.set(true)
                >
                    <Icon icon=TRASH size="20px"></Icon>
                </button>
            </Show>
        </div>
        <ConfirmDialog show=show_delete_confirm class="w-100">
            <p class="text-bright text-xl font-bold">"Delete message"</p>
            <p class="text-muted">"Are you sure you want to delete this message?"</p>
            <div class="my-2 p-2 bg-(--ui-floating-bg) border border-(--tile-border-color) rounded-(--gap)">
                {render_timeline_item(item_sig, true, true, Callback::new(|_| {}))}
            </div>
            <div class="flex gap-2 pt-2 justify-end w-full">
                <button
                    class="px-4 py-1.5 rounded-(--ui-border-radius) text-sm bg-(--ui-solid-hover-bg) hover:bg-(--ui-floating-hover-bg) text-normal cursor-pointer border border-(--tile-border-color) flex flex-grow items-center justify-center"
                    on:click=move |_| show_delete_confirm.set(false)
                >
                    "Cancel"
                </button>
                <button
                    class="px-4 py-1.5 rounded-(--ui-border-radius) text-sm bg-red-600 hover:bg-red-700 text-white cursor-pointer font-semibold flex flex-grow items-center justify-center"
                    on:click=move |_| {
                        show_delete_confirm.set(false);
                        on_delete_confirm.run(());
                    }
                >
                    "Delete"
                </button>
            </div>
        </ConfirmDialog>
    }
}

#[component]
fn ConfirmDialog(
    show: RwSignal<bool>,
    children: ChildrenFn,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    let children = StoredValue::new(children);
    let dialog_class = StoredValue::new(format!(
        "relative pointer-events-auto bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--floating-border-radius) shadow-xl p-3 flex flex-col backdrop-blur-xl {class}",
    ));

    view! {
        <Show when=move || show.get()>
            <Portal>
                <div
                    class="fixed inset-0 z-[1000] bg-(--tile-bg-color)"
                    on:click=move |_| show.set(false)
                />
                <div class="fixed inset-0 z-[1001] flex items-center justify-center pointer-events-none">
                    <div class=move || dialog_class.get_value()>
                        <CloseButton on_click=move |_| show.set(false) />
                        {children.get_value()()}
                    </div>
                </div>
            </Portal>
        </Show>
    }
}

fn render_timeline_event(
    store: ProfileStore,
    room_id: &str,
    own_user_id: &str,
    item_sig: RwSignal<UiTimelineItem>,
    show_header: bool,
    preview: bool,
    scroll_to_event: Callback<String>,
) -> impl IntoView {
    let hovered = RwSignal::new(false);
    let picker_open = RwSignal::new(false);
    let show_delete_confirm = RwSignal::new(false);

    let scroll_target = expect_context::<super::ScrollTarget>().0;
    let node_ref = NodeRef::<Div>::new();

    let (show_highlight, date, sender_id, reply_info, event_id) = item_sig.with_untracked(|item| {
        if let UiTimelineItemKind::Event(event) = &item.kind {
            let sender_id = event.sender_id.clone();
            (
                event.flags.is_highlighted,
                get_date_from_ts(event.timestamp as i64),
                sender_id,
                event.in_reply_to(),
                event.event_id.clone(),
            )
        } else {
            unreachable!("Must be an event")
        }
    });

    let this_event_id = event_id.clone();
    Effect::new(move |_| {
        let Some(el) = node_ref.get() else { return };
        let Some(target) = scroll_target.get() else {
            return;
        };
        if Some(&target) == this_event_id.as_ref() {
            scroll_target.set(None);
            let options = web_sys::ScrollIntoViewOptions::new();
            options.set_behavior(web_sys::ScrollBehavior::Smooth);
            options.set_block(web_sys::ScrollLogicalPosition::Center);
            el.class_list().add_1("animate-highlight").ok();
            el.scroll_into_view_with_scroll_into_view_options(&options);
            let el_id = format!("timeline-event-{target}");
            set_timeout(
                move || {
                    if let Some(el) = document().get_element_by_id(&el_id) {
                        el.class_list().remove_1("animate-highlight").ok();
                    }
                },
                std::time::Duration::from_secs(4),
            );
        }
    });

    let room_id_for_content = room_id.to_string();
    let store_for_content = store.clone();

    let content_for_render = Memo::new(move |_| {
        item_sig.with(|item| {
            let UiTimelineItemKind::Event(event) = &item.kind else {
                return None;
            };
            let mut content = event.content.clone();
            if let EventContent::MsgLike(msg) = &mut content {
                msg.reactions.clear();
            }
            Some((event.sender_id.clone(), event.timestamp, content))
        })
    });

    let rendered_content = move || {
        let Some((sender_id, timestamp, content)) = content_for_render.get() else {
            return ().into_any();
        };

        match content {
            EventContent::MsgLike(ev) => render_message_content(
                *ev,
                store_for_content.clone(),
                room_id_for_content.clone(),
                sender_id,
                timestamp,
            ).into_any(),
            EventContent::FailedToParseMessageLike { event_type, error } => view! { <div class="text-red-500 italic">{format!("Failed to render {event_type}: {error}")}</div> }.into_any(),
            EventContent::FailedToParseState { event_type, state_key, error } => view! {
                <div class="text-red-500 italic">
                    {format!("Failed to render {event_type} with state key {state_key}: {error}")}
                </div>
            }.into_any(),
            EventContent::SystemMessage(ev) => render_system_message(
                sender_id,
                ev,
                store_for_content.clone(),
                room_id_for_content.clone()
            ).into_any(),
        }
    };

    let store_clone = store.clone();
    let event_id_clone = event_id.clone();
    let room_id = room_id.to_string();
    let own_user_id = own_user_id.to_string();
    let reactions_room_id = room_id.clone();

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
            reactions_room_id.clone(),
            own_user_id.clone(),
            event_id_clone.clone(),
        )
    };

    let important_event_id: RwSignal<Option<String>> = expect_context();
    let color_event_id = event_id.clone();

    let current_highlight = Memo::new(move |_| {
        if let Some(important_id) = important_event_id.get()
            && let Some(event_id) = &color_event_id
            && important_id == *event_id
        {
            log::info!("Highlighting event {important_id}");
            Some("white".to_string())
        } else if show_highlight {
            Some("var(--accent-color)".to_string())
        } else {
            None
        }
    });

    let flags = Memo::new(move |_| {
        let item = item_sig.get();

        item.flags()
    });

    let sender_profile_sig = store.get_member_profile(&room_id, &sender_id.clone());

    let reply_room_id = room_id.clone();

    let is_active = move || hovered.get() || picker_open.get() || show_delete_confirm.get();

    view! {
        <div
            node_ref=node_ref
            class="group/msg mx-1 relative flex flex-col gap-[var(--gap)] hover:bg-black/20 rounded-md transform-gpu border border-transparent hover:border-[var(--tile-border-color)] pt-0.75"
            class=("mt-4", show_header && !preview)
            class=("pointer-events-none", preview)
            class=("bg-black/20", move || picker_open.get() || show_delete_confirm.get())
            id=move || item_sig.get().render_key()
            style:background=move || {
                current_highlight
                    .get()
                    .map(|color| {
                        let hovered = is_active();
                        format!(
                            "linear-gradient(in oklch to right, oklch(from {color} l c h / {}) 20%, oklch(from {color} l c h / 0) 100%)",
                            if hovered { "0.10" } else { "0.15" },
                        )
                    })
                    .unwrap_or_default()
            }
            on:mouseenter=move |_| hovered.set(true)
            on:mouseleave=move |_| hovered.set(false)
        >
            {move || {
                if hovered.get() && !show_header {
                    let ml = if current_highlight.get().is_some() {
                        "ml-[14px]"
                    } else {
                        "ml-[5px]"
                    };
                    view! {
                        <div class=format!(
                            "absolute text-xs text-muted mt-[5px] {ml}",
                        )>{date.format("%H:%M").to_string()}</div>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}

            <Show when=move || !preview>
                <MesssageButtons
                    flags=flags
                    room_id=room_id.clone()
                    event_id=event_id.clone()
                    sender_id=sender_id.clone()
                    item_sig=item_sig
                    picker_open=picker_open
                    show_delete_confirm=show_delete_confirm
                />
            </Show>

            <MessageHeader
                reply_info=reply_info
                active_room_id=reply_room_id
                scroll_to_item=scroll_to_event
                show_header=show_header
                sender_profile_sig=sender_profile_sig
                date=date
                current_highlight=current_highlight
            >
                <div>
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
            </MessageHeader>
        </div>
    }.into_any()
}

pub fn render_timeline_item(
    item_sig: RwSignal<UiTimelineItem>,
    show_header: bool,
    preview: bool,
    scroll_to_event: Callback<String>,
) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let Some(room_id) = state.active_room_id_untracked() else {
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
            let is_week_ago = date > Local::now() - chrono::Duration::days(7);

            let label = if is_today {
                "Today".to_string()
            } else if is_yesterday {
                "Yesterday".to_string()
            } else {
                date.format(&format!("{}%d %B %Y", if is_week_ago {
                    "%a "
                } else {
                    ""
                })).to_string()
            };

            view! {
                <div class="flex items-center gap-2 mt-4 drop-shadow">
                    <div class="flex-1 border-t-1 border-(--tile-border-color) bdf"></div>
                    <span class="text-muted text-sm bdf-text">{label}</span>
                    <div class="flex-1 border-t-1 border-(--tile-border-color) bdf"></div>
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
        UiTimelineItemKind::TimelineStart => {
            let room = state.active_room.get_untracked();
            let (icon, name, is_dm) = if let Some(ref room) = room {
                let is_dm = matches!(room.kind, RoomKind::Dm { .. });
                let icon = match &room.kind {
                    RoomKind::VoiceChannel => SPEAKER_HIGH,
                    _ => HASH,
                };
                (icon, room.get_name(), is_dm)
            } else {
                (HASH, "this channel".to_string(), false)
            };

            let heading = if is_dm {
                format!("Welcome to your chat with {}!", name)
            } else {
                format!("Welcome to #{}!", name)
            };
            let subtitle = if is_dm {
                format!("This is the beginning of your direct message history with {}.", name)
            } else {
                format!("This is the start of the #{} channel.", name)
            };

            view! {
                <div class="flex flex-col items-start px-4 pt-10 pb-6 gap-2 pt-30">
                    <div class="w-16 h-16 rounded-full bg-(--ui-solid-bg)  border border-(--tile-border-color) flex items-center justify-center mb-2">
                        <Icon
                            icon=icon
                            size="36px"
                            weight=IconWeight::Bold
                            color="var(--text-base)"
                        />
                    </div>
                    <h2 class="text-3xl font-bold text-bright">{heading}</h2>
                    <p class="text-muted text-sm">{subtitle}</p>
                </div>
            }
            .into_any()
        }
        UiTimelineItemKind::Event(_) => render_timeline_event(store, &room_id, &user_id, item_sig, show_header, preview, scroll_to_event).into_any()
    }
}
