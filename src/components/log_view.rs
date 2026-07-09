use leptos::prelude::*;
use leptos::task::spawn_local;
use nucleo_matcher::{Config, Matcher, Utf32Str};
use shared::api::events::{LogEntry, TauriEvent};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::app::CurrentWindow;
use crate::components::shader::BackgroundShader;
use crate::state::AppState;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    fn invoke(cmd: &str, args: JsValue) -> js_sys::Promise;
}

const SCROLL_CONTAINER_ID: &str = "log-scroll-container";

/// A log level and the Tailwind utility classes that colour it. The classes are
/// kept as literals because Tailwind's scanner only generates CSS for class
/// names it finds verbatim in the source — it cannot see names built at runtime.
/// We look a level up by index. Ordered least-to-most verbose, so the index
/// doubles as the severity value for the filter slider (0 = ERROR, 4 = TRACE).
struct Level {
    name: &'static str,
    text: &'static str,
    bg: &'static str,
    border: &'static str,
    accent: &'static str,
}

static LEVELS: [Level; 5] = [
    Level {
        name: "ERROR",
        text: "text-(--error-color)",
        bg: "bg-(--error-color)",
        border: "border-(--error-color)",
        accent: "accent-(--error-color)",
    },
    Level {
        name: "WARN",
        text: "text-(--idle-color)",
        bg: "bg-(--idle-color)",
        border: "border-(--idle-color)",
        accent: "accent-(--idle-color)",
    },
    Level {
        name: "INFO",
        text: "text-(--success-color)",
        bg: "bg-(--success-color)",
        border: "border-(--success-color)",
        accent: "accent-(--success-color)",
    },
    Level {
        name: "DEBUG",
        text: "text-(--blue)",
        bg: "bg-(--blue)",
        border: "border-(--blue)",
        accent: "accent-(--blue)",
    },
    Level {
        name: "TRACE",
        text: "text-(--purple)",
        bg: "bg-(--purple)",
        border: "border-(--purple)",
        accent: "accent-(--purple)",
    },
];

fn insert_entry(entries: &RwSignal<Vec<LogEntry>>, entry: LogEntry) {
    entries.update(|list| match list.binary_search_by_key(&entry.seq, |e| e.seq) {
        Ok(_) => {}
        Err(pos) => list.insert(pos, entry),
    });
}

fn level_index(level: &str) -> usize {
    LEVELS
        .iter()
        .position(|l| l.name == level)
        .unwrap_or(LEVELS.len() - 1)
}

/// A log line that passed the search filter. Highlighting is computed reactively
/// per row (see `highlighted_field`) rather than stored here, so it updates when
/// the query changes even though `For` keeps the row (keyed by `seq`) alive.
#[derive(Clone, PartialEq)]
struct Match {
    entry: LogEntry,
    location: String,
}

/// Splits `text` into consecutive runs, each flagged with whether it is part of
/// a fuzzy-match hit, so matched characters can be highlighted in place.
fn highlight_segments(text: &str, indices: &[u32]) -> Vec<(String, bool)> {
    let mut hits = indices.to_vec();
    hits.sort_unstable();
    hits.dedup();
    let mut hits = hits.into_iter().peekable();

    let mut segments: Vec<(String, bool)> = Vec::new();
    for (i, ch) in text.chars().enumerate() {
        let hit = hits.peek() == Some(&(i as u32));
        if hit {
            hits.next();
        }
        match segments.last_mut() {
            Some((run, run_hit)) if *run_hit == hit => run.push(ch),
            _ => segments.push((ch.to_string(), hit)),
        }
    }
    segments
}

fn render_highlighted(text: String, indices: Vec<u32>, base_class: &'static str) -> impl IntoView {
    highlight_segments(&text, &indices)
        .into_iter()
        .map(move |(run, hit)| {
            let class = if hit {
                "bg-[rgba(255,180,84,0.30)] text-[#ffe0b0] rounded-[2px]"
            } else {
                base_class
            };
            view! { <span class=class>{run}</span> }
        })
        .collect_view()
}

/// Renders one field (location or message) with the current query's matches
/// highlighted. The closure reads `query` reactively, so when the query is
/// cleared the highlight disappears even though the surrounding row is reused.
fn highlighted_field(
    text: String,
    base_class: &'static str,
    query: RwSignal<String>,
    matcher: StoredValue<Matcher>,
) -> impl IntoView {
    move || {
        let query = query.get();
        let query = query.trim();
        if query.is_empty() {
            return view! { <span class=base_class>{text.clone()}</span> }.into_any();
        }

        let mut indices = Vec::new();
        matcher.update_value(|matcher| {
            let mut needle_buf = Vec::new();
            let mut hay_buf = Vec::new();
            let needle = Utf32Str::new(query, &mut needle_buf);
            let _ = matcher.fuzzy_indices(Utf32Str::new(&text, &mut hay_buf), needle, &mut indices);
        });
        render_highlighted(text.clone(), indices, base_class).into_any()
    }
}

fn is_at_bottom() -> bool {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SCROLL_CONTAINER_ID))
    else {
        return true;
    };
    let distance = el.scroll_height() - el.scroll_top() - el.client_height();
    distance <= 40
}

fn scroll_to_bottom() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SCROLL_CONTAINER_ID))
    {
        el.set_scroll_top(el.scroll_height());
    }
}

#[component]
pub fn LogView() -> impl IntoView {
    let entries = RwSignal::new(Vec::<LogEntry>::new());
    let follow = RwSignal::new(true);
    // Which levels are currently shown. Both the chips and the slider drive this
    // one signal, so the two controls always agree.
    let enabled = RwSignal::new([true; LEVELS.len()]);
    let query = RwSignal::new(String::new());
    let matcher = StoredValue::new(Matcher::new(Config::DEFAULT));
    // Whether long lines wrap; when off, lines stay on one line and the log
    // area scrolls horizontally.
    let wrap = RwSignal::new(true);

    let state = AppState::default();
    state.current_window.set(CurrentWindow::Home);

    provide_context(state);

    spawn_local(async move {
        let closure = Closure::wrap(Box::new(move |event: JsValue| {
            let payload = match js_sys::Reflect::get(&event, &JsValue::from_str("payload")) {
                Ok(p) => p,
                Err(_) => return,
            };
            let json: String = match js_sys::JSON::stringify(&payload) {
                Ok(s) => s.into(),
                Err(_) => return,
            };
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&json) {
                insert_entry(&entries, entry);
            }
        }) as Box<dyn FnMut(JsValue)>);

        let _unlisten = listen(LogEntry::name().as_str(), closure.as_ref().unchecked_ref()).await;
        closure.forget();

        if let Ok(result) = JsFuture::from(invoke("get_log_backlog", JsValue::NULL)).await
            && let Ok(list) = serde_wasm_bindgen::from_value::<Vec<LogEntry>>(result)
        {
            for entry in list {
                insert_entry(&entries, entry);
            }
        }
    });

    // The lines that survive the level filter and the search, recomputed only
    // when the entries, the enabled set or the query change. Chronological order
    // (by seq) is preserved — we never sort by match score.
    let visible = Memo::new(move |_| {
        let enabled = enabled.get();
        let query = query.get();
        let query = query.trim();
        let all = entries.get();

        matcher
            .try_update_value(|matcher| {
                let mut needle_buf = Vec::new();
                let needle = (!query.is_empty()).then(|| Utf32Str::new(query, &mut needle_buf));

                let mut hay_buf = Vec::new();
                let mut out = Vec::new();

                for entry in all {
                    if !enabled[level_index(&entry.level)] {
                        continue;
                    }
                    let location = format!("{}:{}", entry.path, entry.line);

                    let Some(needle) = needle else {
                        out.push(Match { entry, location });
                        continue;
                    };

                    let loc_match = matcher
                        .fuzzy_match(Utf32Str::new(&location, &mut hay_buf), needle)
                        .is_some();
                    let msg_match = matcher
                        .fuzzy_match(Utf32Str::new(&entry.message, &mut hay_buf), needle)
                        .is_some();

                    if loc_match || msg_match {
                        out.push(Match { entry, location });
                    }
                }
                out
            })
            .unwrap_or_default()
    });

    // Slider position: the most-verbose level currently enabled.
    let threshold = move || {
        let enabled = enabled.get();
        (0..LEVELS.len()).rev().find(|&i| enabled[i]).unwrap_or(0)
    };

    Effect::new(move |_| {
        visible.track();
        if follow.get_untracked() {
            request_animation_frame(scroll_to_bottom);
        }
    });

    let chips = (0..LEVELS.len())
        .map(|i| {
            let lvl = &LEVELS[i];
            view! {
                <button
                    on:click=move |_| enabled.update(|set| set[i] = !set[i])
                    class=move || {
                        let on = enabled.get()[i];
                        format!(
                            "px-2 py-0.5 rounded border {} font-mono font-semibold text-[11px] \
                             tracking-wide cursor-pointer {}",
                            lvl.border,
                            if on {
                                format!("{} text-(--ui-solid-bg)", lvl.bg)
                            } else {
                                format!("{} bg-transparent opacity-40", lvl.text)
                            },
                        )
                    }
                >
                    {lvl.name}
                </button>
            }
        })
        .collect_view();

    // Tick marks under the slider: one dot per level, filled up to the current
    // threshold. Clicking a dot jumps the threshold to that level.
    let dots = (0..LEVELS.len())
        .map(|i| {
            let lvl = &LEVELS[i];
            view! {
                <span
                    on:click=move |_| {
                        enabled
                            .update(|set| {
                                for (j, on) in set.iter_mut().enumerate() {
                                    *on = j <= i;
                                }
                            });
                    }
                    class=move || {
                        let active = i <= threshold();
                        format!(
                            "w-1.5 h-1.5 rounded-full cursor-pointer {}",
                            if active {
                                format!("{} {} shadow-[0_0_4px_currentColor]", lvl.bg, lvl.text)
                            } else {
                                format!("bg-transparent border {} opacity-50", lvl.border)
                            },
                        )
                    }
                />
            }
        })
        .collect_view();

    view! {
        <BackgroundShader />
        <div class="backdrop-blur-2xl bg-(--tile-bg-color) flex flex-col h-screen w-screen \
        text-[#c8ccd8] font-mono text-xs">
            <div class="bg-(--ui-solid-bg) flex items-center justify-between gap-4 px-3 py-1.5 \
            border-b border-[#262a38] shrink-0">
                <input
                    type="text"
                    placeholder="Search path or message…"
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                    class="flex-1 max-w-[360px] px-2 py-1 rounded border border-[#262a38] \
                    bg-(--tile-bg-color) text-[#c8ccd8] font-mono text-xs outline-none"
                />
                <div class="flex items-center gap-4">
                    <button
                        on:click=move |_| wrap.update(|w| *w = !*w)
                        class=move || {
                            format!(
                                "px-2 py-0.5 rounded border border-(--accent-color) font-mono \
                                 font-semibold text-[11px] tracking-wide cursor-pointer {}",
                                if wrap.get() {
                                    "bg-(--accent-color) text-(--ui-solid-bg)"
                                } else {
                                    "text-(--accent-color) bg-transparent opacity-40"
                                },
                            )
                        }
                    >
                        "Wrap"
                    </button>
                    <div class="flex gap-1.5">{chips}</div>
                    <div class="flex items-center gap-2">
                        <span class="text-(--error-color) text-[11px] font-semibold">"ERROR"</span>
                        <div class="flex flex-col gap-[3px] w-[130px]">
                            <input
                                type="range"
                                min="0"
                                max=(LEVELS.len() - 1).to_string()
                                step="1"
                                prop:value=move || threshold().to_string()
                                on:input=move |ev| {
                                    let value: usize = event_target_value(&ev)
                                        .parse()
                                        .unwrap_or(LEVELS.len() - 1);
                                    enabled
                                        .update(|set| {
                                            for (i, on) in set.iter_mut().enumerate() {
                                                *on = i <= value;
                                            }
                                        });
                                }
                                class=move || {
                                    format!("w-full cursor-pointer {}", LEVELS[threshold()].accent)
                                }
                            />
                            <div class="flex justify-between px-[5px]">{dots}</div>
                        </div>
                        <span class="text-(--purple) text-[11px] font-semibold">"TRACE"</span>
                    </div>
                </div>
            </div>

            <div
                id=SCROLL_CONTAINER_ID
                on:scroll=move |_| follow.set(is_at_bottom())
                class=move || {
                    format!(
                        "flex-1 px-3 py-2 overflow-y-auto {}",
                        if wrap.get() { "overflow-x-hidden" } else { "overflow-x-auto" },
                    )
                }
            >
                <For each=move || visible.get() key=|m| m.entry.seq let:m>
                    {
                        let level_text = LEVELS[level_index(&m.entry.level)].text;
                        let row_wrap = move || {
                            if wrap.get() {
                                "whitespace-pre-wrap break-words"
                            } else {
                                "whitespace-pre w-max"
                            }
                        };
                        view! {
                            <div class=move || format!("{} py-px leading-normal", row_wrap())>
                                <span class=format!(
                                    "{level_text} font-semibold",
                                )>{m.entry.level}</span>
                                <span class="text-[#3a3f52]">"|"</span>
                                <span class="text-[#4b5163]">{m.entry.timestamp}</span>
                                <span class="text-[#3a3f52]">"|"</span>
                                {highlighted_field(m.location, "text-(--teal)", query, matcher)}
                                <span class="text-[#3a3f52]">"|"</span>
                                {highlighted_field(m.entry.message, "text-[#c8ccd8]", query, matcher)}
                            </div>
                        }
                    }
                </For>
            </div>
        </div>
        <BackgroundShader />
    }
}
