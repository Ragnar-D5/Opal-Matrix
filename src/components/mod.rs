use colorsys::ColorAlpha;
use colorsys::Hsl;
use leptos::prelude::*;
use shared::messages::RichTextSpan;
use user_profile::UserProfileExt;

use crate::state::MemberStore;

pub(crate) mod input;
pub(crate) mod presence;
pub(crate) mod user_profile;

pub fn get_color(string: String) -> Hsl {
    let mut hash: u32 = 0;
    for c in string.chars() {
        hash = (c as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
    }

    let hue = hash % 360;

    Hsl::new(hue as f64, 90.0, 70.0, None)
}

#[component]
pub fn FloatingTile(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    view! {
        <div
            class=format!(
                "servers flex flex-col items-center bg-[var(--tile-bg-color)] border-[1px] border-[var(--tile-border-color)] rounded-(--floating-border-radius) overflow-y-auto shadow-sm flex-shrink-0 backdrop-blur-2xl {class}",
            )
            style="scrollbar-width: none;"
        >
            {children()}
        </div>
    }
}

/// A circular avatar with a single letter, colored based on the provided color.
#[component]
pub fn TextCircle(
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] style: String,
    text: String,
    mut color: Hsl,
) -> impl IntoView {
    let letter_color = color.clone().to_css_string();

    color.set_lightness(10.0);
    let bg_color = color.to_css_string();

    view! {
        <div
            class=format!(
                "relative flex items-center justify-center aspect-square {class} font-bold overflow-hidden",
            )
            style=format!(
                "background-color: {bg_color}; container-type: size; color: {letter_color}; {style}",
            )
        >
            <div
                class="absolute inset-0 pointer-events-none"
                style=format!(
                    "box-shadow: inset 0 0 10cqmin 5cqmin {letter_color}; border-radius: inherit;",
                )
            ></div>

            <span class="relative" style="font-size: 50cqmin; line-height: 1;">
                {text.chars().next().unwrap_or('?').to_ascii_uppercase()}
            </span>
        </div>
    }
}

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
                let style =
                    "color: var(--link-color); text-decoration: underline; cursor: pointer;";

                view! {
                    <span style=style class="text-token">
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
