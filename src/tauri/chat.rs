use crate::app::{call_tauri, AppState, MemberStore};
use crate::components::FloatingTile;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use leptos::html::Div;
use leptos::task::spawn_local;
use leptos::{leptos_dom::logging::console_error, prelude::*};
use leptos_use::{use_intersection_observer, UseIntersectionObserverReturn};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
pub struct ChatMessage {
    room_id: String,
    msg_type: String,
    id: String,
    ts: i64,
    raw_json: String,
    sender: String,
}

#[derive(PartialEq, Clone)]
enum SystemMessage {
    EncryptionEnabled,
    EncryptionDisabled,
    UserJoined,
    UserLeft,
    CallStarted,
    CallEnded,
    Other(String),
}

#[derive(PartialEq, Clone)]
struct TimelineMessageGroup {
    contents: Vec<String>,
}

#[derive(PartialEq, Clone)]
enum TimelineItemKind {
    MessageGroup(TimelineMessageGroup),
    DateSeparator,
    SystemMessage(SystemMessage),
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
        let state = expect_context::<AppState>();
        let store = expect_context::<MemberStore>();

        let profile_sig = if let Some(room_id) = state.active_room_id.get() {
            store.get_profile(&room_id, &self.sender)
        } else {
            return view! {}.into_any();
        };
        let name_sig = profile_sig.clone();

        match &self.kind {
            TimelineItemKind::MessageGroup(group) => view! {
                <div class="flex gap-3 p-3 rounded-md hover:bg-black/20">
                    <div class="shrink-0">
                    {move || {
                        let p = profile_sig.get();
                        let initial = p.clone().display_name.unwrap_or_default().chars().next().unwrap_or('?').to_string();

                        match p.avatar_url {
                            Some(url) => view! {
                                <img src=url class="w-10 h-10 rounded-full object-cover bg-transparent" alt=initial />
                            }.into_any(),
                            None => view! {
                                <div class="w-10 h-10 rounded-full bg-gray-600 flex items-center justify-center text-white font-bold">
                                    {initial}
                                </div>
                            }.into_any()
                        }
                    }}
                    </div>
                    <div class="flex flex-col min-w-0">
                        <div class="flex items-baseline gap-2">
                            <span class="text-bright truncate hover:underline cursor-pointer">
                                {move || name_sig.get().display_name}
                            </span>
                            <span class="text-muted text-xs">
                                {format_date(self.date)}
                            </span>
                        </div>
                        <div class="flex flex-col gap-1">
                            {group.contents.iter().map(|content| {
                                view! {
                                    <div class="text-normal leading-relaxed break-words">
                                        {content.clone()}
                                    </div>
                                }.into_any()
                            }).collect_view()}
                        </div>
                    </div>
                </div>
            }
            .into_any(),
            TimelineItemKind::DateSeparator => view! {
                <div class="flex items-center gap-2 my-4">
                    <div class="flex-1 border-t border-[var(--tile-border-color)]"></div>
                    <span class="text-muted text-sm">
                        {self.date.format("%d %B %Y").to_string()}
                    </span>
                    <div class="flex-1 border-t border-[var(--tile-border-color)]"></div>
                </div>
            }
            .into_any(),
            TimelineItemKind::SystemMessage(sys_msg) => {
                let text = match sys_msg {
                    SystemMessage::EncryptionEnabled => "Encryption enabled",
                    SystemMessage::EncryptionDisabled => "Encryption disabled",
                    SystemMessage::UserJoined => "User joined the room",
                    SystemMessage::UserLeft => "User left the room",
                    SystemMessage::CallStarted => "Call started",
                    SystemMessage::CallEnded => "Call ended",
                    SystemMessage::Other(string) => string.as_str(),
                };

                view! {
                    <div class="flex items-center justify-center my-2">
                        <span class="text-muted text-sm italic">
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

#[derive(Serialize)]
struct FetchMessagesRequest {
    room_id: String,
    oldest_id: Option<String>,
}

#[component]
fn TimeLine() -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());

    let flattened_items = Memo::new(move |_| {
        let msgs = messages.get();
        let mut items: Vec<TimelineItem> = Vec::new();
        let mut last_day: Option<NaiveDate> = None;

        for msg in msgs.iter().rev() {
            let current_date = get_date_from_ts(msg.ts);
            let current_day = current_date.date_naive();

            let maybe_item = match msg.msg_type.as_str() {
                "m.room.message" => {
                    let content = msg.raw_json.clone();

                    let mut grouped = false;
                    if let Some(last_item) = items.last_mut() {
                        if let TimelineItemKind::MessageGroup(ref mut group) = last_item.kind {
                            let same_sender = last_item.sender == msg.sender;
                            let same_minute = (current_date.timestamp() / 60)
                                == (last_item.date.timestamp() / 60);

                            if same_sender && same_minute {
                                group.contents.push(content.clone());
                                grouped = true;
                            }
                        }
                    }

                    if !grouped {
                        Some(TimelineItem {
                            date: current_date,
                            sender: msg.sender.clone(),
                            id: msg.id.clone(),
                            kind: TimelineItemKind::MessageGroup(TimelineMessageGroup {
                                contents: vec![content],
                            }),
                        })
                    } else {
                        None
                    }
                }
                "m.room.encrypted" => Some(TimelineItem {
                    date: current_date,
                    sender: msg.sender.clone(),
                    id: msg.id.clone(),
                    kind: TimelineItemKind::MessageGroup(TimelineMessageGroup {
                        contents: vec!["Failed to decrypt message".to_string()],
                    }),
                }),
                "m.call.member" => Some(TimelineItem {
                    date: current_date,
                    sender: msg.sender.clone(),
                    id: msg.id.clone(),
                    kind: TimelineItemKind::SystemMessage(SystemMessage::CallStarted),
                }),
                // "m.room.encryption" => Some(TimelineItem {
                //     date: current_date,
                //     sender: msg.sender.clone(),
                //     id: msg.id.clone(),
                //     kind: TimelineItemKind::SystemMessage(SystemMessage::Other(
                //         msg.raw_json.clone(),
                //     )),
                // }),
                _ => None,
            };

            if let Some(item) = maybe_item {
                if Some(current_day) != last_day {
                    items.push(TimelineItem {
                        date: current_date,
                        sender: String::new(),
                        id: format!("date-sep-{}", current_day),
                        kind: TimelineItemKind::DateSeparator,
                    });
                    last_day = Some(current_date.date_naive());
                }

                items.push(item);
            } else {
                console_error(&format!("Unknown message type: {}", msg.msg_type));
            }
        }
        items.sort_by_key(|item| item.date);
        items.reverse();
        items
    });

    let (is_loading, set_is_loading) = signal(false);
    let (has_more, set_has_more) = signal(true);
    let (initial_loaded, set_initial_loaded) = signal(false);

    let state = expect_context::<AppState>();
    let fetch_more = move |_: ()| {
        if is_loading.get_untracked() {
            console_error("Already loading, skipping...");
            return;
        }
        if !has_more.get_untracked() {
            console_error("No more messages to load, skipping...");
            return;
        }

        let room_id = state.active_room_id.get_untracked();
        if room_id.is_none() {
            console_error("No active room ID, skipping...");
            return;
        }

        set_is_loading.set(true);

        spawn_local(async move {
            let result = async {
                let rid = room_id?; // Safe return if None
                let oldest_id = messages
                    .get_untracked()
                    .iter()
                    .min_by_key(|m| m.ts)
                    .map(|m| m.id.clone());

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
                    serde_wasm_bindgen::from_value::<(Vec<ChatMessage>, bool)>(js_val)
                {
                    set_has_more.set(has_more);

                    set_messages.update(|existing| {
                        let mut seen_ids: std::collections::HashSet<String> =
                            existing.iter().map(|m| m.id.clone()).collect();

                        let mut unique_new: Vec<ChatMessage> = new_messages
                            .into_iter()
                            .filter(|m| seen_ids.insert(m.id.clone()))
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

            set_timeout(
                move || {
                    fetch_more(());
                },
                std::time::Duration::from_millis(1),
            );
        }
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse p-4 overflow-anchor-auto">
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
pub fn Chat(
    messages: ReadSignal<Vec<ChatMessage>>,
    set_messages: WriteSignal<Vec<ChatMessage>>,
) -> impl IntoView {
    view! {
        <div class="flex-1 h-full flex gap-3 flex-col overflow-hidden">
            <FloatingTile>
                "Chat name goes here"
            </FloatingTile>
            <div class="flex gap-3 flex-row h-full min-h-0">
                <FloatingTile class="flex-1 min-h-0, overflow-hidden">
                    <TimeLine/>
                    <input
                        type="text"
                        placeholder="Type a message..."
                               class="w-full h-15 border-1 border-[var(--tile-border-color)] bg-[rgba(0, 0, 0, 1)] outline-none text-[var(--text-color)]"
                    />
                </FloatingTile>
                <FloatingTile class="hidden xl:flex w-90 p-2 h-full">
                    "Chat info goes here"
                </FloatingTile>
            </div>
        </div>
    }
}
