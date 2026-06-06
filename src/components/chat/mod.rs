use crate::{
    app::{call_tauri, convertFileSrc, format_bytes},
    components::{
        FloatingTile,
        chat::messages::render_timeline_item,
        emoji_picker::EmojiPickerState,
        input::{
            get_active_filter, get_caret_position, handle_input, handle_keydown,
            insert_text_at_caret,
            menu::{MenuType, SelectionMenu},
        },
        presence::PresenceBadge,
        user_profile::{MemberProfileExt},
    },
    hooks::use_tauri_event,
    state::{AppState, ProfileStore, RoomHeader},
    tauri_functions::{get_timeline, pick_files, scroll_up},
};

use crate::components::emoji_picker::pick_emoji;
use phosphor_leptos::{
    HASH, INFO, Icon, IconWeight, MATRIX_LOGO, PHONE, PHONE_DISCONNECT, SMILEY, SPEAKER_HIGH,
    TRASH, UPLOAD_SIMPLE, X_CIRCLE,
};

use leptos::{ev, html::Div, prelude::*, task::spawn_local};
use leptos_use::{UseIntersectionObserverReturn, use_event_listener, use_intersection_observer};
use shared::{
    api::{FileMetadata, UiAttachmentSource}, profile::{MemberProfile, PresenceInfo}, sidebar::RoomKind, timeline::{DetailState, EventContent, UiTimelineDiff, UiTimelineItem, UiTimelineItemKind}
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Element, IntersectionObserverEntry};

pub(crate) mod messages;

#[component]
fn TimeLine() -> impl IntoView {
    let state: AppState = expect_context();

    let messages_update_event: ReadSignal<Option<Vec<UiTimelineDiff>>> =
        use_tauri_event("timeline_update");

    let messages: RwSignal<Vec<RwSignal<UiTimelineItem>>> = RwSignal::new(Vec::new());

    let owner = Owner::current().expect("TimeLine must have an owner");

    Effect::new(move |_| {
        let Some(diffs) = messages_update_event.get() else {
            return;
        };

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
        let Some(room_id) = state.active_room_id_untracked() else {
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

    // `active_room_id` is its own signal that only changes when the active room
    // actually changes (not on per-sync RoomNode metadata churn), so this Effect
    // reloads the timeline only on a real room switch.
    Effect::new(move |_| {
        if let Some(room_id) = state.active_room_id() {
            log::debug!("Loading room {}, resetting messages to empty", room_id);
            messages.set(Vec::new());
            initial_loaded.set(false);
            has_more.set(true);
            is_loading.set(true);

            let current_room_id = room_id.clone();

            spawn_local(async move {
                match get_timeline(&current_room_id).await {
                    Ok(tl) => {
                        if state.active_room_id_untracked() == Some(current_room_id.clone()) {
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
                            if state.active_room_id_untracked() == Some(current_room_id) {
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
    let member_store: ProfileStore = expect_context();
    let state: AppState = expect_context();

    let (info_hovered, set_info_hovered) = signal(false);

    view! {
        <FloatingTile class="h-(--header-height) items-start flex-row gap-1 pl-[5px]">
            <div class="w-8 self-center flex items-center justify-center">
                {move || {
                    let clone = member_store.clone();
                    match header.get() {
                        RoomHeader::TextChannel(_) => {
                            view! {
                                <div class="text-(--ui-base-color) w-full justify-center flex">
                                    <Icon icon=HASH color="currentColor" size="70%" />
                                </div>
                            }
                                .into_any()
                        }
                        RoomHeader::VoiceChannel(_) => {
                            view! {
                                <div class="text-(--ui-base-color) w-full justify-center flex">
                                    <Icon icon=SPEAKER_HIGH color="currentColor" size="70%" />
                                </div>
                            }
                                .into_any()
                        }
                        RoomHeader::DM(profile_sig) => {
                            {
                                view! {
                                    {move || {
                                        let profile = profile_sig.get();
                                        let presence = clone
                                            .clone()
                                            .get_presence(profile.user_id());
                                        view! {
                                            <PresenceBadge presence=presence size=14.0>
                                                {profile.render_icon("30px")}
                                            </PresenceBadge>
                                        }
                                            .into_any()
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
                        RoomHeader::Space(_) => {
                            view! {
                                <div class="text-(--ui-base-color) w-full justify-center flex">
                                    <Icon icon=MATRIX_LOGO color="currentColor" size="70%" />
                                </div>
                            }
                                .into_any()
                        }
                    }
                }}
            </div>
            <div class="flex-1 flex flex-col self-center text-bright text-m font-semibold">
                {move || match header.get() {
                    RoomHeader::TextChannel(name) => {
                        view! { <span>{name.clone()}</span> }.into_any()
                    }
                    RoomHeader::VoiceChannel(name) => {
                        view! { <span>{name.clone()}</span> }.into_any()
                    }
                    RoomHeader::DM(profile_sig) => {
                        { view! { {move || profile_sig.get().render_name("16px")} }.into_any() }
                            .into_any()
                    }
                    RoomHeader::Unknown => view! { <span>"Unknown Room"</span> }.into_any(),
                    RoomHeader::Space(name) => view! { <span>{name.clone()}</span> }.into_any(),
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
                    on:click=move |_| {
                        let value = serde_wasm_bindgen::to_value(
                            &serde_json::json!({"room_id": &state.active_room_id().unwrap()}),
                        );
                        spawn_local(async move {
                            log::debug!(
                                "{:?}", call_tauri("join_matrixrtc_call", value.unwrap()).await
                            );
                        })
                    }
                    on:mouseenter=move |_| set_info_hovered.set(true)
                    on:mouseleave=move |_| set_info_hovered.set(false)
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
            <div class="self-center h-full">
                <button
                    class="transition-opacity h-full mr-1"
                    class=("text-(--ui-hover-color)", move || info_hovered.get())
                    class=("text-(--ui-base-color)", move || !info_hovered.get())
                    on:click=move |_| {
                        let value = serde_wasm_bindgen::to_value(
                            &serde_json::json!({"room_id": &state.active_room_id().unwrap()}),
                        );
                        spawn_local(async move {
                            log::debug!(
                                "{:?}", call_tauri("leave_matrixrtc_call", value.unwrap()).await
                            );
                        })
                    }
                    on:mouseenter=move |_| set_info_hovered.set(true)
                    on:mouseleave=move |_| set_info_hovered.set(false)
                >
                    <div class="h-full justify-center items-center flex cursor-pointer">
                        <Icon
                            icon=PHONE_DISCONNECT
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
    ReplyingTo {
        event_id: String,
        sender_id: String,
        item_id: String,
    },
    Editing {
        event_id: String,
        item_id: String,
    },
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
                    state.drafts.update(|d| {
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
                    state.drafts.update(|d| {
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
    let input_info = RwSignal::new(None);

    provide_context(selected_index);
    provide_context(input_info);

    let mention_matches = RwSignal::new(Vec::new());
    let command_matches = RwSignal::new(Vec::new());
    let room_matches = RwSignal::new(Vec::new());

    provide_context(mention_matches);
    provide_context(command_matches);
    provide_context(room_matches);

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
        state.active_room_id();

        if let Some(el) = input_ref.get() {
            let _ = el.focus();
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
            .and_then(|rid| state.drafts.with_untracked(|d| d.get(&rid).cloned()))
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
                        "Replying to " {move || profile.get().render_name("14px")}
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
                    on:click=move |_| input_info.set(None)
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
                        let mut drafts = state.drafts.get_untracked();
                        let draft = drafts.entry(room_id).or_default();
                        draft.attachments = attachments.get_untracked();
                        state.drafts.set(drafts);
                    }
                }
                Err(e) => log::error!("File picking failed: {}", e),
            }
        });
    };

    let is_editing =
        Memo::new(move |_| matches!(input_info.get(), Some(ChatInputInfo::Editing { .. })));

    let emoji_state: EmojiPickerState = expect_context();

    view! {
        <div class="p-2 pt-0 w-full relative">
            {move || input_info_content()} {move || attachment_view()}
            <SelectionMenu menu=menu input_ref=input_ref />
            <div
                class="text-(--bright-text-color) w-full min-h-13 border-1 border-(--tile-border-color) rounded-b-(--ui-border-radius) bg-[rgba(0, 0, 0, 0.6)] flex flex-row bg-(--ui-floating-bg) items-center gap-3 px-3 cursor-text"
                class=(
                    "rounded-t-(--ui-border-radius)",
                    move || input_info.get().is_none() && attachments.get().is_empty(),
                )
            >
                <button
                    class="text-(--ui-base-color) hover:text-(--bright-text-color) rounded-(--ui-border-radius) hover:bg-(--ui-solid-hover-bg) p-1"
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
                        class="text-(--bright-text-color) outline-none w-full whitespace-pre-wrap break-words py-3 max-h-100 overflow-y-auto"
                        on:input=move |_| handle_input(input_ref, is_empty, state, attachments)
                        on:paste=move |ev| handle_paste(ev, attachments, state)
                        on:keydown=move |ev| handle_keydown(
                            ev,
                            input_ref,
                            state,
                            store.clone(),
                            (
                                menu,
                                selected_index,
                                mention_matches,
                                command_matches,
                                room_matches,
                                is_empty,
                                input_info,
                                attachments,
                            ),
                        )
                    ></div>
                </div>
                <button
                    class="text-(--ui-base-color) hover:text-(--bright-text-color) rounded-(--ui-border-radius) hover:bg-(--ui-solid-hover-bg) p-1 cursor-pointer"
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
    let member_store: ProfileStore = expect_context();

    let header = Memo::new({
        let member_store = member_store.clone();
        move |_| state.get_room_header(member_store.clone())
    });

    let (chat_sidebar_open, set_chat_sidebar_open) = signal(true);

    let room_id = move || state.active_room_id().unwrap_or_default();

    let participants = Memo::new(move |_| state.get_call_members(&room_id()).get());

    view! {
        <div class="flex-1 h-full flex gap-[var(--gap)] flex-col overflow-hidden">
            <ChatHeader
                header=header
                chat_sidebar_open=chat_sidebar_open
                set_chat_sidebar_open=set_chat_sidebar_open
            />
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
                            match &node.kind {
                                RoomKind::Dm { .. } | RoomKind::TextChannel => {
                                    view! {
                                        <TimeLine />
                                        <ChatInput />
                                    }
                                        .into_any()
                                }
                                RoomKind::VoiceChannel => {
                                    let participants = participants.get();
                                    if participants.is_empty() {
                                        view! {
                                            <div class="flex-1 flex items-center justify-center text-muted flex-col gap-2 bg-radial-[at_50%_100%] from-(--accent-color) to-transparent to-80% w-full h-full">
                                                <span class="text-3xl text-bright font-bold text-shadow-xs">
                                                    {node.get_name()}
                                                </span>
                                                <span class="text-muted">
                                                    "No one is currently in this voice channel"
                                                </span>
                                            </div>
                                        }
                                            .into_any()
                                    } else {
                                        let count = participants.len();
                                        let width_class = match count {
                                            1 => "w-full max-w-5xl",
                                            2 => "w-[calc(50%-0.5*var(--gap))] max-w-3xl",
                                            3 | 4 => "w-[calc(50%-0.5*var(--gap))] max-w-2xl",
                                            5..=6 => "w-[calc(33.33%-0.66*var(--gap))] max-w-xl",
                                            7..=9 => "w-[calc(33.33%-0.66*var(--gap))] max-w-lg",
                                            10..=12 => "w-[calc(25%-0.75*var(--gap))] max-w-md",
                                            _ => "w-[calc(20%-0.8*var(--gap))] max-w-sm",
                                        };

                                        view! {
                                            <div class="flex-1 flex flex-wrap justify-center content-center w-full h-full min-h-0 gap-[var(--gap)] p-[var(--gap)] overflow-y-auto">
                                                {participants
                                                    .iter()
                                                    .map(|device| {
                                                        let profile = member_store
                                                            .get_member_profile(&node.room_id, &device.user_id);
                                                        let clone = profile.clone();
                                                        let colors = move || {
                                                            let mut color = clone.get().get_color();
                                                            let fg_color = color.clone().to_css_string();
                                                            color.set_lightness(10.0);
                                                            format!(
                                                                "background-color: {}; box-shadow: inset 0 0 20px 0px {};",
                                                                color.to_css_string(),
                                                                fg_color,
                                                            )
                                                        };
                                                        let clone = profile.clone();
                                                        view! {
                                                            <div
                                                                class=format!(
                                                                    "{} aspect-video rounded-2xl flex flex-col items-center justify-center overflow-hidden transition-all duration-300 rounded-3xl",
                                                                    width_class,
                                                                )
                                                                style=colors
                                                            >

                                                                // Discord-like Avatar Placeholder
                                                                {move || profile.get().render_icon("64px")}
                                                                {move || clone.get().render_name("16px")}
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        }
                                            .into_any()
                                    }
                                }
                                RoomKind::Space { .. } => {
                                    view! {
                                        <div class="flex-1 flex items-center justify-center text-muted">
                                            "Spaces are not supported yet"
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                        }
                    }}
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
    let member_store: ProfileStore = expect_context();

    let content = move || {
        let store_clone = member_store.clone();

        match header.get() {
            RoomHeader::DM(profile_sig) => {
                let banner_color = profile_sig
                    .get().get_color().to_css_string();
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
                                let profile = profile_sig_icon.get();
                                let presence = store_clone.get_presence(profile.user_id());
                                let size_str = format!("{icon_size}px");
                                view! {
                                    <PresenceBadge presence=presence size=25.0>
                                        {profile.render_icon(size_str)}
                                    </PresenceBadge>
                                }
                                    .into_any()
                            }}
                        </div>

                        <div class="px-4 pt-10 pb-6">
                            <h2 class="text-xl font-bold text-bright">
                                {move || profile_sig_name.get().render_name("16px")}
                            </h2>
                            <p class="text-sm text-muted">"Direct Message"</p>
                        </div>
                    </div>
                }
                .into_any()
            }
            RoomHeader::TextChannel(_) => view! { <MemberList /> }.into_any(),
            RoomHeader::VoiceChannel(_) => view! { <MemberList /> }.into_any(),
            RoomHeader::Space(_) => view! { <MemberList /> }.into_any(),
            RoomHeader::Unknown => view! {
                <div class="flex-1 flex items-center justify-center text-muted">
                    "No information available for this room"
                </div>
            }
            .into_any(),
        }
    };

    view! { <div class="flex flex-col w-full overflow-visible">{content}</div> }
}

#[component]
fn MemberList() -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let room_id = state.active_room_id_untracked().unwrap_or_default();

    let members_store = store.clone();
    let members = Memo::new(move |_| members_store.clone().get_member_signals(&room_id));

    let online_store = store.clone();
    let online_view = move || {
        let members = members.get();

        let mut elements: Vec<(String, ArcRwSignal<MemberProfile>, ArcRwSignal<PresenceInfo>)> = members.into_iter().filter_map(|(user_id, member_sig)| {
            let presence = online_store.get_presence(&user_id);

            if !presence.get().is_offline() {
                let name = member_sig.get().get_name();
                Some((name, member_sig, presence))
            } else {
                None
            }
        }).collect();

        elements.sort_by_key(|v| v.0.clone());

        let views: Vec<_> = elements.into_iter().map(|(_, member_sig, presence)| {
            let profile = member_sig.get();
            let name_profile = profile.clone();

            view! {
                <div class="flex items-center gap-2">
                    <PresenceBadge presence=presence size=15.5>
                        {profile.render_icon("32px")}
                    </PresenceBadge>
                    <span class="text-bright">{name_profile.render_name("16px")}</span>
                </div>
            }
        }).collect();

        let online_i = views.len();

        let number_view = view! { <span class="text-sm text-muted">{format!("{} online", online_i)}</span> }
                .into_any();

        if online_i > 0 {
            view! {
                <div>
                    {number_view} <div class="flex flex-col gap-2 mt-2">{views.collect_view()}</div>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }
    };

    let offline_store = store.clone();
    let offline_view = move || {
        let members = members.get();

        let mut elements: Vec<(String, ArcRwSignal<MemberProfile>, ArcRwSignal<PresenceInfo>)> = members.into_iter().filter_map(|(user_id, member_sig)| {
            let presence = offline_store.get_presence(&user_id);

            if presence.get().is_offline() {
                let name = member_sig.get().get_name();
                Some((name, member_sig, presence))
            } else {
                None
            }
        }).collect();

        elements.sort_by_key(|v| v.0.clone());

        let views: Vec<_> = elements.into_iter().map(|(_, member_sig, presence)| {
            let profile = member_sig.get();
            let name_profile = profile.clone();

            view! {
                <div class="flex items-center gap-2">
                    <PresenceBadge presence=presence size=15.5>
                        {profile.render_icon("32px")}
                    </PresenceBadge>
                    <span class="text-bright">{name_profile.render_name("16px")}</span>
                </div>
            }
        }).collect();

        let offline_i = views.len();

        let number_view = view! { <span class="text-sm text-muted">{format!("{} offline", offline_i)}</span> }
                .into_any();

        if offline_i > 0 {
            view! {
                <div>
                    {number_view} <div class="flex flex-col gap-2 mt-2">{views.collect_view()}</div>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }
    };

    let header = move || {
        let members = members.get();

        let mut online_count = 0;
        let mut offline_count = 0;

        for member in members.keys() {
            let presence = store.get_presence(member);
            if !presence.get().is_offline() {
                online_count += 1;
            } else {
                offline_count += 1;
            }
        }

        view! {
            <div class="flex items-center gap-2 justify-center">
                <div class="w-3 h-3 rounded-full bg-(--online-color)"></div>
                <span class="text-ms text-(--online-color) pr-5">{online_count}</span>
                <div class="w-3 h-3 rounded-full bg-(--offline-color)"></div>
                <span class="text-ms text-(--offline-color)">{offline_count}</span>
            </div>
        }
    };

    view! { <div class="flex flex-col gap-2 p-3">{header} {online_view} {offline_view}</div> }
}
