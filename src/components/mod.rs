use colorsys::ColorAlpha;
use leptos::prelude::*;
use shared::{messages::RichTextSpan, user_profile::UserProfile};
use user_profile::UserProfileExt;

use crate::state::{AppState, MemberStore};

pub(crate) mod input;
pub(crate) mod presence;
pub(crate) mod user_profile;

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

#[component]
pub fn TextCircle(
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] style: String,
    text: String,
    color_string: String,
) -> impl IntoView {
    let mut color = UserProfile::get_color(color_string);
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
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    view! {
        <span class=format!("relative p-[2px] group cursor-pointer {class}")>
            <span
                contenteditable=false
                class="absolute inset-0 rounded -z-10 opacity-35 group-hover:opacity-100 transition-opacity duration-200"
                style=format!("background-color: {bg_color};")
            />
            <span class="relative" style=format!("color: {color};")>
                {text}
            </span>
        </span>
    }
}

pub trait RichTextExt {
    fn render(self) -> impl IntoView;
}

impl RichTextExt for RichTextSpan {
    fn render(self) -> impl IntoView {
        let state: AppState = expect_context();
        let store: MemberStore = expect_context();

        let Some(room_id) = state.active_room_id.get_untracked() else {
            return view! {}.into_any();
        };

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
                    let mut color = profile.get_user_color();
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
                            />
                        }
                    }}
                }
                .into_any()
            }

            RichTextSpan::RoomMention => {
                let color = "white".to_string();
                let bg_color = "lightgray".to_string();

                view! { <WordMention text="@room".to_string() color=color bg_color=bg_color /> }
                    .into_any()
            }
            RichTextSpan::Newline => view! { <br /> }.into_any(),
        }
    }
}
