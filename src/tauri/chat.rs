use std::collections::HashMap;

use crate::app::{AppState, MemberStore};
use crate::components::FloatingTile;
use crate::hooks::use_tauri_event;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use leptos::html::Div;
use leptos::leptos_dom::logging::console_error;
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct ChatMessage {
    room_id: String,
    msg_type: String,
    id: String,
    ts: i64,
    content: Option<String>,
    raw_json: String,
    sender: String,
}

fn format_matrix_ts(ts: i64) -> String {
    console_error(&format!("{}", ts));
    let datetime = chrono::Local
        .timestamp_opt(ts, 0)
        .latest()
        .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH.with_timezone(&chrono::Local));

    datetime.format("%d/%m/%Y, %H:%M").to_string()
}

#[derive(PartialEq, Clone)]
enum SystemMessage {
    EncryptionEnabled,
    EncryptionDisabled,
    UserJoined,
    UserLeft,
    CallStarted,
    CallEnded,
}

#[derive(PartialEq, Clone)]
struct TimelineMessage {
    content: String,
}

#[derive(PartialEq, Clone)]
enum TimelineItemKind {
    Message(TimelineMessage),
    DateSeparator,
    SystemMessage(SystemMessage),
}

#[derive(PartialEq, Clone)]
struct TimelineItem {
    date: DateTime<Local>,
    sender: String,

    kind: TimelineItemKind,
}

impl TimelineItem {
    fn render(&self) -> impl IntoView {
        match &self.kind {
            TimelineItemKind::Message(msg) => view! {
                <div class="flex gap-3 p-3 rounded-md transition-colors hover:bg-[rgba(255,255,255,0.02)]">
                    <div class="shrink-0">
                    </div>
                    <div class="flex flex-col min-w-0">
                        <div class="flex items-baseline gap-2">
                            <span class="text-bright truncate hover:underline cursor-pointer">
                                {self.sender.clone()}
                            </span>
                            <span class="text-muted text-xs">
                                {self.date.format("%d/%m/%Y, %H:%M").to_string()}
                            </span>
                        </div>
                        <div class="text-normal leading-relaxed break-words">
                            {msg.content.clone()}
                        </div>
                    </div>
                </div>
            }.into_any(),
            TimelineItemKind::DateSeparator => view! {
                <div class="flex items-center gap-2 my-4">
                    <div class="flex-1 border-t border-[var(--tile-border-color)]"></div>
                    <span class="text-muted text-sm">
                        {self.date.format("%d %B %Y").to_string()}
                    </span>
                    <div class="flex-1 border-t border-[var(--tile-border-color)]"></div>
                </div>
            }.into_any(),
            TimelineItemKind::SystemMessage(sys_msg) => {
                let text = match sys_msg {
                    SystemMessage::EncryptionEnabled => "Encryption enabled",
                    SystemMessage::EncryptionDisabled => "Encryption disabled",
                    SystemMessage::UserJoined => "User joined the room",
                    SystemMessage::UserLeft => "User left the room",
                    SystemMessage::CallStarted => "Call started",
                    SystemMessage::CallEnded => "Call ended",
                };

                view! {
                    <div class="flex items-center justify-center my-2">
                        <span class="text-muted text-sm italic">
                            {text}
                        </span>
                    </div>
                }.into_any()
            }
        }
    }
}

#[component]
fn MessageItem(message: ChatMessage) -> impl IntoView {
    let member_store = expect_context::<MemberStore>();

    let profile_sig = member_store.get_profile(&message.room_id, &message.sender);
    let profile_sig_name = profile_sig.clone();

    view! {
        <div
            class="flex gap-3 p-3 rounded-md transition-colors hover:bg-[rgba(255,255,255,0.02)]"
            style="background-color: var(--tile-bg-color);"
        >
            <div class="shrink-0">
                {move || {
                    let p = profile_sig.get();
                    let initial = p.display_name.unwrap_or_default().chars().next().unwrap_or('?').to_string();

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
                        {move || profile_sig_name.get().display_name}
                    </span>

                    <span class="text-muted text-xs">
                        {format_matrix_ts(message.ts)}
                    </span>
                </div>

                <div
                    class="text-normal leading-relaxed break-words"
                >
                    {format!("{}|\n{:?}|\n{}", message.msg_type, message.content, message.raw_json)}
                </div>
            </div>
        </div>
    }
}

fn get_date_from_ts(ts: i64) -> DateTime<Local> {
    Local
        .timestamp_opt(ts, 0)
        .latest()
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.with_timezone(&Local))
}

#[component]
fn TimeLine(messages: ReadSignal<Vec<ChatMessage>>) -> impl IntoView {
    let flattened_items = Memo::new(move |_| {
        let msgs = messages.get();
        let mut items = Vec::new();
        let mut last_day: Option<NaiveDate> = None;

        for msg in msgs.iter().rev() {
            let current_date = get_date_from_ts(msg.ts);
            let current_day = current_date.date_naive();

            let maybe_item = match msg.msg_type.as_str() {
                "m.text" => Some(TimelineItem {
                    date: current_date,
                    sender: msg.sender.clone(),
                    kind: TimelineItemKind::Message(TimelineMessage {
                        content: msg.content.clone().unwrap_or_default(),
                    }),
                }),
                "m.image" => Some(TimelineItem {
                    date: current_date,
                    sender: msg.sender.clone(),
                    kind: TimelineItemKind::Message(TimelineMessage {
                        content: format!("Image message with content: {:?}", msg.content),
                    }),
                }),
                "m.call.member" => Some(TimelineItem {
                    date: current_date,
                    sender: msg.sender.clone(),
                    kind: TimelineItemKind::SystemMessage(SystemMessage::CallStarted),
                }),
                "m.room.encryption" => {
                    if let Some(content) = &msg.content {
                        if content.contains("enabled") {
                            Some(TimelineItem {
                                date: current_date,
                                sender: msg.sender.clone(),
                                kind: TimelineItemKind::SystemMessage(
                                    SystemMessage::EncryptionEnabled,
                                ),
                            })
                        } else if content.contains("disabled") {
                            Some(TimelineItem {
                                date: current_date,
                                sender: msg.sender.clone(),
                                kind: TimelineItemKind::SystemMessage(
                                    SystemMessage::EncryptionDisabled,
                                ),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(item) = maybe_item {
                if Some(current_day) != last_day {
                    items.push(TimelineItem {
                        date: current_date,
                        sender: String::new(),
                        kind: TimelineItemKind::DateSeparator,
                    });
                    last_day = Some(current_date.date_naive());
                }

                items.push(item);
            }
        }
        items.reverse();
        items
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse p-4 gap-2">
            <For
                each=move || flattened_items.get()
                key=|item| format!("{}-{}", item.date.timestamp(), item.sender)
                children=|item| item.render()
            />
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
                <FloatingTile class="flex-1 h-full overflow-x">
                    <TimeLine messages=messages></TimeLine>
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
