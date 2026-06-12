use csscolorparser::Color;
use leptos::prelude::*;
use phosphor_leptos::{Icon, IconWeight, CARET_DOWN, GEAR, HEADPHONES, MICROPHONE, X};
use serde_json::json;
use shared::ColorExt;
use web_sys::{KeyboardEvent, MouseEvent};

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
                "absolute text-muted hover:text-(--bright-text-color) border border-transparent hover:bg-(--ui-solid-hover-bg) hover:border-(--tile-border-color) cursor-pointer p-1 rounded-(--gap) {class}",
            )
            style:top=inset.clone()
            style:right=inset
            on:click=on_click
        >
            <Icon icon=X size=size />
        </button>
    }
}

use leptos::portal::Portal;

use crate::app::call_tauri;

#[component]
pub fn SettingsIcon(#[prop(into, optional)] class: String) -> impl IntoView {
    let (is_open, set_is_open) = signal(false);

    let (slider_value, set_slider_value) = signal(50);
    let slider_action = Action::new_local(|new_value: &f64| {
        let val_clone = *new_value;
        async move {
            call_tauri(
                "change_screen_scaling",
                serde_wasm_bindgen::to_value(&json!({"scale_factor": val_clone})).unwrap(),
            )
            .await
        }
    });
    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if is_open.try_get_untracked().unwrap_or(false) && ev.key() == "Escape" {
            set_is_open.set(false);
        }
    });

    view! {
        <button
            on:click=move |_| set_is_open.update(|v| *v = !*v)
            class=format!(
                "text-muted hover:text-(--bright-text-color) cursor-pointer transition-transform duration-300 ease-in-out hover:rotate-[90deg] {class}",
            )
        >
            <Icon icon=GEAR size="20px" weight=IconWeight::Bold />
        </button>

        <Show when=move || is_open.get() fallback=|| view! { "" }>
            <Portal>
                <div
                    on:click=move |_| set_is_open.set(false)
                    class="fixed inset-0 z-40 bg-transparent flex items-center justify-center p-6 md:p-12"
                >
                    <div
                        on:click=move |e| e.stop_propagation()
                        class="bg-neutral-900 opacity-100 text-(--bright-text-color, black) rounded-xl shadow-2xl border border-neutral-700/50 w-full max-w-5xl h-full max-h-[85vh] min-h-[50vh] flex flex-col overflow-hidden z-50 p-6"
                    >
                        <div class="flex-1 overflow-y-auto">
                            <div class="flex items-center justify-between p-4 bg-neutral-800/40 rounded-xl border border-neutral-800/80 w-full">
                                <div class="flex flex-col space-y-1 pr-4">
                                    <label
                                        for="scaling-slider"
                                        class="text-sm font-semibold text-(--bright-text-color, white)"
                                    >
                                        "UI scale"
                                    </label>
                                    <span class="text-xs text-muted font-mono">
                                        {move || {
                                            format!(
                                                "{:.2}x",
                                                0.5 + (slider_value.get() as f64 / 100.0) * 1.5,
                                            )
                                        }}
                                    </span>
                                </div>

                                <div class="w-full max-w-xs md:max-w-md">
                                    <input
                                        id="scaling-slider"
                                        type="range"
                                        min="0"
                                        max="100"
                                        prop:value=move || slider_value.get()

                                        on:input=move |ev| {
                                            let val = event_target_value(&ev)
                                                .parse::<i32>()
                                                .unwrap_or(0);
                                            set_slider_value.set(val);
                                            let mapped_val = 0.5 + (val as f64 / 100.0) * 1.5;
                                            slider_action.dispatch_local(mapped_val);
                                        }
                                        class="w-full h-2 bg-neutral-700 rounded-lg appearance-none cursor-pointer accent-indigo-500"
                                    />
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </Portal>
        </Show>
    }
}

#[component]
pub fn MuteMenu(#[prop(into, optional)] class: String) -> impl IntoView {
    // None = neither hovered, Some(true) = mic hovered, Some(false) = caret hovered
    let hovered: RwSignal<Option<bool>> = RwSignal::new(None);

    view! {
        <div
            class=format!("flex flex-row gap-[1px] h-full {class} py-[9px] cursor-pointer")
            on:mouseleave=move |_| hovered.set(None)
        >
            <button
                class="text-muted hover:text-(--bright-text-color) cursor-pointer hover:text-bright rounded-l-(--ui-border-radius) h-full aspect-square flex items-center justify-center"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(true))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(false))
                on:mouseenter=move |_| hovered.set(Some(true))
            >
                <Icon icon=MICROPHONE size="18px" weight=IconWeight::Fill />
            </button>
            <button
                class="text-muted hover:text-(--bright-text-color) cursor-pointer hover:text-bright rounded-r-(--ui-border-radius) h-full"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(false))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(true))
                on:mouseenter=move |_| hovered.set(Some(false))
            >
                <Icon icon=CARET_DOWN size="12px" />
            </button>
        </div>
    }
}

#[component]
pub fn DeafenMenu(#[prop(into, optional)] class: String) -> impl IntoView {
    // None = neither hovered, Some(true) = mic hovered, Some(false) = caret hovered
    let hovered: RwSignal<Option<bool>> = RwSignal::new(None);

    view! {
        <div
            class=format!("flex flex-row gap-[1px] h-full {class} py-[9px] cursor-pointer")
            on:mouseleave=move |_| hovered.set(None)
        >
            <button
                class="text-muted hover:text-(--bright-text-color) cursor-pointer hover:text-bright rounded-l-(--ui-border-radius) h-full aspect-square flex items-center justify-center"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(true))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(false))
                on:mouseenter=move |_| hovered.set(Some(true))
            >
                <Icon icon=HEADPHONES size="18px" weight=IconWeight::Fill />
            </button>
            <button
                class="text-muted hover:text-(--bright-text-color) cursor-pointer hover:text-bright rounded-r-(--ui-border-radius) h-full"
                class=("bg-(--color-item-selected)", move || hovered.get() == Some(false))
                class=("bg-(--color-item-hover)", move || hovered.get() == Some(true))
                on:mouseenter=move |_| hovered.set(Some(false))
            >
                <Icon icon=CARET_DOWN size="12px" />
            </button>
        </div>
    }
}
