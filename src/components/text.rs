use crate::openUrl;
use colorsys::ColorAlpha;
use leptos::{prelude::*, task::spawn_local};
use shared::messages::RichTextSpan;

use crate::{components::user_profile::UserProfileExt, state::MemberStore};

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
    fn render(self, store: MemberStore, room_id: String) -> impl IntoView;
}

impl RichTextExt for RichTextSpan {
    fn render(self, store: MemberStore, room_id: String) -> impl IntoView {
        match self {
            RichTextSpan::Plain(text) => {
                view! { <span class="text-token">{text}</span> }.into_any()
            }

            RichTextSpan::Link { url, .. } => {
                let style = "color: var(--accent-color); cursor: pointer;";
                let url_click = url.clone();

                view! {
                    <span
                        style=style
                        class="text-token"
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
                let profile_sig = store.get_profile(&room_id, &user_id);

                let colors = Memo::new(move |_| {
                    let profile = profile_sig.get().unwrap_or_default();
                    let mut color = profile.get_color();
                    let primary = color.to_css_string();

                    color.set_alpha(0.4);
                    let background = color.to_css_string();

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
                                data_id=user_id.clone()
                            />
                        }
                    }}
                }
                .into_any()
            }

            RichTextSpan::RoomMention => {
                let color = "white".to_string();
                let bg_color = "lightgray".to_string();

                view! {
                    <WordMention
                        text="@room".to_string()
                        color=color
                        bg_color=bg_color
                        data_type="room_mention".to_string()
                        data_id=room_id
                    />
                }
                .into_any()
            }
            RichTextSpan::Newline => view! { <br /> }.into_any(),
        }
    }
}
