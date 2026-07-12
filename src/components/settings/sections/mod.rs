use std::collections::HashMap;

use icondata as i;
use leptos::{html::Button, portal::Portal, prelude::*};
use leptos_icons::Icon as LIcon;
use serde::{de::DeserializeOwned, Serialize};
use shared::settings::EnumVariants;
use web_sys::{KeyboardEvent, ScrollBehavior, ScrollIntoViewOptions, ScrollLogicalPosition};

use phosphor_leptos::{Icon, CARET_DOWN, CHECK, QUESTION};

use crate::components::settings::MatrixSettingField;

pub mod appearance;
pub mod chats;
pub mod general;
pub mod profiles;
pub mod updates;

fn get_cloud_stuff(uses_cloud: bool) -> (i::Icon, &'static str, &'static str) {
    if uses_cloud {
        (
            i::BsCloud,
            "color: var(--accent-color);",
            "This setting is synced with the cloud.",
        )
    } else {
        (
            i::BsCloudSlash,
            "color: var(--dim-text-color);",
            "This setting is not synced with the cloud.",
        )
    }
}

pub fn render_toggle(
    on_toggle: impl Fn(bool) + 'static,
    signal: RwSignal<bool>,
    name: &str,
    description: &str,
    uses_cloud: Option<bool>,
) -> AnyView {
    let (cloud_icon, cloud_color, cloud_tooltip) = get_cloud_stuff(uses_cloud.unwrap_or(false));

    view! {
        <label class="flex justify-between gap-2 cursor-pointer border-transparent hover:border-(--tile-border-color) border transition-colors duration-100 rounded-lg p-(--gap) items-center hover:bg-(--tile-hover-color) text-dim hover:text-normal">
            <span class="inline-flex items-center gap-2">
                <span class="select-none">{name}</span>
                <div title=description class="flex items-center">
                    <Icon icon=QUESTION size="14px" color="var(--dim-text-color)" />
                </div>
            </span>
            <span class="inline-flex items-center gap-3">
                <div class="relative inline-block w-11 h-5 shrink-0">
                    <input
                        type="checkbox"
                        prop:checked=move || signal.get()
                        on:change=move |ev| on_toggle(event_target_checked(&ev))
                        class="peer appearance-none w-11 h-5 rounded-full checked:bg-(--muted-text-color) cursor-pointer transition-colors duration-300 focus:border-(--accent-color) border-(--tile-border-color) border"
                    />
                    <span class="absolute top-0 left-0 w-5 h-5 bg-(--error-color) peer-checked:bg-(--success-color) rounded-full transition-transform duration-300 peer-checked:translate-x-6 pointer-events-none border border-(--tile-border-color)"></span>
                </div>
                <Show when=move || uses_cloud.is_some()>
                    <div title=cloud_tooltip>
                        <LIcon icon=cloud_icon style=cloud_color height="18px" />
                    </div>
                </Show>
            </span>
        </label>
    }
    .into_any()
}

fn render_dropdown_from_options<T>(
    field: MatrixSettingField<T>,
    options: Vec<(T, String)>,
) -> AnyView
where
    T: Clone + PartialEq + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    let signal = field.signal();
    let options = StoredValue::new(options);
    let name = field.human_readable;
    let description = field.description;

    let is_open = RwSignal::new(false);
    let button_ref: NodeRef<Button> = NodeRef::new();
    let anchor_rect: RwSignal<Option<(f64, f64, f64, f64)>> = RwSignal::new(None);

    // The keyboard cursor while the panel is open. Arrow keys and type-ahead
    // move it; it's only committed to `field` when the user presses Enter
    // (or clicks a row, which commits directly).
    let highlighted_index: RwSignal<Option<usize>> = RwSignal::new(None);

    let current_label = move || {
        options.with_value(|opts| {
            opts.iter()
                .find(|(variant, _)| *variant == signal.get())
                .map(|(_, label)| label.clone())
                .unwrap_or_default()
        })
    };

    let toggle_open = move |_| {
        let opening = !is_open.get_untracked();
        if opening {
            if let Some(el) = button_ref.get_untracked() {
                let rect = el.get_bounding_client_rect();
                anchor_rect.set(Some((rect.left(), rect.top(), rect.right(), rect.bottom())));
            }
            let current = signal.get_untracked();
            let selected_idx =
                options.with_value(|opts| opts.iter().position(|(variant, _)| *variant == current));
            highlighted_index.set(Some(selected_idx.unwrap_or(0)));
        }
        is_open.update(|v| *v = !*v);
    };

    let move_highlight = move |delta: i32| {
        let len = options.with_value(|opts| opts.len());
        if len == 0 {
            return;
        }
        let current = highlighted_index.get_untracked().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len as i32) as usize;
        highlighted_index.set(Some(next));
    };

    let confirm_highlighted = move || {
        let variant = options.with_value(|opts| {
            highlighted_index
                .get_untracked()
                .and_then(|idx| opts.get(idx))
                .map(|(variant, _)| variant.clone())
        });
        if let Some(variant) = variant {
            field.set(variant);
        }
        is_open.set(false);
    };

    // Type-ahead: letters typed while the panel is open accumulate into a
    // buffer that's cleared after a pause, so typing quickly narrows the
    // match instead of restarting it on every keystroke.
    let type_buffer = RwSignal::new(String::new());
    let type_buffer_seq = RwSignal::new(0u64);

    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if !is_open.try_get_untracked().unwrap_or(false) {
            return;
        }

        match ev.key().as_str() {
            "Escape" => {
                is_open.set(false);
                return;
            }
            "Enter" => {
                ev.prevent_default();
                confirm_highlighted();
                return;
            }
            "ArrowDown" => {
                ev.prevent_default();
                move_highlight(1);
                return;
            }
            "ArrowUp" => {
                ev.prevent_default();
                move_highlight(-1);
                return;
            }
            _ => {}
        }

        if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
            return;
        }
        let Some(ch) = ev
            .key()
            .chars()
            .next()
            .filter(|c| c.is_alphanumeric() && ev.key().chars().count() == 1)
        else {
            return;
        };

        let mut buffer = type_buffer.get_untracked();
        buffer.push(ch.to_ascii_lowercase());
        type_buffer.set(buffer.clone());

        let seq = type_buffer_seq.get_untracked() + 1;
        type_buffer_seq.set(seq);
        set_timeout(
            move || {
                if type_buffer_seq.get_untracked() == seq {
                    type_buffer.set(String::new());
                }
            },
            std::time::Duration::from_millis(600),
        );

        let matched_idx = options.with_value(|opts| {
            opts.iter()
                .position(|(_, label)| label.to_lowercase().starts_with(&buffer))
        });

        if let Some(idx) = matched_idx {
            highlighted_index.set(Some(idx));
        }
    });

    let panel_style = move || {
        let Some((left, top, right, bottom)) = anchor_rect.get() else {
            return String::new();
        };

        let win = web_sys::window().unwrap();
        let vw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let vh = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);
        let offset = 4.0;

        let panel_w = (right - left).max(180.0);
        let max_h = 260.0_f64;
        let space_below = vh - bottom;
        let space_above = top;

        let y_style = if space_below < max_h.min(240.0) && space_above > space_below {
            format!("bottom:{}px;", vh - top + offset)
        } else {
            format!("top:{}px;", bottom + offset)
        };
        let x_style = if left + panel_w <= vw - offset {
            format!("left:{left}px;")
        } else {
            format!("left:{}px;", (vw - offset - panel_w).max(offset))
        };

        format!("{x_style}{y_style}width:{panel_w}px;max-height:{max_h}px;")
    };

    let (cloud_icon, cloud_color, cloud_tooltip) = get_cloud_stuff(field.uses_cloud);

    view! {
        <div style="display: contents;">
            <label class="flex justify-between gap-2 cursor-pointer border-transparent hover:border-(--tile-border-color) border transition-colors duration-100 rounded-lg p-(--gap) items-center hover:bg-(--tile-hover-color) text-dim hover:text-normal">
                <span class="inline-flex items-center gap-2">
                    <span class="select-none">{name}</span>
                    <div title=description class="flex items-center">
                        <Icon icon=QUESTION size="14px" color="var(--dim-text-color)" />
                    </div>
                </span>

                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        node_ref=button_ref
                        on:click=toggle_open
                        class="flex items-center gap-2 min-w-50 px-3 py-1.5 rounded-lg border border-(--tile-border-color) bg-(--ui-solid-bg) text-normal text-sm cursor-pointer hover:border-(--accent-color) transition-colors duration-100 justify-between"
                    >
                        <span class="select-none">{current_label}</span>
                        <span
                            class="flex items-center transition-transform duration-100"
                            class=("rotate-180", move || is_open.get())
                        >
                            <Icon icon=CARET_DOWN size="12px" color="var(--dim-text-color)" />
                        </span>
                    </button>
                    <div title=cloud_tooltip>
                        <LIcon icon=cloud_icon style=cloud_color height="18px" />
                    </div>
                </div>
            </label>

            <Show when=move || is_open.get()>
                <Portal>
                    <div class="fixed inset-0 z-[999]" on:click=move |_| is_open.set(false) />
                    <div
                        class="fixed z-[1000] flex flex-col gap-0.5 p-1 overflow-y-auto bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--floating-border-radius) shadow-lg"
                        style=panel_style
                    >
                        {move || {
                            options
                                .get_value()
                                .into_iter()
                                .enumerate()
                                .map(|(idx, (variant, label))| {
                                    let is_selected = variant == signal.get();
                                    let is_highlighted = highlighted_index.get() == Some(idx);
                                    let selected_variant = variant.clone();
                                    let row_ref: NodeRef<Button> = NodeRef::new();
                                    Effect::new(move |_| {
                                        if !is_highlighted {
                                            return;
                                        }
                                        let Some(el) = row_ref.get() else {
                                            return;
                                        };
                                        let scroll_opts = ScrollIntoViewOptions::new();
                                        scroll_opts.set_behavior(ScrollBehavior::Smooth);
                                        scroll_opts.set_block(ScrollLogicalPosition::Nearest);
                                        el.scroll_into_view_with_scroll_into_view_options(
                                            &scroll_opts,
                                        );
                                    });

                                    // Whenever this list is rebuilt (e.g. type-ahead or the arrow
                                    // keys move the keyboard cursor), scroll it into view.

                                    view! {
                                        <button
                                            type="button"
                                            node_ref=row_ref
                                            class="flex items-center justify-between gap-4 text-left px-2 py-1.5 rounded-lg text-sm cursor-pointer border border-transparent hover:bg-(--ui-solid-hover-bg)"
                                            class=("text-normal", is_selected)
                                            class=("text-dim", !is_selected)
                                            class=("bg-(--ui-solid-hover-bg)", is_selected)
                                            class=("border-(--accent-color)", is_highlighted)
                                            on:click=move |_| {
                                                highlighted_index.set(Some(idx));
                                                field.set(selected_variant.clone());
                                                is_open.set(false);
                                            }
                                        >
                                            <span class="select-none">{label}</span>
                                            <Show when=move || is_selected>
                                                <Icon icon=CHECK size="12px" color="var(--accent-color)" />
                                            </Show>
                                        </button>
                                    }
                                        .into_any()
                                })
                                .collect_view()
                        }}
                    </div>
                </Portal>
            </Show>
        </div>
    }
    .into_any()
}

#[component]
pub fn SubSection<'a>(title: &'a str, children: Children) -> AnyView {
    let expanded = RwSignal::new(true);

    view! {
        <div
            class="w-full flex items-center justify-between cursor-pointer select-none group pr-(--gap)"
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
    let signal = field.signal();
    let name = field.human_readable;
    let description = field.description;

    render_toggle(
        move |v| field.set(v),
        signal,
        name,
        description,
        Some(field.uses_cloud),
    )
}

#[component]
pub fn Dropdown<T: EnumVariants + Clone + PartialEq + Send + Sync + 'static>(
    field: MatrixSettingField<T>,
) -> AnyView {
    let options = T::variants()
        .map(|(variant, label)| (variant, label.to_string()))
        .collect();
    render_dropdown_from_options(field, options)
}

#[component]
pub fn Spacer() -> AnyView {
    view! { <div class="h-8"></div> }.into_any()
}

#[component]
pub fn EnumToggle<D>(
    field: MatrixSettingField<HashMap<D, bool>>,
    #[prop(optional)] modes: Vec<(&'static str, &'static [D])>,
) -> AnyView
where
    D: EnumVariants + Copy + Eq + std::hash::Hash + Send + Sync + 'static,
{
    let variants = D::variants().collect();
    render_enum_toggle_from_options(field, variants, modes)
}

fn render_enum_toggle_from_options<D>(
    field: MatrixSettingField<HashMap<D, bool>>,
    variants: Vec<(D, &'static str)>,
    modes: Vec<(&'static str, &'static [D])>,
) -> AnyView
where
    D: Copy + Eq + std::hash::Hash + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    let signal = field.signal();
    let variants = StoredValue::new(variants);
    let modes = StoredValue::new(modes);
    let name = field.human_readable;
    let description = field.description;

    let toggle_variant = move |variant: D| {
        let mut map = signal.get_untracked();
        let entry = map.entry(variant).or_insert(false);
        *entry = !*entry;
        field.set(map);
    };

    let apply_mode = move |mode_variants: &'static [D]| {
        let map = variants.with_value(|vs| {
            vs.iter()
                .map(|(variant, _)| (*variant, mode_variants.contains(variant)))
                .collect()
        });
        field.set(map);
    };

    let (cloud_icon, cloud_color, cloud_tooltip) = get_cloud_stuff(field.uses_cloud);

    let variants_selected = move |variants: &[D]| {
        signal
            .get()
            .iter()
            .all(|(var, enabled)| variants.contains(var) == *enabled)
    };

    view! {
        <div class="flex flex-col gap-2 rounded-lg p-(--gap)">
            <div class="flex flex-row gap-2">
                <div class="flex gap-2 items-center">
                    <span class="inline-flex items-center gap-2">
                        <span class="select-none text-normal">{name}</span>
                        <div title=description class="flex items-center">
                            <Icon icon=QUESTION size="14px" color="var(--dim-text-color)" />
                        </div>
                    </span>
                </div>

                <Show when=move || modes.with_value(|m| !m.is_empty())>
                    <div class="flex flex-wrap gap-2">
                        {move || {
                            modes
                                .get_value()
                                .into_iter()
                                .map(|(label, mode_variants)| {
                                    view! {
                                        <button
                                            type="button"
                                            on:click=move |_| apply_mode(mode_variants)
                                            class="px-2 py-1 rounded-lg border border-(--tile-border-color) text-dim hover:text-normal hover:border-(--accent-color) text-xs cursor-pointer transition-colors duration-100"
                                            class=(
                                                "bg-(--ui-solid-hover-bg)",
                                                move || variants_selected(mode_variants),
                                            )
                                            class=(
                                                "text-normal",
                                                move || variants_selected(mode_variants),
                                            )
                                        >
                                            {label}
                                        </button>
                                    }
                                        .into_any()
                                })
                                .collect_view()
                        }}
                    </div>
                </Show>
                <div class="flex-1" />
                <div title=cloud_tooltip>
                    <LIcon icon=cloud_icon style=cloud_color height="18px" />
                </div>
            </div>

            <div class="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2">
                {move || {
                    variants
                        .get_value()
                        .into_iter()
                        .map(|(variant, label)| {
                            let is_enabled = move || {
                                signal.with(|map| map.get(&variant).copied().unwrap_or(false))
                            };

                            view! {
                                <button
                                    type="button"
                                    on:click=move |_| toggle_variant(variant)
                                    class="h-9 px-2 flex items-center justify-center rounded-lg border text-sm truncate cursor-pointer transition-colors duration-200 border-(--tile-border-color) hover:border-(--accent-color)"
                                    class=("text-(--success-color)", is_enabled)
                                    class=("bg-(--success-bg-color)", is_enabled)
                                    class=("text-dim", move || !is_enabled())
                                >
                                    {label}
                                </button>
                            }
                                .into_any()
                        })
                        .collect_view()
                }}
            </div>
        </div>
    }
    .into_any()
}

// #[component]
// pub fn DropdownWithValues(
//     field: MatrixSettingField<String>,
//     values: Vec<(String, String)>,
// ) -> AnyView {
//     let options = values.into_iter().map(|(label, value)| (value, label)).collect();
//     render_dropdown_from_options(field, options)
// }
