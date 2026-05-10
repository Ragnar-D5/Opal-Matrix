use leptos::{prelude::*, task::spawn_local};
use log::info;
use shared::{messages::RichTextSpan, user_profile::UserProfile};
use user_profile::UserProfileExt;
use web_sys::MouseEvent;

use crate::{
    app::openUrl,
    state::{AppState, MemberStore},
};

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
pub fn Caret() -> impl IntoView {
    view! {
        <span class="inline-block relative w-0 h-[1.2em] translate-y-[0.1em] pointer-events-none">
            "\u{200B}"
            <div class="absolute left-0 w-[1.5px] h-full bg-current transition-opacity" />
        </span>
    }
}

pub trait RenderRichText {
    fn render(self) -> impl IntoView;
    fn renhder_caret(self, caret_pos: Option<usize>) -> impl IntoView;
}

impl RenderRichText for RichTextSpan {
    fn render(self) -> impl IntoView {
        self.renhder_caret(None)
    }

    fn renhder_caret(self, caret_pos: Option<usize>) -> impl IntoView {
        let state: AppState = expect_context();
        let store: MemberStore = expect_context();

        let Some(room_id) = state.active_room_id.get_untracked() else {
            return view! {}.into_any();
        };

        match self {
            RichTextSpan::Plain(text) => {
                if let Some(caret_pos) = caret_pos {
                    let split = text.split_at(caret_pos);

                    view! {
                        <span class="text-token">{split.0}</span>
                        <Caret />
                        <span class="text-token">{split.1}</span>
                    }
                    .into_any()
                } else {
                    view! { <span class="text-token">{text}</span> }.into_any()
                }
            }

            RichTextSpan::UserMention {
                user_id,
                display_name,
            } => {
                let profile_sig = store.get_profile(&room_id, &user_id);

                let color = Memo::new(move |_| {
                    let profile = profile_sig.get().unwrap_or_default();
                    profile.get_user_color().to_css_string()
                });

                let mut caret_before = view! {}.into_any();
                let mut caret_after = view! {}.into_any();
                if let Some(caret_pos) = caret_pos {
                    if caret_pos == 0 {
                        caret_before = view! { <Caret /> }.into_any();
                    } else {
                        caret_after = view! { <Caret /> }.into_any();
                    }
                }

                view! {
                    {caret_before}
                    <span class="relative p-[2px] group cursor-pointer">
                        <span
                            class="absolute inset-0 rounded -z-10 opacity-10 group-hover:opacity-40 transition-opacity duration-200"
                            style:background-color=move || color.get()
                        />

                        <span class="relative" style:color=move || color.get() title=user_id>
                            "@"
                            {display_name}
                        </span>
                    </span>
                    {caret_after}
                }
            .into_any()
            }

            RichTextSpan::RoomMention => view! {
                <span class="bg-[#FEE75C]/30 text-[#FEE75C] px-1 mx-0.5 rounded font-medium">
                    "@room"
                </span>
            }
            .into_any(),

            RichTextSpan::Link { url, .. } => {
                let clone = url.clone();

                let on_click = move |ev: MouseEvent| {
                    ev.prevent_default(); // Stop the webview from navigating
                    let u = clone.clone();
                    spawn_local(async move {
                        let _ = openUrl(&u);
                    });
                };

                view! {
                    <a
                        href=url.clone()
                        target="_blank"
                        class="text-[#00A8FC] hover:underline"
                        on:click=on_click
                    >
                        {url.clone()}
                    </a>
                }
                .into_any()
            }
            RichTextSpan::Newline => {
                if caret_pos.is_some() {
                    view! {
                        <Caret />
                        <br />
                    }
                    .into_any()
                } else {
                    view! { <br /> }.into_any()
                }
            }
        }
    }
}
