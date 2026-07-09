use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use crate::{
    app::{convertFileSrc, format_bytes},
    components::{
        FloatingTile, TypingIndicator,
        chat::{
            calls::CallView, header::ChatHeader, info::ChatSideBar, messages::render_timeline_item,
        },
        input::{
            get_active_filter, get_caret_position, handle_input, handle_keydown,
            insert_text_at_caret,
            menu::{MenuCompletionMatches, MenuType, SelectionMenu},
        },
        overlays::{
            emoji_picker::{EmojiPickerState, pick_emoji},
            gif_picker::{GifPickerState, pick_gif},
        },
        user_profile::MemberProfileExt,
    },
    hooks::{setup_update_effect, use_tauri_event},
    state::{AppState, CurrentSection, ProfileStore},
    tauri_functions::{get_timeline, indicate_typing, pick_files, scroll_timeline},
};

use phosphor_leptos::{GIF, Icon, IconWeight, SMILEY, TRASH, UPLOAD_SIMPLE, X_CIRCLE};

use leptos::{ev, html::Div, prelude::*, task::spawn_local};
use leptos_use::{
    UseIntersectionObserverOptions, UseIntersectionObserverReturn, use_event_listener,
    use_intersection_observer_with_options,
};
use shared::{
    api::{
        FileMetadata, ScrollDirection, SearchParameters, UiAttachmentSource,
        events::SearchResultUpdate,
    },
    sidebar::RoomNode,
    timeline::{EventContent, UiTimelineDiff, UiTimelineItem, UiTimelineItemKind},
};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Element, IntersectionObserverEntry, ScrollBehavior, ScrollIntoViewOptions,
    ScrollLogicalPosition,
};

pub(crate) mod calls;
pub(crate) mod header;
pub(crate) mod info;
pub(crate) mod messages;

#[component]
fn TimeLine() -> impl IntoView {
    let state: AppState = expect_context();

    let messages_update_event: ReadSignal<Option<Vec<UiTimelineDiff>>> = use_tauri_event();

    let messages: RwSignal<Vec<RwSignal<UiTimelineItem>>> = RwSignal::new(Vec::new());

    let owner = Owner::current().expect("TimeLine must have an owner");

    setup_update_effect(messages_update_event, move |diffs| {
        owner.with(|| {
            for diff in diffs {
                match diff {
                    UiTimelineDiff::Set { index, value } => {
                        messages.with_untracked(|msgs| {
                            if let Some(item) = msgs.get(index) {
                                item.set(value);
                            }
                        });
                    }
                    UiTimelineDiff::Append { values } => {
                        messages.update(|msgs| {
                            msgs.extend(values.iter().map(|v| RwSignal::new(v.clone())));
                        });
                    }
                    UiTimelineDiff::PushBack { value } => {
                        messages.update(|msgs| msgs.push(RwSignal::new(value)));
                    }
                    UiTimelineDiff::PushFront { value } => {
                        messages.update(|msgs| msgs.insert(0, RwSignal::new(value)));
                    }
                    UiTimelineDiff::Insert { index, value } => {
                        messages.update(|msgs| {
                            if index <= msgs.len() {
                                msgs.insert(index, RwSignal::new(value));
                            }
                        });
                    }
                    UiTimelineDiff::Remove { index } => {
                        messages.update(|msgs| {
                            if index < msgs.len() {
                                msgs.remove(index);
                            }
                        });
                    }
                    UiTimelineDiff::PopBack => {
                        messages.update(|msgs| {
                            msgs.pop();
                        });
                    }
                    UiTimelineDiff::PopFront => {
                        messages.update(|msgs| {
                            if !msgs.is_empty() {
                                msgs.remove(0);
                            }
                        });
                    }
                    UiTimelineDiff::Clear => {
                        messages.update(|msgs| msgs.clear());
                    }
                    UiTimelineDiff::Reset { values } => {
                        messages.update(|msgs| {
                            msgs.clear();
                            msgs.extend(values.iter().map(|v| RwSignal::new(v.clone())));
                        });
                    }
                    UiTimelineDiff::Truncate { length } => {
                        messages.update(|msgs| msgs.truncate(length));
                    }
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
                continue;
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
                if let UiTimelineItemKind::Event(event) = &item.kind {
                    last_sender = Some(event.sender_id.clone());
                    last_timestamp = Some(event.timestamp);
                }
            } else if let UiTimelineItemKind::Event(event) = &item.kind {
                if let EventContent::MsgLike(msg) = &event.content
                    && msg.in_reply_to.is_some()
                {
                    show_header = true;
                } else {
                    let id = &event.sender_id;

                    if let Some(last_sender_id) = &last_sender
                        && last_sender_id == id
                        && let Some(last_ts) = last_timestamp
                    {
                        // If this message is within 5 minutes of the previous message, hide header
                        if event.timestamp.saturating_sub(last_ts) < 5 * 60 {
                            show_header = false;
                        }
                    }
                    last_sender = Some(id.clone());
                    last_timestamp = Some(event.timestamp);
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

    let is_loading_top = RwSignal::new(false);
    let is_loading_bottom = RwSignal::new(false);
    let has_more_top = RwSignal::new(true);
    let has_more_bottom = RwSignal::new(true);

    let sentinel_ref_top = NodeRef::<Div>::new();
    let sentinel_ref_bottom = NodeRef::<Div>::new();

    let timeline_id: RwSignal<Option<String>> = RwSignal::new(None);

    let JumpTarget(jump_target) = expect_context();

    let fetch_more = move |scroll_direction: ScrollDirection,
                           loading: RwSignal<bool>,
                           has_more: RwSignal<bool>| {
        let Some(id) = timeline_id.get_untracked() else {
            return;
        };

        // Preserve scroll position when paginating down (newer messages): anchor to
        // the row just past the bottom sentinel and pull it back into view once the
        // new rows mount. Upward pagination needs no correction — `flex-col-reverse`
        // keeps the view stable when older rows are prepended above it.
        let forward_anchor: Option<web_sys::Element> =
            if matches!(scroll_direction, ScrollDirection::Down) {
                sentinel_ref_bottom
                    .get_untracked()
                    .and_then(|s| s.next_element_sibling())
            } else {
                None
            };

        loading.set(true);
        spawn_local(async move {
            match scroll_timeline(&id, scroll_direction).await {
                Ok(new_has_more) => has_more.set(new_has_more),
                Err(e) => log::error!("Failed to fetch more messages: {}", e),
            };
            loading.set(false);

            if let Some(anchor) = forward_anchor {
                set_timeout(
                    move || {
                        let options = ScrollIntoViewOptions::new();
                        options.set_behavior(ScrollBehavior::Instant);
                        options.set_block(ScrollLogicalPosition::Start);
                        anchor.scroll_into_view_with_scroll_into_view_options(&options);
                    },
                    Duration::ZERO,
                );
            }
        });
    };

    let UseIntersectionObserverReturn { .. } = use_intersection_observer_with_options(
        sentinel_ref_top,
        move |entries: Vec<IntersectionObserverEntry>, _| {
            if entries[0].is_intersecting() && has_more_top.get() && !is_loading_top.get() {
                fetch_more(ScrollDirection::Up, is_loading_top, has_more_top);
            };
        },
        UseIntersectionObserverOptions::default().root_margin("200px 0px 200px 0px"),
    );

    let UseIntersectionObserverReturn { .. } = use_intersection_observer_with_options(
        sentinel_ref_bottom,
        move |entries: Vec<IntersectionObserverEntry>, _| {
            if entries[0].is_intersecting() && has_more_bottom.get() && !is_loading_bottom.get() {
                fetch_more(ScrollDirection::Down, is_loading_bottom, has_more_bottom);
            };
        },
        UseIntersectionObserverOptions::default().root_margin("200px 0px 200px 0px"),
    );

    Effect::new(move || {
        if let Some(room_id) = state.active_room_id() {
            if jump_target.get_untracked().is_some() {
                // A jump to a specific message is pending for this room; let the
                // jump_target effect below fetch a timeline centered on it instead.
                return;
            }

            log::debug!("Loading room {}, resetting messages to empty", room_id);
            messages.set(Vec::new());
            has_more_top.set(true);
            has_more_bottom.set(true);
            is_loading_top.set(true);

            let current_room_id = room_id.clone();

            spawn_local(async move {
                match get_timeline(&current_room_id, None).await {
                    Ok(result) => {
                        if state.active_room_id_untracked() == Some(current_room_id.clone()) {
                            timeline_id.set(Some(result.timeline_id));
                            messages.set(result.messages.into_iter().map(RwSignal::new).collect());
                            is_loading_top.set(false);
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
                            if state.active_room_id_untracked() == Some(current_room_id) {
                                is_loading_top.set(false);
                            }
                        }
                    }
                }
            });
        }
    });

    let important_event_id: RwSignal<Option<String>> = expect_context();

    let ScrollTarget(scroll_target) = expect_context();

    let input_info: RwSignal<Option<ChatInputInfo>> = expect_context();
    Effect::new(move |_| {
        if input_info.get().is_none() {
            important_event_id.set(None);
        }
    });

    let scroll_to_event = Callback::new(move |event_id: String| {
        let Some(room_id) = state.active_room_id_untracked() else {
            log::error!("No active room ID, cannot scroll to event");
            return;
        };

        let element_id = format!("timeline-event-{event_id}");
        let options = ScrollIntoViewOptions::new();
        options.set_behavior(ScrollBehavior::Smooth);
        options.set_block(ScrollLogicalPosition::Center);

        if let Some(el) = document().get_element_by_id(&element_id) {
            log::debug!("Event {} already in DOM, scrolling into view", event_id);

            // Add a temporary border to the element via a tailwind css class, and remove it after 2 seconds. Have it animate
            el.class_list().add_1("animate-highlight").ok();

            set_timeout(
                move || {
                    if let Some(el) = document().get_element_by_id(&element_id) {
                        el.class_list().remove_1("animate-highlight").ok();
                    }
                },
                Duration::from_secs(4),
            );

            el.scroll_into_view_with_scroll_into_view_options(&options);
            return;
        }

        messages.set(Vec::new());
        has_more_top.set(true);
        has_more_bottom.set(true);
        is_loading_top.set(true);

        spawn_local(async move {
            log::debug!(
                "Fetching timeline around event {} in room {}",
                event_id,
                room_id
            );
            match get_timeline(&room_id, Some(event_id.clone())).await {
                Ok(result) => {
                    timeline_id.set(Some(result.timeline_id));
                    messages.set(result.messages.into_iter().map(RwSignal::new).collect());
                    is_loading_top.set(false);
                    is_loading_bottom.set(false);
                    scroll_target.set(Some(event_id.clone()));

                    log::debug!(
                        "Loaded focused timeline for event {} in room {}",
                        event_id,
                        room_id
                    );
                }
                Err(e) => {
                    log::error!("Failed to load timeline for scroll: {}", e);
                }
            }
        });
    });

    Effect::new(move |_| {
        if let Some(event_id) = jump_target.get() {
            jump_target.set(None);
            scroll_to_event.run(event_id);
        }
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse pb-8 [mask-image:linear-gradient(to_top,transparent_0%,black_2rem)]">
            <Show
                when=move || !is_loading_bottom.get()
                fallback=|| view! { <div class="text-center p-4 text-muted">"Loading..."</div> }
            >
                <div node_ref=sentinel_ref_bottom class="h-[1px] w-full shrink-0" />
            </Show>

            <For
                each=move || timeline.get()
                key=|(item_sig, _)| {
                    item_sig
                        .try_with_untracked(|item| { item.id.clone() })
                        .unwrap_or_else(|| "disposed_fallback_key".to_string())
                }
                children=move |(item_sig, show_header)| {
                    render_timeline_item(
                        item_sig,
                        show_header,
                        false,
                        scroll_to_event,
                        scroll_target,
                    )
                }
            />

            <Show
                when=move || !is_loading_top.get()
                fallback=|| view! { <div class="text-center p-4 text-muted">"Loading..."</div> }
            >
                <div node_ref=sentinel_ref_top class="h-[1px] w-full shrink-0" />
            </Show>
        </div>
    }
}

#[component]
fn TypingUserIndicator() -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let has_typing = Memo::new(move |_| {
        let Some(room_id) = state.active_room_id() else {
            return false;
        };
        let own_id = state.user_id.get();
        state
            .get_typing_users(&room_id)
            .get()
            .into_iter()
            .any(|id| id != own_id)
    });

    let visible_users: RwSignal<Vec<String>> = RwSignal::new(Vec::new());

    Effect::new(move |_| {
        let Some(room_id) = state.active_room_id() else {
            visible_users.set(Vec::new());
            return;
        };
        let own_id = state.user_id.get();
        let users: Vec<String> = state
            .get_typing_users(&room_id)
            .get()
            .into_iter()
            .filter(|id| id != &own_id)
            .take(4)
            .collect();

        if !users.is_empty() {
            visible_users.set(users);
        } else {
            set_timeout(
                move || visible_users.set(Vec::new()),
                Duration::from_millis(210),
            );
        }
    });

    let content = move || {
        let users = visible_users.get();
        if users.is_empty() {
            return None;
        }
        let room_id = state.active_room_id()?;

        let profiles: Vec<_> = users
            .into_iter()
            .map(|user_id| store.get_member_profile(&room_id, &user_id))
            .collect();

        let names = match profiles.len() {
            0 => return None,
            1 => {
                view! { <span>{profiles[0].get().render_name_popup("14px")} " is typing..."</span> }
                    .into_any()
            }
            2 => view! {
                <span>
                    {profiles[0].get().render_name_popup("14px")} " and "
                    {profiles[1].get().render_name_popup("14px")} " are typing..."
                </span>
            }
            .into_any(),
            3 => view! {
                <span>
                    {profiles[0].get().render_name_popup("14px")}", "
                    {profiles[1].get().render_name_popup("14px")} " and "
                    {profiles[2].get().render_name_popup("14px")} " are typing..."
                </span>
            }
            .into_any(),
            _ => view! { <span>"Several people are typing..."</span> }.into_any(),
        };

        Some(view! {
            <TypingIndicator size="8px" />
            <div class="pl-3">{names}</div>
        })
    };

    view! {
        <div class="pl-(--gap) h-8 -mt-8 w-full overflow-hidden shrink-0 pointer-events-none text-normal">
            <div
                class="h-8 w-full flex flex-row items-center px-3 [text-shadow:0_0_8px_var(--ui-solid-bg),0_0_4px_var(--ui-solid-bg)] transition-[transform,opacity] duration-200 ease-out"
                style=move || {
                    if has_typing.get() {
                        "transform: translateY(0); opacity: 1;"
                    } else {
                        "transform: translateY(100%); opacity: 0;"
                    }
                }
            >
                {content}
            </div>
        </div>
    }
}

#[derive(Clone, Copy, Default)]
pub struct ScrollTarget(pub RwSignal<Option<String>>);

/// Set to request jumping to a message in the chat (e.g. from search results), the
/// same way clicking a reply preview does: scroll to it if already loaded, otherwise
/// fetch a timeline centered on it.
#[derive(Clone, Copy, Default)]
pub struct JumpTarget(pub RwSignal<Option<String>>);

#[derive(Clone, Debug)]
pub enum ChatInputInfo {
    ReplyingTo { event_id: String, sender_id: String },
    Editing { event_id: String },
}

#[derive(Clone, Debug)]
pub struct Attachment {
    id: String,
    file_name: String,
    mime_type: String,
    size: u64,
    preview_url: Option<String>,
    source: UiAttachmentSource,
}

impl Attachment {
    pub fn from_klipy_file(url: String, file_name: String, size: u64) -> Self {
        let mime_type = if url.ends_with(".webp") {
            "image/webp"
        } else if url.ends_with(".gif") {
            "image/gif"
        } else {
            "application/octet-stream"
        };

        let source = UiAttachmentSource::Url(url.clone());

        Attachment {
            id: Uuid::new_v4().to_string(),
            file_name,
            mime_type: mime_type.to_string(),
            size,
            preview_url: Some(url),
            source,
        }
    }

    pub fn into_file_metadata(self) -> FileMetadata {
        FileMetadata {
            source: self.source.clone(),
            file_name: self.file_name.clone(),
            mime_type: self.mime_type.clone(),
            size: self.size,
        }
    }

    fn from_file_metadata(metadata: FileMetadata) -> Self {
        let preview_url = match metadata.source.clone() {
            UiAttachmentSource::LocalFile(path) => Some(convertFileSrc(&path)),
            UiAttachmentSource::RawBytes(_) => {
                log::warn!("Not implemented yet");
                todo!()
            }
            UiAttachmentSource::Url(url) => Some(url),
        };

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: metadata.file_name,
            mime_type: metadata.mime_type,
            size: metadata.size,
            preview_url,
            source: metadata.source,
        }
    }

    pub fn render_preview(&self, attachments: RwSignal<Vec<Attachment>>) -> impl IntoView {
        let id = self.id.clone();
        let name = self.file_name.clone();
        let size_str = format_bytes(self.size);
        let preview_url = self.preview_url.clone().unwrap_or_default();
        let is_image = self.mime_type.starts_with("image/");

        let mut ext = if name.contains('.') {
            name.split('.').next_back().unwrap_or("FILE").to_uppercase()
        } else {
            "FILE".to_string()
        };
        ext.truncate(4);

        let fallback_color = match ext.as_str() {
            "PDF" => "#ED4245",
            "ZIP" | "RAR" | "TAR" | "GZ" => "#FEE75C",
            "TXT" | "MD" | "RS" | "CSS" => "#5865F2",
            _ => "#4E5058",
        };

        view! {
            <div class="flex flex-col w-35 bg-(--ui-solid-bg) rounded-(--ui-border-radius) overflow-hidden border border-(--tile-border-color)">
                <div class="group relative">

                    {if is_image {
                        view! {
                            <img
                                src=preview_url
                                class="w-35 h-25 object-contain bg-black rounded-t-(--ui-border-radius) border-b border-(--tile-border-color)"
                            />
                        }
                            .into_any()
                    } else {
                        view! {
                            <div
                                class="flex items-center justify-center w-full h-full text-white font-extrabold tracking-wider p-2"
                                style=format!("background-color: {};", fallback_color)
                            >
                                <span>{ext}</span>
                            </div>
                        }
                            .into_any()
                    }}
                    <div
                        class="absolute top-1.5 right-1.5 rounded cursor-pointer flex items-center justify-center opacity-0 transition-all duration-150 ease-in-out group-hover:opacity-100 bg-(--ui-solid-bg) hover:bg-(--ui-solid-hover-bg) border border-(--tile-border-color) hover:border-(--accent-color) text-muted hover:text-bright"
                        on:click=move |_| {
                            let mut atts = attachments.get_untracked();
                            if let Some(pos) = atts.iter().position(|a| a.id == id) {
                                atts.remove(pos);
                                attachments.set(atts);
                            }
                        }
                    >
                        <Icon icon=TRASH size="16px" color="currentColor" />
                    </div>
                </div>

                <div class="p-2 flex flex-col gap-1">
                    <div class="text-dim text-xs font-medium truncate" title=name.clone()>
                        {name.clone()}
                    </div>
                    <div class="text-muted text-xs">{size_str}</div>
                </div>
            </div>
        }
    }
}

fn handle_paste(
    ev: web_sys::ClipboardEvent,
    attachments: RwSignal<Vec<Attachment>>,
    state: AppState,
) {
    use wasm_bindgen::JsCast;

    let Some(dt) = ev.clipboard_data() else {
        return;
    };

    // Non-WebKit path: items are populated (e.g. Chromium-based webviews, Firefox)
    let items = dt.items();
    if items.length() > 0 {
        let mut file_count = 0;
        for i in 0..items.length() {
            let Some(item) = items.get(i) else { continue };
            if item.kind() != "file" {
                continue;
            }
            let Ok(Some(file)) = item.get_as_file() else {
                continue;
            };
            file_count += 1;
            let name = file.name();
            let mime = file.type_();
            let size = file.size() as u64;
            let preview_url = web_sys::Url::create_object_url_with_blob(&file).ok();
            spawn_local(async move {
                let Ok(ab) = JsFuture::from(file.array_buffer()).await else {
                    return;
                };
                let bytes = js_sys::Uint8Array::new(&ab).to_vec();
                let att = Attachment {
                    id: uuid::Uuid::new_v4().to_string(),
                    file_name: name,
                    mime_type: mime,
                    size,
                    preview_url,
                    source: UiAttachmentSource::RawBytes(bytes),
                };
                attachments.update(|v| v.push(att));
                if let Some(room_id) = state.active_room_id() {
                    state.room_states.update(|d| {
                        d.entry(room_id).or_default().attachments = attachments.get_untracked();
                    });
                }
            });
        }
        if file_count > 0 {
            ev.prevent_default();
        }
        return;
    }

    // WebKitGTK path: items is always empty.
    // If there's text in the clipboard, let the browser's default paste handle it.
    let text = dt.get_data("text/plain").unwrap_or_default();
    if !text.is_empty() {
        return;
    }

    // No text — likely an image. Prevent the inline paste and read via the async Clipboard API.
    ev.prevent_default();

    spawn_local(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let clipboard = window.navigator().clipboard();
        let Ok(val) = JsFuture::from(clipboard.read()).await else {
            log::warn!("navigator.clipboard.read() failed");
            return;
        };

        let clip_items = js_sys::Array::from(&val);
        for i in 0..clip_items.length() {
            let item: web_sys::ClipboardItem = clip_items.get(i).unchecked_into();
            let types = item.types();
            for j in 0..types.length() {
                let mime = types.get(j).as_string().unwrap_or_default();
                if !mime.starts_with("image/") {
                    continue;
                }

                let Ok(blob_val) = JsFuture::from(item.get_type(&mime)).await else {
                    continue;
                };
                let blob: web_sys::Blob = blob_val.unchecked_into();
                let preview_url = web_sys::Url::create_object_url_with_blob(&blob).ok();

                let Ok(ab) = JsFuture::from(blob.array_buffer()).await else {
                    continue;
                };
                let bytes = js_sys::Uint8Array::new(&ab).to_vec();

                let ext = mime.split('/').nth(1).unwrap_or("png");
                let att = Attachment {
                    id: uuid::Uuid::new_v4().to_string(),
                    file_name: format!("file.{ext}"),
                    mime_type: mime,
                    size: bytes.len() as u64,
                    preview_url,
                    source: UiAttachmentSource::RawBytes(bytes),
                };
                attachments.update(|v| v.push(att));
                if let Some(room_id) = state.active_room_id() {
                    state.room_states.update(|d| {
                        d.entry(room_id).or_default().attachments = attachments.get_untracked();
                    });
                }
                break;
            }
        }
    });
}

#[component]
fn ChatInput() -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let menu = RwSignal::new(MenuType::None);
    let selected_index = RwSignal::new(0);

    provide_context(selected_index);

    let input_info: RwSignal<Option<ChatInputInfo>> = expect_context();

    let matches: RwSignal<MenuCompletionMatches> = RwSignal::new(MenuCompletionMatches::None);
    provide_context(matches);

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
                } else if let Some(filter) = get_active_filter(&el, caret_pos, '/') {
                    menu.set(MenuType::CommandAutocomplete { filter });
                } else if let Some(filter) = get_active_filter(&el, caret_pos, '#') {
                    menu.set(MenuType::RoomAutocomplete { filter });
                } else if let Some(filter) = get_active_filter(&el, caret_pos, ':') {
                    menu.set(MenuType::EmojiAutocomplete { filter });
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

    let is_empty = RwSignal::new(true);
    provide_context(is_empty);

    let attachments: RwSignal<Vec<Attachment>> = RwSignal::new(Vec::new());
    provide_context(attachments);

    // Load on room change
    Effect::new(move |_| {
        let room_id = state.active_room_id();
        let draft = room_id
            .and_then(|rid| state.room_states.with_untracked(|d| d.get(&rid).cloned()))
            .unwrap_or_default();

        let Some(el) = input_ref.get() else {
            return;
        };

        let content = draft.content;
        el.set_inner_html(&content);

        is_empty.set(content.is_empty() || &content == "<br>");
        attachments.set(draft.attachments);

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

    let important_event_id: RwSignal<Option<String>> = expect_context();
    let input_info_content = move || {
        let Some(info) = input_info.get() else {
            return ().into_any();
        };

        let content = match info {
            ChatInputInfo::ReplyingTo { sender_id, .. } => {
                let profile = store_clone
                    .get_member_profile(&state.active_room_id().unwrap_or_default(), &sender_id);
                view! {
                    <span class="text-sm text-bright">
                        "Replying to " {move || profile.get().render_name_popup("14px")}
                    </span>
                }
                .into_any()
            }
            ChatInputInfo::Editing { .. } => {
                view! { <span class="text-sm text-bright">"Editing message"</span> }.into_any()
            }
        };

        view! {
            <div class="w-full bg-(--ui-floating-bg) rounded-t-(--ui-border-radius) border border-(--tile-border-color) border-b-0 px-2 py-1 flex flex-row justify-between items-center">
                {content}
                <button
                    class="text-muted hover:text-bright cursor-pointer"
                    on:click=move |_| {
                        input_info.set(None);
                        important_event_id.set(None);
                    }
                >
                    <Icon icon=X_CIRCLE size="18px" color="currentColor" weight=IconWeight::Fill />
                </button>
            </div>
        }.into_any()
    };

    let attachment_view = move || {
        let atts = attachments.get();
        if atts.is_empty() {
            return ().into_any();
        }

        view! {
            <div
                class="w-full bg-(--ui-floating-bg) border border-(--tile-border-color) border-b-0 p-2 flex flex-row gap-2 overflow-x-auto"
                class=("rounded-t-(--ui-border-radius)", move || input_info.get().is_none())
            >
                {atts.iter().map(|att| att.render_preview(attachments)).collect_view()}
            </div>
        }.into_any()
    };

    let add_files_to_attachment = move || {
        spawn_local(async move {
            match pick_files().await {
                Ok(paths) => {
                    let mut new_atts = attachments.get_untracked();
                    for file in paths {
                        new_atts.push(Attachment::from_file_metadata(file));
                    }
                    attachments.set(new_atts);

                    if let Some(room_id) = state.active_room_id_untracked() {
                        let mut drafts = state.room_states.get_untracked();
                        let draft = drafts.entry(room_id).or_default();
                        draft.attachments = attachments.get_untracked();
                        state.room_states.set(drafts);
                    }
                }
                Err(e) => log::error!("File picking failed: {}", e),
            }
        });
    };

    let is_editing =
        Memo::new(move |_| matches!(input_info.get(), Some(ChatInputInfo::Editing { .. })));

    let emoji_state: EmojiPickerState = expect_context();
    let gif_state: GifPickerState = expect_context();

    let is_typing = RwSignal::new(false);
    let timing_timeout: StoredValue<Option<TimeoutHandle>> = StoredValue::new(None);

    let on_type = move || {
        let Some(room_id) = state.active_room_id() else {
            return;
        };

        if let Some(handle) = timing_timeout.get_value() {
            handle.clear();
        }

        let room_id_clone = room_id.clone();
        if !is_typing.get_untracked() {
            is_typing.set(true);
            spawn_local(async move {
                if let Err(e) = indicate_typing(&room_id_clone, true).await {
                    log::error!("Failed to send typing notification: {}", e);
                }
            });
        }

        let new_handle = set_timeout_with_handle(
            move || {
                is_typing.set(false);
                spawn_local(async move {
                    if let Err(e) = indicate_typing(&room_id, false).await {
                        log::error!("Failed to send typing notification: {}", e);
                    }
                });
            },
            Duration::from_secs(3),
        )
        .ok();

        timing_timeout.set_value(new_handle);
    };

    Effect::new(move |_| {
        if is_empty.get()
            && let Some(room_id) = state.active_room_id()
        {
            spawn_local(async move {
                if let Err(e) = indicate_typing(&room_id, false).await {
                    log::error!("Failed to send typing notification: {}", e);
                }
            });
        }
    });

    let is_focused = RwSignal::new(false);

    view! {
        <div class="p-2 pt-0 w-full relative">
            {move || input_info_content()} {move || attachment_view()}
            <SelectionMenu menu=menu input_ref=input_ref />
            <div
                class="text-normal w-full min-h-13 border rounded-b-(--ui-border-radius) flex flex-row bg-(--ui-solid-bg) items-center gap-3 px-3 cursor-text duration-100"
                class=(
                    "rounded-t-(--ui-border-radius)",
                    move || input_info.get().is_none() && attachments.get().is_empty(),
                )
                class=("border-(--focus-color)", move || is_focused.get())
                class=("border-(--tile-border-color)", move || !is_focused.get())
            >
                <button
                    class="text-(--ui-base-color) hover:text-normal rounded-(--ui-border-radius) hover:bg-(--ui-solid-hover-bg) p-1 transition-colors"
                    class=("cursor-not-allowed", move || is_editing.get())
                    class=("cursor-pointer", move || !is_editing.get())
                    on:click=move |_| {
                        if !is_editing.get() {
                            add_files_to_attachment()
                        }
                    }
                >
                    <Icon icon=UPLOAD_SIMPLE size="20px" />
                </button>
                <div class="relative flex-1 min-w-0 flex items-center">
                    <Show when=move || is_empty.get()>
                        <div class="text-muted absolute left-0 top-0 pointer-events-none select-none py-3">
                            "Type a message..."
                        </div>
                    </Show>
                    <div
                        node_ref=input_ref
                        contenteditable="true"
                        class="text-normal outline-none w-full whitespace-pre-wrap break-words py-3 max-h-100 overflow-y-auto"
                        on:input=move |_| {
                            on_type();
                            handle_input(input_ref, is_empty, state, attachments)
                        }
                        on:paste=move |ev| handle_paste(ev, attachments, state)
                        on:keydown=move |ev| handle_keydown(
                            ev,
                            input_ref,
                            state,
                            store.clone(),
                            (menu, selected_index, matches, is_empty, input_info, attachments),
                        )
                        on:focus=move |_| is_focused.set(true)
                        on:blur=move |_| is_focused.set(false)
                    ></div>
                </div>
                <button
                    class="text-(--ui-base-color) hover:text-normal rounded-(--ui-border-radius) hover:bg-(--ui-solid-hover-bg) p-1 cursor-pointer"
                    on:click=move |ev| {
                        let anchor: Element = ev.target().unwrap().unchecked_into();
                        spawn_local(async move {
                            let Some(string) = pick_gif(&anchor, gif_state).await else {
                                return;
                            };
                            if let Ok((url, name, size)) = serde_json::from_str::<
                                (String, String, u64),
                            >(&string) {
                                let attachment = Attachment::from_klipy_file(url, name, size);
                                attachments.update(|v| v.push(attachment));
                            }
                        });
                    }
                >
                    <Icon icon=GIF weight=IconWeight::Fill size="20px" />
                </button>
                <button
                    class="text-(--ui-base-color) hover:text-normal rounded-(--ui-border-radius) hover:bg-(--ui-solid-hover-bg) p-1 cursor-pointer"
                    on:click=move |ev| {
                        let anchor: Element = ev.target().unwrap().unchecked_into();
                        spawn_local(async move {
                            let Some(emoji) = pick_emoji(&anchor, emoji_state).await else {
                                return;
                            };
                            if let Some(el) = input_ref.get() {
                                insert_text_at_caret(&el, &emoji);
                                let input_event = web_sys::InputEvent::new("input").unwrap();
                                el.dispatch_event(&input_event).unwrap();
                            }
                        });
                    }
                >
                    <Icon icon=SMILEY size="20px" />
                </button>
            </div>
        </div>
    }
}

#[component]
pub fn Chat() -> impl IntoView {
    let state: AppState = expect_context();

    let chat_sidebar_open = RwSignal::new(matches!(
        state.active_section.get_untracked(),
        CurrentSection::Server(_)
    ));

    Effect::new({
        move |_| {
            chat_sidebar_open.set(matches!(
                state.active_section.get(),
                CurrentSection::Server(_)
            ))
        }
    });

    let input_info: RwSignal<Option<ChatInputInfo>> = RwSignal::new(None);
    provide_context(input_info);

    let important_event_id: RwSignal<Option<String>> = RwSignal::new(None);
    let scroll_target: RwSignal<Option<String>> = RwSignal::new(None);
    let jump_target: RwSignal<Option<String>> = RwSignal::new(None);

    provide_context(important_event_id);
    provide_context(ScrollTarget(scroll_target));
    provide_context(JumpTarget(jump_target));

    let search_parameters: RwSignal<Option<SearchParameters>> = RwSignal::new(None);
    let search_results: RwSignal<Option<HashMap<String, Vec<UiTimelineItem>>>> =
        RwSignal::new(None);

    provide_context(search_parameters);
    provide_context(search_results);

    let pinned_result: RwSignal<Option<Vec<UiTimelineItem>>> = RwSignal::new(None);

    provide_context(pinned_result);

    Effect::new(move |_| {
        let search_params = search_parameters.get();
        let search_results = search_results.get();
        let pinned_result = pinned_result.get().is_some().then_some(Vec::new());

        if let Some(active_room_id) = state.active_room_id_untracked() {
            state.room_states.update(|drafts| {
                let entry = drafts.entry(active_room_id.clone()).or_default();
                entry.search_parameters = search_params;
                entry.search_results = search_results;
                entry.pinned_result = pinned_result;
            });
        }
    });

    // Load the active room's search draft whenever the active room changes.
    Effect::new(move |_| {
        let draft = state
            .active_room_id()
            .and_then(|rid| state.room_states.with_untracked(|d| d.get(&rid).cloned()))
            .unwrap_or_default();

        pinned_result.set(draft.pinned_result);
        search_parameters.set(draft.search_parameters);
        search_results.set(draft.search_results);
    });

    let search_update_sig: ReadSignal<Option<SearchResultUpdate>> = use_tauri_event();
    let seen_search_rooms: StoredValue<(Uuid, HashSet<String>)> =
        StoredValue::new((Uuid::nil(), HashSet::new()));
    setup_update_effect(
        search_update_sig,
        move |(result_search_id, room_id, update)| {
            if let Some(SearchParameters { search_id, .. }) = search_parameters.get_untracked()
                && search_id == result_search_id
            {
                // The first batch a search returns for a room replaces the room's
                // results from the previous search, later batches extend them.
                let first_batch = seen_search_rooms
                    .with_value(|(id, rooms)| *id != result_search_id || !rooms.contains(&room_id));
                seen_search_rooms.update_value(|(id, rooms)| {
                    if *id != result_search_id {
                        *id = result_search_id;
                        rooms.clear();
                    }
                    rooms.insert(room_id.clone());
                });

                search_results.update(|res| {
                    if let Some(res) = res {
                        let room_results = res.entry(room_id.clone()).or_default();
                        if first_batch {
                            room_results.clear();
                        }
                        room_results.extend(update);
                    }
                });

                if let Some(active_room_id) = state.active_room_id_untracked() {
                    state.room_states.update(|drafts| {
                        drafts.entry(active_room_id).or_default().search_results =
                            search_results.get_untracked();
                    });
                }
            }
        },
    );

    view! {
        <div class="flex-1 h-full flex gap-[var(--gap)] flex-col overflow-hidden">
            <ChatHeader chat_sidebar_open=chat_sidebar_open />
            <div class="flex flex-row h-full min-h-0">
                <FloatingTile class="flex-1 flex flex-col h-full min-h-0 overflow-hidden">
                    {move || match state.active_room.get() {
                        None => {
                            view! {
                                <div class="flex-1 flex items-center justify-center text-muted">
                                    "No room selected"
                                </div>
                            }
                                .into_any()
                        }
                        Some(node) => {
                            match &node {
                                RoomNode::Dm(_)
                                | RoomNode::TextChannel(_)
                                | RoomNode::Single(_) => {
                                    let Some(room_id) = state.active_room_id() else {
                                        return ().into_any();
                                    };
                                    let call_members = state.get_call_members(&room_id).get();
                                    let node = node.clone();

                                    view! {
                                        <Show when=move || !call_members.is_empty()>
                                            <div class="h-50 w-full border-(--tile-border-color) border-b">
                                                <CallView node=node.clone() />
                                            </div>
                                        </Show>
                                        <TimeLine />
                                        <TypingUserIndicator />
                                        <ChatInput />
                                    }
                                        .into_any()
                                }
                                RoomNode::VoiceChannel(_) => {
                                    view! {
                                        <div class="flex flex-row gap-(--gap) h-full w-full">
                                            <div class="flex flex-1 h-full border-(--tile-border-color) border-r">
                                                <CallView node=node />
                                            </div>
                                            <div class="flex flex-col h-full min-h-0 w-100">
                                                <TimeLine />
                                                <TypingUserIndicator />
                                                <ChatInput />
                                            </div>
                                        </div>
                                    }
                                        .into_any()
                                }
                                RoomNode::Space(_) => {
                                    view! {
                                        <div class="flex-1 flex items-center justify-center text-muted">
                                            "Spaces are not supported yet"
                                        </div>
                                    }
                                        .into_any()
                                }
                                RoomNode::Server(_) => {
                                    view! {
                                        <div class="flex-1 flex items-center justify-center text-muted">
                                            "Servers are not supported yet"
                                        </div>
                                    }
                                        .into_any()
                                }
                                RoomNode::Unjoined(_) => {
                                    view! {
                                        <div class="flex-1 flex items-center justify-center text-muted">
                                            "You have not joined this room"
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                        }
                    }}
                </FloatingTile>
                <ChatSideBar chat_sidebar_open=chat_sidebar_open />
            </div>
        </div>
    }
}
