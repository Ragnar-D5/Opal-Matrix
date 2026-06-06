use std::collections::HashMap;

use chrono::{DateTime, Local, TimeZone};
use colorsys::Hsl;
use leptos::{html::Div, prelude::*, task::spawn_local};
use phosphor_leptos::{
    Icon, IconWeight, ARROW_BEND_UP_LEFT, ARROW_RIGHT, HASH, PENCIL_SIMPLE, SMILEY, SPEAKER_HIGH,
    TRASH, WARNING_CIRCLE, X,
};
use shared::{
    get_color,
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
    app::format_date,
    components::{
        chat::{Attachment, ChatInputInfo},
        emoji_picker::{pick_emoji, EmojiPickerState},
        input::move_caret_to_end,
        previews::render_link,
        text::{richt_text_spans_to_html, RichTextExt},
        user_profile::{render_profile_name, MemberProfileExt},
        TextCircle, TextCircleProps,
    },
    state::{AppState, LighboxImage, ProfileStore},
    tauri_functions::{delete_message, toggle_reaction},
};

#[component]
fn ReplyPreview(reply_info: Option<ReplyInfo>, active_room_id: String) -> impl IntoView {
    let Some(reply_info) = reply_info else {
        return ().into_any();
    };

    let store: ProfileStore = expect_context();

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
        if let Some(preview) = preview.get() {
            match &preview.sender {
                DetailState::Ready(sender) => Some(
                    profile_store
                        .get_member_profile(&store_room_id, &sender.id)
                        .get(),
                ),
                DetailState::Error(e) => {
                    log::error!("Failed to load sender profile for reply preview: {e}");
                    None
                }
                DetailState::Pending => None,
                DetailState::Unavailable => {
                    log::error!("Sender profile for reply preview is unavailable");
                    None
                }
            }
        } else {
            None
        }
    });

    let spans = Memo::new(move |_| {
        if let Some(preview) = preview.get() {
            preview.content
        } else {
            vec![RichTextSpan::Plain("click to go to event".to_string())]
        }
    });

    view! {
        <div class="flex items-center gap-1 ml-[52px] mb-1 cursor-pointer text-xs relative group/reply cursor-pointer">
            <div class="absolute -left-[32px] top-[calc(50%-1px)] w-[28px] h-4.5 border-l-2 border-t-2 border-white/20 rounded-tl-md"></div>

            {move || profile.get().render_icon("20px")}
            {move || profile.get().render_name("12px")}

            <span class="truncate text-bright line-clamp-1">
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
    sender_id: Option<String>,
    timestamp: u64,
) -> impl IntoView {
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
            ..
        } => {
            let (thumb_w, thumb_h) = shared::timeline::fit_dimensions(
                width.unwrap_or(MAX_W),
                height.unwrap_or(MAX_H),
                MAX_W,
                MAX_H,
            );
            let thumb_src = source.thumbnail_url(thumb_w, thumb_h);
            let state: AppState = expect_context();

            let lightbox = state.lightbox_image;
            let lightbox_source = source.clone();
            let box_file_name = filename.clone();
            let box_width = width;
            let box_height = height;
            view! {
                <div class="mt-1">
                    <div class="relative inline-block group/image">
                        <img
                            src=thumb_src.clone()
                            alt=filename.clone()
                            width=thumb_w
                            height=thumb_h
                            class="rounded-md border border-[var(--tile-border-color)] relative group/image cursor-pointer"
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
                                    "Image failed to load: {}, {}", source.url(), e.to_js_string()
                                )
                            }
                        />
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
                        TextCircle(TextCircleProps::builder().text(format!("+{}", len - 4)).class("-ml-1.5 first:ml-0 w-[30px] h-[20px] rounded-full").color(Hsl::new(0.0, 0.0, 60.0, None)).build()).into_any()
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
    sender_id: Option<String>,
    content: SystemMessage,
    store: ProfileStore,
    room_id: String,
) -> impl IntoView {
    let sender_id_str = sender_id.clone().unwrap_or_default();

    let user_div = |user_id: &str| {
        let profile_sig = store.get_member_profile(&room_id, user_id);
        let name_sig = profile_sig.clone();

        view! {
            <div class="flex items-center gap-1 pr-1">
                {move || profile_sig.get().render_icon("20px")}
                {move || name_sig.get().render_name("16px")}
            </div>
        }
    };

    let content = match content {
        SystemMessage::MembershipChange { user_id, change } => {
            let before = if let Some(change) = &change {
                match change {
                    UiMembershipChange::Joined => {
                        view! { <Icon icon=ARROW_RIGHT color="var(--online-color)" weight=IconWeight::Bold size="15px" /> }.into_any()
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
                <div class="flex flex-row gap-1 items-center">
                    {before} {user_div(&user_id)} <span>{text}</span>
                </div>
            }
            .into_any()
        }
        SystemMessage::CallInvite => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"started a call"</span>
            </div>
        }
        .into_any(),
        SystemMessage::CallMember => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"joined a call"</span>
            </div>
        }
        .into_any(),
        SystemMessage::PolicyRuleRoom => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"changed the room's policy"</span>
            </div>
        }
        .into_any(),
        SystemMessage::PolicyRuleServer => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"changed the server's policy"</span>
            </div>
        }
        .into_any(),
        SystemMessage::PolicyRuleUser => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"changed their policy"</span>
            </div>
        }
        .into_any(),
        SystemMessage::ProfileChange(change) => {
            let text = change.display_string();

            view! { <div class="flex flex-row gap-1">{user_div(&change.user_id)} <span>{text}</span></div> }
            .into_any()
        }
        SystemMessage::Redacted => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"had a message redacted"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomAvatar { .. } => view! {
            <div class="flex flex-row gap-1">
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

            view! { <div class="flex flex-row gap-1">{user_div(&sender_id_str)} <span>{text}</span></div> }
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
                <div class="flex flex-row gap-1">
                    {user_div(&sender_id_str)}
                    <span>{format!("created {type_string}{additional}")}</span>
                </div>
            }
            .into_any()
        }
        SystemMessage::RoomEncryption { algorithm } => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)}
                <span>{format!("enabled encryption with {algorithm}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomGuestAccess { guest_access } => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)}
                <span>{format!("changed guest access to: {guest_access}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomHistoryVisibility { visibility } => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)}
                <span>{format!("changed history visibility to: {visibility}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomJoinRules { join_rule } => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)}
                <span>{format!("changed join rules to: {join_rule}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomName { name } => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)}
                <span>{format!("changed the room name to: {name}")}</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomPinnedEvents { .. } => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"changed pinned events"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomPowerLevels => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"changed the room power levels"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomServerAcl => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"changed the room server ACL"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomThirdPartyInvite { display_name } => view! {
            <div class="flex flex-row gap-1">
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
            <div class="flex flex-row gap-1">
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

            let declined_string = if !declined_by.is_empty() {
                format!(" which was declined by {}", declined_by.join(", "))
            } else {
                "".to_string()
            };

            view! {
                <div class="flex flex-row gap-1">
                    {user_div(&sender_id_str)}
                    <span>{format!("started {intent_string}{declined_string}")}</span>
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
                <div class="flex flex-row gap-1">
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
                <div class="flex flex-row gap-1">
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
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"performed an unknown system action"</span>
            </div>
        }
        .into_any(),
        SystemMessage::BeaconInfo => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"shared a live location"</span>
            </div>
        }
        .into_any(),
        SystemMessage::MemberHints => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"updated their member hints"</span>
            </div>
        }
        .into_any(),
        SystemMessage::RoomImagePack => view! {
            <div class="flex flex-row gap-1">
                {user_div(&sender_id_str)} <span>"updated the room's image pack"</span>
            </div>
        }
        .into_any(),
    };

    view! { <div class="flex text-muted items-center justify-center my-2">{content.into_any()}</div> }
    .into_any()
}

fn message_buttons(
    flags: Memo<EventFlags>,
    room_id: String,
    event_id: Option<String>,
    sender_id: Option<String>,
    item_id: String,
    item_sig: RwSignal<UiTimelineItem>,
) -> impl IntoView {
    let no_buttons = move || {
        let f = flags.get();
        !f.is_reactable && !f.can_be_replied_to && !f.is_editable
    };

    let react_event_id = event_id.clone();
    let react_room_id = room_id.clone();
    let react = move |ev: web_sys::MouseEvent| {
        let emoji_state: EmojiPickerState = expect_context();
        let anchor: Element = ev.target().unwrap().unchecked_into();

        let Some(event_id) = react_event_id.clone() else {
            return;
        };

        let room_id = react_room_id.clone();

        spawn_local(async move {
            let Some(emoji) = pick_emoji(&anchor, emoji_state).await else {
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

        let Some(event_id) = reply_event_id.clone() else {
            return;
        };
        let Some(sender_id) = sender_id.clone() else {
            return;
        };

        input_info.set(Some(ChatInputInfo::ReplyingTo {
            event_id,
            sender_id,
            item_id: "".to_string(),
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

        input_info.set(Some(ChatInputInfo::Editing {
            event_id,
            item_id: item_id.clone(),
        }));
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

    let show_delete_confirm = RwSignal::new(false);

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
            class="absolute -top-4 right-4 flex items-center gap-1 bg-(--ui-solid-bg) p-1 rounded-(--gap) text-muted text-xs border border-(--tile-border-color) opacity-0 group-hover/msg:opacity-100"
            class=("hidden", no_buttons)
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
            <Show when=move || { flags.get().is_editable }>
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
                {render_timeline_item(item_sig, true, true)}
            </div>
            <div class="flex gap-2 pt-2 justify-end w-full">
                <button
                    class="px-4 py-1.5 rounded-(--ui-border-radius) text-sm bg-(--ui-solid-hover-bg) hover:bg-(--ui-floating-hover-bg) text-(--text-color) cursor-pointer border border-(--tile-border-color) flex flex-grow items-center justify-center"
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
    view! {
        <Show when=move || show.get()>
            <div
                class="fixed inset-0 z-[1000] bg-(--tile-bg-color)"
                on:click=move |_| show.set(false)
            />
            <div class="fixed inset-0 z-[1001] flex items-center justify-center pointer-events-none">
                <div class=format!(
                    "relative pointer-events-auto bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--floating-border-radius) shadow-xl p-3 flex flex-col backdrop-blur-xl {class}",
                )>
                    <button
                        class="absolute top-3 right-3 text-muted hover:text-(--bright-text-color) border border-transparent hover:bg-(--ui-solid-hover-bg) hover:border-(--tile-border-color) cursor-pointer p-1 rounded-(--gap)"
                        on:click=move |_| show.set(false)
                    >
                        <Icon icon=X size="18px" />
                    </button>
                    {children()}
                </div>
            </div>
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
) -> impl IntoView {
    let hovered = RwSignal::new(false);

    let (show_highlight, date, sender_id, name, color, reply_info, event_id) = item_sig
        .with_untracked(|item| {
            if let UiTimelineItemKind::Event(event) = &item.kind {
                let sender_id = event.get_sender_id();
                let name = event
                    .get_sender_name()
                    .unwrap_or(sender_id.clone().unwrap_or("Unknown".to_string()));
                (
                    event.flags.is_highlighted,
                    get_date_from_ts(event.timestamp as i64),
                    sender_id,
                    name,
                    event
                        .get_sender_id()
                        .map(|v| get_color(&v))
                        .unwrap_or(Hsl::new(0.0, 0.0, 70.0, None)),
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

    let content_for_render = Memo::new(move |_| {
        item_sig.with(|item| {
            let UiTimelineItemKind::Event(event) = &item.kind else {
                return None;
            };
            let mut content = event.content.clone();
            if let EventContent::MsgLike(msg) = &mut content {
                msg.reactions.clear();
            }
            Some((event.get_sender_id(), event.timestamp, content))
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

    let input_info: RwSignal<Option<ChatInputInfo>> = expect_context();

    let current_highlight = Memo::new({
        let item_id = item_id.clone();
        move |_| {
            match input_info.get() {
                Some(ChatInputInfo::ReplyingTo {
                    item_id: reply_id, ..
                }) if *reply_id == item_id => return Some("white".to_string()),
                Some(ChatInputInfo::Editing {
                    item_id: edit_id, ..
                }) if *edit_id == item_id => return Some("white".to_string()),
                _ => (),
            }
            if show_highlight {
                return Some("var(--accent-color)".to_string());
            }
            None
        }
    });

    let flags = Memo::new(move |_| {
        let item = item_sig.get();

        item.flags()
    });

    let sender_profile_sig =
        store.get_member_profile(&room_id, &sender_id.clone().unwrap_or_default());

    view! {
        <div
            class="group/msg relative flex flex-col gap-[var(--gap)] hover:bg-black/20 rounded-md"
            class=("mt-5", show_header && !preview)
            class=("pointer-events-none", preview)
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

            {if !preview {
                message_buttons(
                        flags,
                        room_id.clone(),
                        event_id.clone(),
                        sender_id.clone(),
                        item_id.clone(),
                        item_sig,
                    )
                    .into_any()
            } else {
                ().into_any()
            }}

            <ReplyPreview reply_info=reply_info active_room_id=room_id />

            <div class="flex gap-[var(--gap)]">
                <div class="shrink-0 mr-2 w-[40px] mt-[5px]">
                    {if show_header {
                        view! { {move || sender_profile_sig.get().render_icon("40px")} }.into_any()
                    } else {
                        ().into_any()
                    }}
                </div>

                <div class="flex flex-col min-w-0 flex-1">
                    {if show_header {
                        view! {
                            <div class="flex items-baseline gap-2">
                                <span class="text-bright truncate cursor-pointer">
                                    {render_profile_name(name, color, "16px")}
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

pub fn render_timeline_item(
    item_sig: RwSignal<UiTimelineItem>,
    show_header: bool,
    preview: bool,
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
                            color="var(--text-color)"
                        />
                    </div>
                    <h2 class="text-3xl font-bold text-bright">{heading}</h2>
                    <p class="text-muted text-sm">{subtitle}</p>
                </div>
            }
            .into_any()
        }
        UiTimelineItemKind::Event(_) => render_timeline_event(store, &room_id, &user_id, item_sig, show_header, preview).into_any()
    }
}
