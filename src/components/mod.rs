use colorsys::ColorAlpha;
use leptos::{html::Span, prelude::*, task::spawn_local};
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
pub fn Caret(caret_ref: NodeRef<Span>) -> impl IntoView {
    view! {
        <span
            node_ref=caret_ref
            class="inline-block relative w-0 h-[1.2em] pointer-events-none"
            style="vertical-align: text-bottom;"
        >
            <div class="absolute left-0 w-[1.5px] h-full bg-current transition-opacity" />
        </span>
    }
}

#[component]
fn WordMention(
    text: String,
    color: String,
    bg_color: String,
    #[prop(into, optional)] class: String,
    data_t_idx: usize,
) -> impl IntoView {
    view! {
        <span data-t-idx=data_t_idx class=format!("relative p-[2px] group cursor-pointer {class}")>
            <span
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
    fn render_caret(
        self,
        caret_pos: Option<usize>,
        caret_ref: NodeRef<Span>,
        idx: usize,
    ) -> impl IntoView;
    fn len(&self) -> usize;
}

impl RichTextExt for RichTextSpan {
    fn len(&self) -> usize {
        match self {
            RichTextSpan::Plain(text) => text.len(),
            RichTextSpan::Link { url, .. } => url.len(),
            RichTextSpan::Newline
            | RichTextSpan::RoomMention
            | RichTextSpan::UserMention { .. } => 1,
        }
    }

    fn render(self) -> impl IntoView {
        self.render_caret(None, NodeRef::default(), 0)
    }

    fn render_caret(
        self,
        caret_pos: Option<usize>,
        caret_ref: NodeRef<Span>,
        idx: usize,
    ) -> impl IntoView {
        let state: AppState = expect_context();
        let store: MemberStore = expect_context();

        let Some(room_id) = state.active_room_id.get_untracked() else {
            return view! {}.into_any();
        };

        let caret = view! { <Caret caret_ref=caret_ref /> }.into_any();

        match self {
            RichTextSpan::Plain(text) => {
                if let Some(caret_pos) = caret_pos {
                    let split = text.split_at(caret_pos);

                    view! {
                        <span data-t-idx=idx class="text-token">
                            {split.0}
                        </span>
                        {caret}
                        <span data-t-idx=idx + 1 class="text-token">
                            {split.1}
                        </span>
                    }
                    .into_any()
                } else {
                    view! {
                        <span data-t-idx=idx class="text-token">
                            {text}
                        </span>
                    }
                    .into_any()
                }
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

                let (caret_before, caret_after) = match caret_pos {
                    Some(0) => (Some(caret), None),
                    Some(_) => (None, Some(caret)),
                    None => (None, None),
                };

                view! {
                    {caret_before}
                    {move || {
                        let (color, bg_color) = colors.get();

                        view! {
                            <WordMention
                                text=format!("@{}", display_name)
                                color=color
                                bg_color=bg_color
                                data_t_idx=idx
                            />
                        }
                    }}
                    {caret_after}
                }
                .into_any()
            }

            RichTextSpan::RoomMention => {
                let (caret_before, caret_after) = match caret_pos {
                    Some(0) => (Some(caret), None),
                    Some(_) => (None, Some(caret)),
                    None => (None, None),
                };

                let color = "white".to_string();
                let bg_color = "lightgray".to_string();

                view! {
                    {caret_before}
                    <WordMention
                        text="@room".to_string()
                        color=color
                        bg_color=bg_color
                        data_t_idx=idx
                    />
                    {caret_after}
                }
                .into_any()
            }

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
                if let Some(caret_pos) = caret_pos {
                    if caret_pos == 0 {
                        view! {
                            {caret}
                            <br data-t-idx=idx />
                        }
                        .into_any()
                    } else {
                        view! {
                            <br data-t-idx=idx />
                            {caret}
                        }
                        .into_any()
                    }
                } else {
                    view! { <br data-t-idx=idx /> }.into_any()
                }
            }
        }
    }
}
