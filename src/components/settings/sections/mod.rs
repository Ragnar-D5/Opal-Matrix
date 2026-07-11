use std::collections::HashMap;

use crate::app::MatrixSettingField;
use icondata as i;
use leptos::prelude::*;
use leptos_icons::Icon as LIcon;

use phosphor_leptos::{Icon, CARET_DOWN, QUESTION};

pub mod appearance;
pub mod chats;
pub mod profiles;
pub mod updates;

pub fn render_toggle(field: MatrixSettingField<bool>) -> AnyView {
    let signal = field.signal();
    let name = field.human_readable;
    let description = field.description;

    let (cloud_icon, cloud_color) = if field.uses_cloud {
        (i::BsCloud, "var(--accent-color)")
    } else {
        (i::BsCloudSlash, "var(--dim-text-color)")
    };

    view! {
        <label class="flex justify-between gap-2 cursor-pointer border-transparent hover:border-(--tile-border-color) border transition-colors duration-100 rounded-lg p-3 items-center hover:bg-(--tile-hover-color)">
            <span class="inline-flex items-center gap-2">
                <div class="relative inline-block w-11 h-5 shrink-0">
                    <input
                        type="checkbox"
                        prop:checked=move || signal.get()
                        on:change=move |ev| field.set(event_target_checked(&ev))
                        class="peer appearance-none w-11 h-5 rounded-full checked:bg-(--muted-text-color) cursor-pointer transition-colors duration-300 focus:border-(--accent-color) border-(--tile-border-color) border"
                    />
                    <span class="absolute top-0 left-0 w-5 h-5 bg-(--error-color) peer-checked:bg-(--success-color) rounded-full transition-transform duration-300 peer-checked:translate-x-6 pointer-events-none border border-(--tile-border-color)"></span>
                </div>
                <span class="text-normal select-none">{name}</span>
                <div title=description class="flex items-center">
                    <Icon icon=QUESTION size="14px" color="var(--dim-text-color)" />
                </div>
            </span>
            <div title=if field.uses_cloud {
                "This setting is synced with the cloud."
            } else {
                "This setting is not synced with the cloud."
            }>
                <LIcon icon=cloud_icon style=format!("color: {cloud_color};") height="18px" />
            </div>
        </label>
    }
    .into_any()
}

#[component]
pub fn SubSection<'a>(title: &'a str, children: Children) -> AnyView {
    let expanded = RwSignal::new(true);

    view! {
        <div
            class="w-full flex items-center justify-between cursor-pointer select-none group"
            on:click=move |_| expanded.update(|v| *v = !*v)
        >
            <h2 class="text-lg font-semibold text-normal">{title}</h2>
            <div class="flex-1 h-px bg-(--tile-border-color) mx-2"></div>
            <button
                class="flex items-center justify-center transition-transform duration-100 cursor-pointer text-dim group-hover:text-normal"
                class=("rotate-180", move || !expanded.get())
            >
                <Icon icon=CARET_DOWN size="16px" />
            </button>
        </div>
        <div
            class="overflow-hidden transition-all duration-100 mb-4"
            style=move || {
                if expanded.get() {
                    "max-height: 1000px; opacity: 1;"
                } else {
                    "max-height: 0; opacity: 0;"
                }
            }
        >
            {children()}
        </div>
    }
    .into_any()
}

#[component]
pub fn Toggle(field: MatrixSettingField<bool>) -> AnyView {
    render_toggle(field)
}
