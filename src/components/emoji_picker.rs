use std::cell::RefCell;
use std::rc::Rc;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::Element;

use emojis::Group;

#[derive(Clone, Copy)]
pub struct EmojiPickerState {
    resolve: RwSignal<Option<js_sys::Function>>,
    anchor_rect: RwSignal<Option<(f64, f64, f64, f64)>>,
}

impl EmojiPickerState {
    fn new() -> Self {
        Self {
            resolve: RwSignal::new(None),
            anchor_rect: RwSignal::new(None),
        }
    }

    fn close(&self, emoji: Option<&str>) {
        if let Some(resolve) = self.resolve.get_untracked() {
            let val = match emoji {
                Some(e) => JsValue::from_str(e),
                None => JsValue::NULL,
            };
            let _ = resolve.call1(&JsValue::UNDEFINED, &val);
        }
        self.resolve.set(None);
        self.anchor_rect.set(None);
    }
}

/// Open the picker anchored near `anchor`. Returns the selected emoji or `None` if dismissed.
pub async fn pick_emoji(anchor: &Element) -> Option<String> {
    let state: EmojiPickerState = expect_context();

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

const PICKER_W: f64 = 320.0;
const PICKER_H: f64 = 380.0;

fn group_icon(group: emojis::Group) -> &'static str {
    match group {
        Group::SmileysAndEmotion => "😀",
        Group::PeopleAndBody => "👋",
        Group::AnimalsAndNature => "🐶",
        Group::FoodAndDrink => "🍕",
        Group::TravelAndPlaces => "🚀",
        Group::Activities => "🎉",
        Group::Objects => "💡",
        Group::Symbols => "❤️",
        Group::Flags => "🏳️",
    }
}

const ALL_GROUPS: &[emojis::Group] = &[
    Group::SmileysAndEmotion,
    Group::PeopleAndBody,
    Group::AnimalsAndNature,
    Group::FoodAndDrink,
    Group::TravelAndPlaces,
    Group::Activities,
    Group::Objects,
    Group::Symbols,
    Group::Flags,
];

#[component]
fn EmojiPickerPortal() -> impl IntoView {
    let state: EmojiPickerState = expect_context();
    let search = RwSignal::new(String::new());
    let active_group: RwSignal<Group> = RwSignal::new(Group::SmileysAndEmotion);

    Effect::new(move |_| {
        if state.resolve.get().is_some() {
            search.set(String::new());
            active_group.set(Group::SmileysAndEmotion);
        }
    });

    let style = move || {
        let Some((left, top, _, bottom)) = state.anchor_rect.get() else {
            return String::new();
        };
        let win = web_sys::window().unwrap();
        let vw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let x = left.min(vw - PICKER_W - 8.0).max(8.0);
        let y = if top >= PICKER_H + 8.0 {
            top - PICKER_H - 8.0
        } else {
            bottom + 8.0
        };
        format!("left:{x}px;top:{y}px;width:{PICKER_W}px;")
    };

    view! {
        <Show when=move || state.resolve.get().is_some()>
            // backdrop
            <div class="fixed inset-0 z-[999]" on:click=move |_| state.close(None) />

            // picker panel
            <div
                class="fixed z-[1000] flex flex-col bg-(--ui-floating-hover-bg) backdrop-blur-2xl border border-(--tile-border-color) rounded-(--floating-border-radius) shadow-xl overflow-hidden"
                style=style
            >
                // search bar
                <div class="p-2 border-b border-(--tile-border-color) flex-shrink-0">
                    <input
                        type="text"
                        placeholder="Search emoji..."
                        class="w-full bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--ui-border-radius) px-2 py-1 text-sm text-(--bright-text-color) outline-none"
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
                </div>

                // category tabs — hidden while searching
                <Show when=move || search.get().is_empty()>
                    <div
                        class="flex flex-row px-1 pt-1 gap-0.5 border-b border-(--tile-border-color) flex-shrink-0 overflow-x-auto"
                        style="scrollbar-width:none;"
                    >
                        {ALL_GROUPS
                            .iter()
                            .map(|&group| {
                                view! {
                                    <button
                                        class="p-1.5 rounded text-base cursor-pointer flex-shrink-0 transition-colors"
                                        class=(
                                            "bg-(--ui-hover-bg)",
                                            move || active_group.get() == group,
                                        )
                                        title=format!("{group:?}")
                                        on:click=move |_| active_group.set(group)
                                    >
                                        {group_icon(group)}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                </Show>

                // emoji grid
                <div class="overflow-y-auto flex-1 p-2">
                    {move || {
                        let q = search.get().to_lowercase();
                        let emojis: Vec<&'static emojis::Emoji> = if q.is_empty() {
                            emojis::iter()
                                .filter(|e| e.group() == active_group.get())
                                .collect()
                        } else {
                            emojis::iter()
                                .filter(|e| e.name().to_lowercase().contains(q.as_str()))
                                .collect()
                        };
                        view! {
                            <div class="grid grid-cols-8 gap-0.5">
                                {emojis
                                    .into_iter()
                                    .map(|emoji| {
                                        let s = emoji.as_str().to_string();
                                        let for_click = s.clone();
                                        view! {
                                            <button
                                                class="text-xl w-8 h-8 flex items-center justify-center rounded hover:bg-(--ui-hover-bg) cursor-pointer transition-colors"
                                                title=emoji.name()
                                                on:click=move |_| state.close(Some(&for_click))
                                            >
                                                {s}
                                            </button>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                    }}
                </div>
            </div>
        </Show>
    }
}
