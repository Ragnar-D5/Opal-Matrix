use std::cell::RefCell;
use std::rc::Rc;

use klipy::{File, MediaItem};
use leptos::{
    html::{Div, Input},
    prelude::*,
    task::spawn_local,
};
use leptos_use::{use_intersection_observer, UseIntersectionObserverReturn};
use phosphor_leptos::{Icon, IconWeight, MAGNIFYING_GLASS, SMILEY_SAD};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Element, IntersectionObserverEntry};

use crate::tauri_functions::get_gifs;

#[derive(Clone, Copy)]
pub struct GifPickerState {
    resolve: RwSignal<Option<js_sys::Function>>,
    anchor_rect: RwSignal<Option<(f64, f64, f64, f64)>>,
}

impl Default for GifPickerState {
    fn default() -> Self {
        Self {
            resolve: RwSignal::new(None),
            anchor_rect: RwSignal::new(None),
        }
    }
}

impl GifPickerState {
    fn close(&self, strings: Option<(&str, &str, u64)>) {
        if let Some(resolve) = self.resolve.get_untracked() {
            let val = match strings {
                // Return entire file object
                Some(u) => match serde_json::to_string(&u) {
                    Ok(json_str) => JsValue::from_str(&json_str),
                    Err(e) => {
                        leptos::logging::error!("Failed to serialize file for GIF picker: {e}");
                        JsValue::NULL
                    }
                },
                None => JsValue::NULL,
            };
            let _ = resolve.call1(&JsValue::UNDEFINED, &val);
        }
        self.resolve.set(None);
        self.anchor_rect.set(None);
    }
}

pub async fn pick_gif(anchor: &Element, state: GifPickerState) -> Option<String> {
    let rect = anchor.get_bounding_client_rect();
    state
        .anchor_rect
        .set(Some((rect.left(), rect.top(), rect.right(), rect.bottom())));

    let holder: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
    let holder_clone = holder.clone();

    let promise = js_sys::Promise::new(&mut move |resolve, _| {
        *holder_clone.borrow_mut() = Some(resolve);
    });

    if let Some(resolve) = holder.borrow().clone() {
        state.resolve.set(Some(resolve));
    }

    let result = JsFuture::from(promise).await.ok()?;

    if result.is_null() || result.is_undefined() {
        None
    } else {
        result.as_string()
    }
}

fn preview_file(item: &MediaItem) -> Option<File> {
    let sizes = &item.file;
    let file = sizes
        .sm
        .as_ref()
        .and_then(|f| f.webp.as_ref().or(f.gif.as_ref()))
        .or_else(|| {
            sizes
                .md
                .as_ref()
                .and_then(|f| f.webp.as_ref().or(f.gif.as_ref()))
        })
        .or_else(|| {
            sizes
                .hd
                .as_ref()
                .and_then(|f| f.webp.as_ref().or(f.gif.as_ref()))
        });
    file.cloned()
}

fn send_file(item: &MediaItem) -> Option<File> {
    let sizes = &item.file;
    sizes
        .md
        .as_ref()
        .and_then(|f| f.gif.as_ref())
        .or_else(|| sizes.hd.as_ref().and_then(|f| f.gif.as_ref()))
        .or_else(|| sizes.sm.as_ref().and_then(|f| f.gif.as_ref()))
        .or_else(|| sizes.md.as_ref().and_then(|f| f.webp.as_ref()))
        .or_else(|| sizes.hd.as_ref().and_then(|f| f.webp.as_ref()))
        .or_else(|| sizes.sm.as_ref().and_then(|f| f.webp.as_ref()))
        .cloned()
}

const SKELETON_HEIGHTS_LEFT: &[u32] = &[100, 175, 130, 100];
const SKELETON_HEIGHTS_RIGHT: &[u32] = &[130, 100, 175, 130];

#[component]
pub fn GifPickerPortal() -> impl IntoView {
    let state: GifPickerState = expect_context();
    let search = RwSignal::new(String::new());
    let current_page = RwSignal::new(0u32);
    let gifs: RwSignal<Vec<MediaItem>> = RwSignal::new(Vec::new());
    let has_next = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let search_ref: NodeRef<Input> = NodeRef::new();
    let sentinel_ref: NodeRef<Div> = NodeRef::new();
    let request_gen: RwSignal<u32> = RwSignal::new(0);

    let do_search = move |term: String, pg: u32, append: bool| {
        let my_request_gen = request_gen.get_untracked() + 1;
        request_gen.set(my_request_gen);
        loading.set(true);
        if !append {
            gifs.set(Vec::new());
        }
        spawn_local(async move {
            match get_gifs(term, pg).await {
                Ok(page) => {
                    if request_gen.get_untracked() != my_request_gen {
                        return;
                    }
                    has_next.set(page.has_next);
                    let items: Vec<MediaItem> = page.content_items().cloned().collect();
                    if append {
                        gifs.update(|g| g.extend(items));
                    } else {
                        gifs.set(items);
                    }
                }
                Err(e) => {
                    if request_gen.get_untracked() != my_request_gen {
                        return;
                    }
                    leptos::logging::error!("Failed to fetch GIFs: {e}");
                    has_next.set(false);
                }
            }
            loading.set(false);
        });
    };

    let UseIntersectionObserverReturn { .. } = use_intersection_observer(
        sentinel_ref,
        move |entries: Vec<IntersectionObserverEntry>, _| {
            if entries[0].is_intersecting() && has_next.get_untracked() && !loading.get_untracked()
            {
                let new_page = current_page.get_untracked() + 1;
                current_page.set(new_page);
                do_search(search.get_untracked(), new_page, true);
            }
        },
    );

    Effect::new(move |_| {
        if state.resolve.get().is_some() {
            search.set(String::new());
            current_page.set(0);
            do_search(String::new(), 0, false);
            if let Some(el) = search_ref.get() {
                let _ = el.focus();
            }
        }
    });

    let style = move || {
        let Some((left, top, right, bottom)) = state.anchor_rect.get() else {
            return String::new();
        };

        let win = web_sys::window().unwrap();
        let vw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let vh = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);

        let picker_w: f64 = 500.0;
        let picker_h: f64 = 450.0;
        let offset = 21.0;

        let actual_w = picker_w.min(vw - offset * 2.0);
        let actual_h = picker_h.min(vh - offset * 2.0);

        let space_below = vh - bottom;
        let space_above = top;
        let place_below = space_below >= actual_h + offset || space_below > space_above;

        let y_style = if place_below {
            format!("top:{}px;", bottom + offset)
        } else {
            format!("bottom:{}px;", vh - top + offset)
        };

        let target_right = right + offset;
        let target_left = left - offset;

        let x_style = if target_right <= vw - offset && target_right - actual_w >= offset {
            format!("right:{}px;", vw - target_right)
        } else if target_left >= offset && target_left + actual_w <= vw - offset {
            format!("left:{}px;", target_left)
        } else {
            let clamped_right = target_right.min(vw - offset).max(actual_w + offset);
            format!("right:{}px;", vw - clamped_right)
        };

        format!("{x_style}{y_style}width:{actual_w}px;height:{actual_h}px;")
    };

    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" {
            state.close(None);
        }

        let term = search.get_untracked();
        current_page.set(0);
        do_search(term, 0, false);
    };

    view! {
        <Show when=move || state.resolve.get().is_some()>
            <div class="fixed inset-0 z-[999]" on:click=move |_| state.close(None) />

            <div
                class="fixed z-[1000] flex flex-col bg-(--ui-floating-hover-bg) backdrop-blur-2xl border border-(--tile-border-color) rounded-(--floating-border-radius) shadow-xl overflow-hidden"
                style=style
            >
                <div class="p-2 border-b border-(--tile-border-color) flex-shrink-0">
                    <div class="relative flex items-center">
                        <div class="absolute left-2 flex items-center pointer-events-none text-(--muted-text-color)">
                            <Icon icon=MAGNIFYING_GLASS weight=IconWeight::Bold size="14px" />
                        </div>
                        <input
                            type="text"
                            node_ref=search_ref
                            placeholder="Search KLIPY"
                            class="w-full bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--ui-border-radius) pl-7 pr-2 py-1 text-sm text-(--bright-text-color) outline-none placeholder:text-muted"
                            on:keydown=on_keydown
                            on:input=move |ev| {
                                let el = ev
                                    .target()
                                    .unwrap()
                                    .dyn_into::<web_sys::HtmlInputElement>()
                                    .unwrap();
                                search.set(el.value());
                            }
                            prop:value=move || search.get()
                        />
                        <img
                            src="public/powered_by_klipy.svg"
                            class="absolute right-2 opacity-35 h-1/2"
                        />
                    </div>
                </div>

                <div class="overflow-y-auto flex-1 p-1.5" style="scrollbar-width: thin;">
                    <Show when=move || gifs.get().is_empty() && !loading.get()>
                        <div class="w-full h-full flex flex-col items-center justify-center text-(--muted-text-color) gap-2 pt-12">
                            <Icon icon=SMILEY_SAD size="60px" weight=IconWeight::Thin />
                            <span class="text-sm">"No GIFs found"</span>
                        </div>
                    </Show>

                    <div style="display: flex; gap: 4px;">
                        <div style="flex: 1; min-width: 0;">
                            <For
                                each=move || {
                                    gifs.get()
                                        .into_iter()
                                        .enumerate()
                                        .filter_map(|(i, g)| (i % 2 == 0).then_some(g))
                                        .collect::<Vec<_>>()
                                }
                                key=|gif| gif.id
                                children=move |gif| {
                                    let Some(file) = preview_file(&gif) else {
                                        return ().into_any();
                                    };
                                    let loaded = RwSignal::new(false);
                                    let title = gif.title.clone();
                                    let url_click = file.url.clone();
                                    let Some(send_file) = send_file(&gif) else {
                                        return ().into_any();
                                    };
                                    view! {
                                        <div
                                            style="margin-bottom: 4px; position: relative; cursor: pointer;"
                                            class="group rounded overflow-hidden"
                                            on:click=move |_| {
                                                log::debug!("Selected GIF: {} — {}", title, url_click);
                                                state
                                                    .close(Some((&send_file.url, &gif.title, send_file.size)));
                                            }
                                        >
                                            <img
                                                src=file.url
                                                width=file.width
                                                height=file.height
                                                style="width: 100%; height: auto; display: block;"
                                                class="group-hover:opacity-75 transition-opacity rounded"
                                                on:load=move |_| loaded.set(true)
                                            />
                                            <Show when=move || !loaded.get()>
                                                <div
                                                    style="position: absolute; inset: 0;"
                                                    class="animate-pulse bg-(--ui-hover-bg)"
                                                />
                                            </Show>
                                        </div>
                                    }
                                        .into_any()
                                }
                            />
                            <Show when=move || {
                                loading.get()
                            }>
                                {SKELETON_HEIGHTS_LEFT
                                    .iter()
                                    .map(|&h| {
                                        view! {
                                            <div
                                                style=format!("height: {h}px; margin-bottom: 4px;")
                                                class="animate-pulse bg-(--ui-hover-bg) rounded"
                                            />
                                        }
                                    })
                                    .collect_view()}
                            </Show>
                        </div>

                        <div style="flex: 1; min-width: 0;">
                            <For
                                each=move || {
                                    gifs.get()
                                        .into_iter()
                                        .enumerate()
                                        .filter_map(|(i, g)| (i % 2 != 0).then_some(g))
                                        .collect::<Vec<_>>()
                                }
                                key=|gif| gif.id
                                children=move |gif| {
                                    let Some(file) = preview_file(&gif) else {
                                        return ().into_any();
                                    };
                                    let loaded = RwSignal::new(false);
                                    let title = gif.title.clone();
                                    let url_click = file.url.clone();
                                    let Some(send_file) = send_file(&gif) else {
                                        return ().into_any();
                                    };

                                    view! {
                                        <div
                                            style="margin-bottom: 4px; position: relative; cursor: pointer;"
                                            class="group rounded overflow-hidden"
                                            on:click=move |_| {
                                                log::debug!("Selected GIF: {} — {}", title, url_click);
                                                state
                                                    .close(Some((&send_file.url, &gif.title, send_file.size)));
                                            }
                                        >
                                            <img
                                                src=file.url
                                                width=file.width
                                                height=file.height
                                                style="width: 100%; height: auto; display: block;"
                                                class="group-hover:opacity-75 transition-opacity rounded"
                                                on:load=move |_| loaded.set(true)
                                            />
                                            <Show when=move || !loaded.get()>
                                                <div
                                                    style="position: absolute; inset: 0;"
                                                    class="animate-pulse bg-(--ui-hover-bg)"
                                                />
                                            </Show>
                                        </div>
                                    }
                                        .into_any()
                                }
                            />
                            <Show when=move || {
                                loading.get()
                            }>
                                {SKELETON_HEIGHTS_RIGHT
                                    .iter()
                                    .map(|&h| {
                                        view! {
                                            <div
                                                style=format!("height: {h}px; margin-bottom: 4px;")
                                                class="animate-pulse bg-(--ui-hover-bg) rounded"
                                            />
                                        }
                                    })
                                    .collect_view()}
                            </Show>
                        </div>
                    </div>

                    <div node_ref=sentinel_ref class="h-1 w-full" />
                </div>
            </div>
        </Show>
    }
}
