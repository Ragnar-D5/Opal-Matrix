use colorsys::Hsl;
use leptos::prelude::*;
use phosphor_leptos::{Icon, X};
use web_sys::MouseEvent;

pub(crate) mod authentication;
pub(crate) mod chat;
pub(crate) mod emoji_picker;
pub(crate) mod input;
pub(crate) mod loading;
pub(crate) mod presence;
pub(crate) mod previews;
pub(crate) mod shader;
pub(crate) mod sidebar;
pub(crate) mod text;
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
pub fn SingleFloatingTile(
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] style: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="w-screen h-screen flex justify-center items-center" style=style>
            <FloatingTile class=class>{children()}</FloatingTile>
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
                {text}
            </span>
        </div>
    }
}

#[component]
pub fn TypingIndicator(#[prop(into)] size: String) -> impl IntoView {
    let style_string =
        move |delay| format!("width: {size}; height: {size}; animation-delay: {delay}s;");

    view! {
        <div class="flex items-center space-x-1">
            <div class="typing-indicator rounded-full" style=style_string(0.0)></div>
            <div class="typing-indicator rounded-full" style=style_string(0.2)></div>
            <div class="typing-indicator rounded-full" style=style_string(0.4)></div>
        </div>
    }
}

#[component]
pub fn CloseButton<T>(
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] size: Option<String>,
    #[prop(into, optional)] inset: Option<String>,
    on_click: T,
) -> impl IntoView
where
    T: Fn(MouseEvent) + 'static,
{
    let inset = inset.unwrap_or("12px".to_string());
    let size = size.unwrap_or("16px".to_string());

    view! {
        <button
            class=format!("absolute text-muted hover:text-(--bright-text-color) border border-transparent hover:bg-(--ui-solid-hover-bg) hover:border-(--tile-border-color) cursor-pointer p-1 rounded-(--gap) {class}")
            style:top=inset.clone()
            style:right=inset
            on:click=on_click
        >
            <Icon icon=X size=size />
        </button>
    }
}
