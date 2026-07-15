use crate::hooks::openUrl;
use leptos::{prelude::*, task::spawn_local};
use ruma::RoomId;
use shared::{ColorExt, timeline::RichTextSpan};

use crate::state::ProfileStore;

#[component]
fn WordMention(
    text: String,
    color: String,
    bg_color: String,
    data_type: String,
    data_id: String,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    view! {
        <span
            contenteditable="false"
            data-type=data_type
            data-id=data_id
            class=format!(
                "inline-flex items-center leading-none relative p-[2px] group cursor-pointer select-none {class}",
            )
        >
            <span
                class="absolute inset-0 rounded opacity-35 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none"
                style=format!("background-color: {bg_color};")
            />
            <span class="relative pointer-events-none" style=format!("color: {color};")>
                {text}
            </span>
        </span>
    }
}

pub trait RichTextExt {
    fn render(self, store: ProfileStore, room_id: &RoomId) -> AnyView;
}

impl RichTextExt for RichTextSpan {
    fn render(self, store: ProfileStore, room_id: &RoomId) -> AnyView {
        match self {
            RichTextSpan::Plain(text) => {
                view! { <span class="text-token cursor-text">{text}</span> }.into_any()
            }

            RichTextSpan::Link { url, .. } => {
                let url_click = url.clone();

                view! {
                    <span
                        class="text-token text-(--link-color) cursor-pointer hover:underline"
                        on:click=move |_| {
                            let url_async = url_click.clone();
                            spawn_local(async move {
                                let _ = openUrl(&url_async)
                                    .await
                                    .map_err(|e| log::warn!("Failed to open link: {:?}", e));
                            });
                        }
                    >
                        {url}
                    </span>
                }
                .into_any()
            }

            RichTextSpan::UserMention {
                user_id,
                display_name,
            } => {
                let profile_sig = store.get_member_profile(room_id, &user_id);

                let colors = Memo::new(move |_| {
                    let profile = profile_sig.get();
                    let mut color = profile.name_color();
                    let primary = color.to_css_hsl();

                    color.set_alpha(0.4);
                    let background = color.to_css_hsl();

                    (primary, background)
                });

                view! {
                    {move || {
                        let (color, bg_color) = colors.get();

                        view! {
                            <WordMention
                                text=format!("@{}", display_name)
                                color=color
                                bg_color=bg_color
                                data_type="user_mention".to_string()
                                data_id=user_id.to_string()
                            />
                        }
                    }}
                }
                .into_any()
            }

            RichTextSpan::RoomMention {
                room_id,
                display_name,
            } => {
                let color = "white".to_string();
                let bg_color = "lightgray".to_string();

                view! {
                    <WordMention
                        text=display_name
                        color=color
                        bg_color=bg_color
                        data_type="room_mention".to_string()
                        data_id=room_id.source()
                    />
                }
                .into_any()
            }
            RichTextSpan::Newline => view! { <br /> }.into_any(),

            RichTextSpan::Highlight(text) => view! {
                <span
                    class="text-token cursor-text text-bright"
                    style="background-color: color-mix(in srgb, var(--result-highlight-color) 40%, transparent);"
                >
                    {text}
                </span>
            }
            .into_any(),
        }
    }
}

pub fn richt_text_spans_to_html(
    spans: &[RichTextSpan],
    store: ProfileStore,
    room_id: &RoomId,
) -> String {
    let doc = document();
    let owner = Owner::new();

    let combined_html = owner.with(|| {
        spans
            .iter()
            .map(|span| {
                match span.clone() {
                    RichTextSpan::Plain(text) | RichTextSpan::Highlight(text) => {
                        text
                    }
                    RichTextSpan::RoomMention { .. } | RichTextSpan::UserMention { .. } => {
                        let view = span.clone().render(store.clone(), room_id);
                        let any_view: AnyView = view.into_any();

                        // Create a fresh temporary container for this span
                        let temp_container = doc.create_element("div").unwrap();

                        // Build and mount
                        let mut render_state = any_view.build();
                        render_state.mount(&temp_container, None);

                        // Extract the raw HTML string
                        temp_container.inner_html()
                    }
                    RichTextSpan::Link { url, text } => format!(
                        r#"<span class="text-blue-500 underline link cursor-pointer" data-url="{url}">{}</span>"#,
                        text.unwrap_or(url.clone())
                    ),
                    RichTextSpan::Newline => "<br>".to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("")
    });

    owner.cleanup();

    combined_html
}
