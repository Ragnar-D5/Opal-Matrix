use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use leptos::{html::Input, prelude::*};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use phosphor_leptos::{
    Icon, IconWeight, IconWeightData, AIRPLANE, CARET_DOWN, CUBE, FLAG, GAME_CONTROLLER, HAMBURGER,
    HEART, MAGNIFYING_GLASS, PERSON, PLANT, SMILEY, SMILEY_SAD,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Element;

use emojis::Group;

#[derive(Clone, Copy)]
pub struct EmojiPickerState {
    resolve: RwSignal<Option<js_sys::Function>>,
    anchor_rect: RwSignal<Option<(f64, f64, f64, f64)>>,
}

impl Default for EmojiPickerState {
    fn default() -> Self {
        Self {
            resolve: RwSignal::new(None),
            anchor_rect: RwSignal::new(None),
        }
    }
}

impl EmojiPickerState {
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
pub async fn pick_emoji(anchor: &Element, state: EmojiPickerState) -> Option<String> {
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

fn group_icon<'a>(group: Group) -> &'a IconWeightData {
    match group {
        Group::SmileysAndEmotion => SMILEY,
        Group::PeopleAndBody => PERSON,
        Group::AnimalsAndNature => PLANT,
        Group::FoodAndDrink => HAMBURGER,
        Group::TravelAndPlaces => AIRPLANE,
        Group::Activities => GAME_CONTROLLER,
        Group::Objects => CUBE,
        Group::Symbols => HEART,
        Group::Flags => FLAG,
    }
}

fn group_name(group: Group) -> &'static str {
    match group {
        Group::SmileysAndEmotion => "Smileys & Emotion",
        Group::PeopleAndBody => "People & Body",
        Group::AnimalsAndNature => "Animals & Nature",
        Group::FoodAndDrink => "Food & Drink",
        Group::TravelAndPlaces => "Travel & Places",
        Group::Activities => "Activities",
        Group::Objects => "Objects",
        Group::Symbols => "Symbols",
        Group::Flags => "Flags",
    }
}

const ALL_GROUPS: &[Group] = &[
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

fn fuzzy_match_emojis(query: &str, matcher: &mut Matcher) -> Vec<&'static emojis::Emoji> {
    let mut needle_buf = Vec::new();
    let mut haystack_buf = Vec::new();
    let needle = Utf32Str::new(query, &mut needle_buf);

    let mut matched: Vec<(u16, &'static emojis::Emoji)> = emojis::iter()
        .filter_map(|emoji| {
            let haystack = Utf32Str::new(emoji.name(), &mut haystack_buf);
            let name_score = matcher.fuzzy_match(haystack, needle);

            let shortcode_score = emoji
                .shortcodes()
                .map(|shortcode| {
                    let haystack = Utf32Str::new(shortcode, &mut haystack_buf);
                    matcher.fuzzy_match(haystack, needle)
                })
                .max()
                .unwrap_or_default();

            name_score.max(shortcode_score).map(|score| (score, emoji))
        })
        .collect();

    matched.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
    matched.into_iter().map(|(_, emoji)| emoji).collect()
}

#[component]
pub fn EmojiPickerPortal() -> impl IntoView {
    let state: EmojiPickerState = expect_context();
    let search = RwSignal::new(String::new());
    let active_group: RwSignal<Group> = RwSignal::new(Group::SmileysAndEmotion);

    let collapsed_groups: RwSignal<HashSet<Group>> = RwSignal::new(HashSet::new());

    let matcher = StoredValue::new(Matcher::new(Config::DEFAULT));
    let search_ref: NodeRef<Input> = NodeRef::new();

    Effect::new(move |_| {
        if state.resolve.get().is_some() {
            search.set(String::new());
            active_group.set(Group::SmileysAndEmotion);
            collapsed_groups.set(std::collections::HashSet::new());

            if let Some(el) = search_ref.get() {
                let _ = el.focus();
            }
        }
    });

    window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
        if state.resolve.try_get_untracked().flatten().is_some() && ev.key() == "Escape" {
            state.close(None);
        }
    });

    let style = move || {
        let Some((left, top, right, bottom)) = state.anchor_rect.get() else {
            return String::new();
        };

        let win = web_sys::window().unwrap();
        let vw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let vh = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);

        let picker_dim: f64 = 350.0;
        let offset = 21.0;

        // Clamp the dimensions so it doesn't break on extreme mobile screens
        let actual_w = picker_dim.min(vw - offset * 2.0);
        let actual_h = picker_dim.min(vh - offset * 2.0);

        // --- Vertical Logic ---
        let space_below = vh - bottom;
        let space_above = top;
        let place_below = space_below >= actual_h + offset || space_below > space_above;

        let y_style = if place_below {
            format!("top:{}px;", bottom + offset)
        } else {
            format!("bottom:{}px;", vh - top + offset)
        };

        // --- Horizontal Logic ---
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

        // Apply explicit width and height
        format!("{x_style}{y_style}width:{actual_w}px;height:{actual_h}px;")
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
                    <div class="relative flex items-center">
                        <div class="absolute left-2 flex items-center pointer-events-none text-muted">
                            <Icon icon=MAGNIFYING_GLASS weight=IconWeight::Bold size="14px" />
                        </div>
                        <input
                            type="text"
                            node_ref=search_ref
                            placeholder="Search emoji..."
                            class="w-full bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--ui-border-radius) pl-7 pr-2 py-1 text-sm text-bright outline-none"
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
                </div>

                // main layout for sidebar + continuous scroll grid
                <div class="flex flex-row overflow-hidden flex-1">

                    <div
                        class="flex flex-col px-1 pt-1 pb-2 gap-1 border-r border-(--tile-border-color) flex-shrink-0 overflow-y-auto"
                        style="scrollbar-width:none;"
                    >
                        {ALL_GROUPS
                            .iter()
                            .map(|&group| {
                                view! {
                                    <button
                                        class="p-1.5 rounded text-base cursor-pointer flex-shrink-0 transition-colors text-muted hover:text-bright"
                                        class=(
                                            "bg-(--ui-hover-bg)",
                                            move || active_group.get() == group,
                                        )
                                        title=format!("{group:?}")
                                        on:click=move |_| {
                                            active_group.set(group);
                                            search.set(String::new());
                                            if let Some(el) = search_ref.get() {
                                                let _ = el.focus();
                                            }
                                            if let Some(window) = web_sys::window()
                                                && let Some(document) = window.document()
                                            {
                                                let id = format!("emoji-group-{group:?}");
                                                if let Some(el) = document.get_element_by_id(&id) {
                                                    el.scroll_into_view();
                                                }
                                            }
                                        }
                                    >
                                        <Icon icon=group_icon(group) weight=IconWeight::Fill />
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>

                    // emoji grid
                    <div
                        class="overflow-y-auto flex-1 p-2 relative"
                        style="scroll-behavior: smooth;"
                    >
                        {move || {
                            let q = search.get().to_lowercase();
                            if q.is_empty() {

                                view! {
                                    <div class="flex flex-col pb-4">
                                        {ALL_GROUPS
                                            .iter()
                                            .map(|&group| {
                                                let group_emojis: Vec<_> = emojis::iter()
                                                    .filter(|e| e.group() == group)
                                                    .collect();

                                                view! {
                                                    <div id=format!("emoji-group-{group:?}")>
                                                        // Sticky group header / dropdown trigger
                                                        <button
                                                            class="sticky top-[-8px] z-10 w-full flex items-center bg-(--ui-floating-hover-bg) backdrop-blur-md py-1 mb-1 px-1 text-sm font-semibold text-normal hover:text-bright cursor-pointer transition-colors"
                                                            on:click=move |_| {
                                                                if let Some(el) = search_ref.get() {
                                                                    let _ = el.focus();
                                                                }
                                                                collapsed_groups
                                                                    .update(|set| {
                                                                        if set.contains(&group) {
                                                                            set.remove(&group);
                                                                        } else {
                                                                            set.insert(group);
                                                                        }
                                                                    });
                                                            }
                                                        >
                                                            <div class="flex items-center gap-2">
                                                                <Icon icon=group_icon(group) weight=IconWeight::Fill />
                                                                <span>{group_name(group)}</span>
                                                            </div>
                                                            <div
                                                                class="transition-transform duration-200 flex items-center justify-center"
                                                                class=(
                                                                    "-rotate-90",
                                                                    move || collapsed_groups.with(|set| set.contains(&group)),
                                                                )
                                                            >
                                                                <Icon icon=CARET_DOWN weight=IconWeight::Bold />
                                                            </div>
                                                        </button>

                                                        // Section grid — Hides when the group is in the collapsed set
                                                        <div
                                                            class="grid grid-cols-8 gap-0.5"
                                                            class=(
                                                                "hidden",
                                                                move || collapsed_groups.with(|set| set.contains(&group)),
                                                            )
                                                            class=(
                                                                "mb-4",
                                                                move || !collapsed_groups.with(|set| set.contains(&group)),
                                                            )
                                                        >
                                                            {group_emojis
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
                                                    </div>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            } else {
                                let emojis = matcher
                                    .try_update_value(|m| fuzzy_match_emojis(&q, m))
                                    .unwrap_or_default();
                                if !emojis.is_empty() {
                                    view! {
                                        <div class="grid grid-cols-8 gap-0.5 pb-2">
                                            {emojis
                                                .into_iter()
                                                .map(|emoji| {
                                                    let s = emoji.as_str().to_string();
                                                    let for_click = s.clone();
                                                    view! {
                                                        <button
                                                            class="text-xl w-8 h-8 flex items-center justify-center rounded hover:bg-(--ui-hover-bg) cursor-pointer"
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
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="w-full h-full flex flex-col items-center justify-center text-muted">
                                            <Icon
                                                icon=SMILEY_SAD
                                                size="100px"
                                                weight=IconWeight::Thin
                                            />
                                            <span>"No emojis match your search"</span>
                                        </div>
                                    }
                                        .into_any()
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}
