use csscolorparser::Color;
use leptos::prelude::*;
use phosphor_leptos::{Icon, IconWeight, CARET_DOWN, CARET_UP, HEADPHONES, MICROPHONE, X};
use shared::ColorExt;
use web_sys::MouseEvent;

pub use overlays::settings::SettingsIcon;

use crate::{
    components::overlays::audi_menu::audio_device_popup,
    tauri_functions::{close_window, get_audio_devices, minimize_window, toggle_fullscreen},
};

pub(crate) mod authentication;
pub(crate) mod blurhash;
pub(crate) mod chat;
pub(crate) mod input;
pub(crate) mod loading;
pub(crate) mod overlays;
pub(crate) mod presence;
pub(crate) mod previews;
pub(crate) mod shader;
pub(crate) mod sidebar;
pub(crate) mod text;
pub(crate) mod user_profile;

#[component]
pub fn FloatingTile(
    #[prop(into, optional)] class: String,
    children: Children,
    #[prop(into, optional)] style: String,
) -> impl IntoView {
    view! {
        <div
            class=format!(
                "servers flex flex-col items-center bg-(--tile-bg-color) border border-(--tile-border-color) rounded-(--floating-border-radius) overflow-y-auto shadow-sm flex-shrink-0 backdrop-blur-2xl {class}",
            )
            style=format!("scrollbar-width: none; {}", style)
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
    mut color: Color,
) -> impl IntoView {
    let letter_color = color.clone().to_css_hsl();

    color.set_lightness(0.1);
    let bg_color = color.to_css_hsl();

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
            class=format!(
                "absolute text-muted hover:text-bright border border-transparent hover:bg-(--ui-solid-hover-bg) hover:border-(--tile-border-color) cursor-pointer p-1 rounded-(--gap) {class}",
            )
            style:top=inset.clone()
            style:right=inset
            on:click=on_click
        >
            <Icon icon=X size=size />
        </button>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AudioMenu {
    Mute,
    Deafen,
}

#[component]
pub fn MuteMenu(
    #[prop(into, optional)] class: String,
    open: RwSignal<Option<AudioMenu>>,
) -> impl IntoView {
    // None = neither hovered, Some(true) = mic hovered, Some(false) = caret hovered
    let hovered: RwSignal<Option<bool>> = RwSignal::new(None);
    let is_open = move || open.get() == Some(AudioMenu::Mute);

    view! {
        <div
            class=format!("relative flex flex-row gap-[1px] h-full {class} py-[9px] cursor-pointer")
            on:mouseleave=move |_| hovered.set(None)
        >
            {move || {
                if is_open() {
                    audio_device_popup(Callback::new(move |_| open.set(None)), true)
                } else {
                    ().into_any()
                }
            }}

            <button
                class="text-muted hover:text-bright cursor-pointer rounded-l-(--ui-border-radius) h-full aspect-square flex items-center justify-center"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(true))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(false))
                on:mouseenter=move |_| hovered.set(Some(true))
            >
                <Icon icon=MICROPHONE size="18px" weight=IconWeight::Fill />
            </button>
            <button
                class="text-muted hover:text-bright cursor-pointer rounded-r-(--ui-border-radius) h-full"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(false))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(true))
                on:mouseenter=move |_| hovered.set(Some(false))
                on:click=move |e| {
                    e.stop_propagation();
                    let now_open = !is_open();
                    open.set(if now_open { Some(AudioMenu::Mute) } else { None });
                    if now_open {
                        get_audio_devices();
                    }
                }
            >
                {move || {
                    view! {
                        <Icon icon=if is_open() { CARET_UP } else { CARET_DOWN } size="12px" />
                    }
                }}
            </button>
        </div>
    }
}

#[component]
pub fn DeafenMenu(
    #[prop(into, optional)] class: String,
    open: RwSignal<Option<AudioMenu>>,
) -> impl IntoView {
    // None = neither hovered, Some(true) = mic hovered, Some(false) = caret hovered
    let hovered: RwSignal<Option<bool>> = RwSignal::new(None);
    let is_open = move || open.get() == Some(AudioMenu::Deafen);

    view! {
        <div
            class=format!("relative flex flex-row gap-[1px] h-full {class} py-[9px] cursor-pointer")
            on:mouseleave=move |_| hovered.set(None)
        >
            {move || {
                if is_open() {
                    audio_device_popup(Callback::new(move |_| open.set(None)), false)
                } else {
                    ().into_any()
                }
            }}

            <button
                class="text-muted hover:text-bright cursor-pointer hover:text-bright rounded-l-(--ui-border-radius) h-full aspect-square flex items-center justify-center"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(true))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(false))
                on:mouseenter=move |_| hovered.set(Some(true))
            >
                <Icon icon=HEADPHONES size="18px" weight=IconWeight::Fill />
            </button>
            <button
                class="text-muted hover:text-bright cursor-pointer hover:text-bright rounded-r-(--ui-border-radius) h-full"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(false))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(true))
                on:mouseenter=move |_| hovered.set(Some(false))
                on:click=move |e| {
                    e.stop_propagation();
                    let now_open = !is_open();
                    open.set(if now_open { Some(AudioMenu::Deafen) } else { None });
                    if now_open {
                        get_audio_devices();
                    }
                }
            >
                {move || {
                    view! {
                        <Icon icon=if is_open() { CARET_UP } else { CARET_DOWN } size="12px" />
                    }
                }}
            </button>
        </div>
    }
}

#[component]
pub fn SystemButtons() -> impl IntoView {
    let btns: Vec<(&str, Callback<()>)> = vec![
        (
            "var(--idle-color)",
            Callback::new(move |_| minimize_window()),
        ),
        (
            "var(--online-color)",
            Callback::new(move |_| toggle_fullscreen()),
        ),
        ("var(--busy-color)", Callback::new(move |_| close_window())),
    ];

    view! {
        <div class="flex flex-row gap-3 z-9999">
            {btns
                .into_iter()
                .map(|(color, callback)| {
                    let btn_pressed = RwSignal::new(false);

                    view! {
                        <button
                            class="h-3.5 w-3.5 rounded-full hover:brightness-[60%] transition-transform duration-75 z-9999 cursor-pointer"
                            style=format!("background-color: {color};")
                            on:click=move |_| callback.run(())
                            class=("scale-75", move || btn_pressed.get())
                            on:mousedown=move |_| btn_pressed.set(true)
                            on:mouseup=move |_| btn_pressed.set(false)
                            on:mouseleave=move |_| btn_pressed.set(false)
                        />
                    }
                })
                .collect_view()}
        </div>
    }
}
