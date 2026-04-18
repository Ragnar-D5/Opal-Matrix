use std::collections::HashMap;

use crate::components::FloatingTile;
use crate::hooks::use_tauri_event;
use leptos::html::Div;
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct ChatMessage {
    msg_type: String,
    id: String,
    ts: i64,
    content: Option<String>,
    raw_json: String,
    sender: String,
}

#[component]
fn MessageItem(message: ChatMessage) -> impl IntoView {
    let color = match message.msg_type.as_str() {
        "m.text" => "text-green-500",
        "m.image" => "text-blue-500",
        "m.file" => "text-yellow-500",
        _ => "text-gray-500",
    };

    view! {
        <div class="p-2 rounded-md break-all" style=format!("background-color: var(--tile-bg-color); color: var(--text-color);")>
            {message.raw_json}
        </div>
    }
}

#[component]
fn TimeLine(messages: ReadSignal<Vec<ChatMessage>>) -> impl IntoView {
    let scroll_ref = NodeRef::<Div>::new();

    Effect::new(move |_| {
        if let Some(el) = scroll_ref.get() {
            request_animation_frame(move || {
                el.set_scroll_top(el.scroll_height());
            })
        }
    });

    view! {
        <div class="flex-1 w-full w-full overflow-y-auto flex flex-col-reverse p-4 gap-2">
            <For
                each=move || messages.get()
                key=|msg| msg.id.clone()
                children=|msg| {
                    view! { <MessageItem message=msg /> }
                }
            />
        </div>
    }
}

#[component]
pub fn Chat(
    messages: ReadSignal<Vec<ChatMessage>>,
    set_messages: WriteSignal<Vec<ChatMessage>>,
) -> impl IntoView {
    let message_update_event: ReadSignal<Option<HashMap<String, Vec<ChatMessage>>>> =
        use_tauri_event("message_update");

    view! {
        <div class="flex-1 h-full flex gap-3 flex-col">
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
                <FloatingTile class="w-90 p-2 h-full">
                    "Chat info goes here"
                </FloatingTile>
            </div>
        </div>
    }
}
