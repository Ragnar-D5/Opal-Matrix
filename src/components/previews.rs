use leptos::{prelude::*, task::spawn_local};
use phosphor_leptos::{Icon, IconWeight, DOWNLOAD_SIMPLE, X};
use shared::timeline::RichTextSpan;
use shared::timeline::UiMediaSource;
use wasm_bindgen::JsCast;

use crate::{
    components::{chat::format_bytes, user_profile::UserProfileMaybeExt},
    state::{AppState, MemberStore},
    tauri_functions::{fetch_preview_data, save_file_to_picked_dest},
};

pub fn render_link(span: RichTextSpan) -> impl IntoView {
    let RichTextSpan::Link { url, .. } = span else {
        return ().into_any();
    };

    let fetch_url = url.clone();
    let preview = LocalResource::new(move || {
        let fetch_url = fetch_url.clone();
        async move {
            fetch_preview_data(fetch_url.clone())
                .await
                .map_err(|e| log::error!("Error fetching preview for URL {}: {:?}", fetch_url, e))
                .ok()
        }
    });

    view! {
        <Suspense fallback=move || {
            view! {
                <div class="animate-pulse bg-(--ui-solid-bg) w-full max-w-sm h-24 rounded-md mt-2"></div>
            }
        }>
            {move || {
                match preview.get() {
                    None => None,
                    Some(None) => Some(().into_any()),
                    Some(Some(data)) => {
                        let link_url = data.url.clone().unwrap_or(url.clone());
                        let app_color = data.color.clone().unwrap_or_else(|| "#ffffff".to_string());
                        let is_small_image = data.image_width.unwrap_or(400) < 250;
                        Some(
                            view! {
                                <div class="flex flex-row max-w-[520px] bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--ui-border-radius) overflow-hidden">
                                    <div
                                        class="w-1 shrink-0"
                                        style=format!("background-color: {}", app_color)
                                    ></div>

                                    <div class=format!(
                                        "p-3 flex gap-3 flex-1 {}",
                                        if is_small_image {
                                            "flex-row justify-between items-center"
                                        } else {
                                            "flex-col"
                                        },
                                    )>

                                        <div class="flex flex-col gap-3 flex-1 min-w-0">

                                            {data
                                                .site_name
                                                .clone()
                                                .map(|site| {
                                                    view! {
                                                        <span class="text-xs font-semibold text-dim truncate">
                                                            {site}
                                                        </span>
                                                    }
                                                })}
                                            <a
                                                href=link_url.clone()
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                class="text-[15px] font-semibold text-[#00a8fc] hover:underline line-clamp-2 leading-tight"
                                            >
                                                {data.title}
                                            </a>
                                            {data
                                                .description
                                                .clone()
                                                .map(|desc| {
                                                    view! {
                                                        <p class="text-[13px] text-[#dbdee1] line-clamp-3 mt-0.5">
                                                            {desc}
                                                        </p>
                                                    }
                                                })}
                                            {(!is_small_image)
                                                .then(|| {
                                                    data.image_url
                                                        .clone()
                                                        .map(|img| {
                                                            let w = data.image_width.unwrap_or(400);
                                                            let h = data.image_height.unwrap_or(300);
                                                            view! {
                                                                <div class="relative rounded-lg overflow-hidden w-full">
                                                                    <a
                                                                        href=link_url.clone()
                                                                        target="_blank"
                                                                        rel="noopener noreferrer"
                                                                    >
                                                                        <img
                                                                            src=img
                                                                            width=data.image_width.unwrap_or(400)
                                                                            height=data.image_height.unwrap_or(300)
                                                                            alt="Preview thumbnail"
                                                                            style=format!(
                                                                                "aspect-ratio: {} / {}; max-height: 330px;",
                                                                                w,
                                                                                h,
                                                                            )
                                                                            class="w-full h-auto object-cover hover:opacity-80"
                                                                        />
                                                                    </a>
                                                                </div>
                                                            }
                                                        })
                                                })}
                                        </div>

                                        {is_small_image
                                            .then(|| {
                                                data.image_url
                                                    .clone()
                                                    .map(|img| {
                                                        view! {
                                                            <div class="shrink-0 relative rounded-md overflow-hidden w-20 h-20 ml-2">
                                                                <a
                                                                    href=link_url.clone()
                                                                    target="_blank"
                                                                    rel="noopener noreferrer"
                                                                >
                                                                    <img
                                                                        src=img
                                                                        alt="Preview thumbnail"
                                                                        class="w-full h-full object-cover hover:opacity-80"
                                                                    />
                                                                </a>
                                                            </div>
                                                        }
                                                    })
                                            })}
                                    </div>
                                </div>
                            }
                                .into_any(),
                        )
                    }
                }
            }}
        </Suspense>
    }.into_any()
}

#[component]
pub fn ImageLightbox() -> impl IntoView {
    let state: AppState = expect_context();
    let lightbox = state.lightbox_image;
    let zoomed = RwSignal::new(false);

    Effect::new(move |_| {
        if lightbox.get().is_none() {
            zoomed.set(false);
            return;
        }

        let signal = lightbox;
        let handler = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |e: web_sys::KeyboardEvent| {
                if e.key() == "Escape" {
                    signal.set(None);
                }
            },
        );
        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref())
            .ok();
        handler.forget();
    });

    view! {
        <Show when=move || lightbox.get().is_some()>
            <div
                class="fixed inset-0 z-[200] bg-black/85 flex flex-col"
                on:click=move |_| lightbox.set(None)
            >
                {move || {
                    lightbox
                        .get()
                        .map(|img| {
                            view! {
                                <LightboxHeader
                                    sender_id=img.sender_id.clone()
                                    timestamp=img.timestamp
                                    filename=img.name.clone()
                                    size=img.size
                                    src=img.source.clone()
                                    on_close=Callback::new(move |_| lightbox.set(None))
                                />
                            }
                        })
                }}

                // Image area
                <div class="flex-1 flex items-center justify-center overflow-hidden">
                    {move || {
                        lightbox
                            .get()
                            .map(|img| {
                                let url = img.source.url();
                                view! {
                                    <img
                                        src=url
                                        class="max-w-[90vw] cursor-zoom-in max-h-[calc(90vh-3rem)] object-contain rounded-md transition-transform duration-200"
                                        style=move || {
                                            if zoomed.get() {
                                                "transform: scale(2); cursor: zoom-out"
                                            } else {
                                                "cursor: zoom-in"
                                            }
                                        }
                                        on:click=move |e| {
                                            e.stop_propagation();
                                            zoomed.update(|z| *z = !*z);
                                        }
                                    />
                                }
                            })
                    }}
                </div>
            </div>
        </Show>
    }
}

#[component]
fn LightboxHeader(
    sender_id: Option<String>,
    timestamp: u64,
    filename: String,
    size: Option<u64>,
    src: UiMediaSource,
    on_close: Callback<()>,
) -> impl IntoView {
    let date = js_sys::Date::new(&js_sys::Number::from(timestamp as f64 * 1000.0));
    let timestamp_str = format!(
        "{}, {:02}:{:02}",
        date.to_date_string().as_string().unwrap_or_default(),
        date.get_hours(),
        date.get_minutes(),
    );

    let store: MemberStore = expect_context();
    let state: AppState = expect_context();
    let room_id = state.active_room_id().unwrap_or_default();
    let profile_sig = store.get_profile(&room_id, &sender_id.unwrap_or_default());
    let name_sig = profile_sig.clone();

    let download_name = filename.clone();
    let on_download = move |e: web_sys::MouseEvent| {
        e.stop_propagation();
        let src = src.clone();
        let name = download_name.clone();

        spawn_local(async move {
            if let Err(e) = save_file_to_picked_dest(src, &name).await {
                log::error!("Error picking file destination: {:?}", e);
            }
        });
    };

    view! {
        <div
            class="grid grid-cols-3 items-center"
            style="background: var(--tile-bg-color); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px); border-bottom: 1px solid var(--tile-border-color)"
            on:click=move |e| e.stop_propagation()
        >
            // Left: avatar + name + timestamp
            <div class="flex items-center gap-2 p-3">
                {move || profile_sig.get().render_icon(35)}
                <div class="flex flex-col min-w-0">
                    {move || name_sig.get().render_name(16)}
                    <span class="text-dim text-xs">{timestamp_str.clone()}</span>
                </div>
            </div>

            // Center: filename + size (truly centered via grid)
            <div class="flex items-center justify-center gap-1 min-w-0">
                <span class="text-normal text-sm truncate">{filename}</span>
                <span class="text-dim text-xs shrink-0">
                    {size.map(|s| format!("({})", format_bytes(s)))}
                </span>
            </div>

            // Right: download + close
            <div class="flex items-center justify-end gap-1 pr-3">
                <button
                    class="text-dim hover:text-bright p-1.5 rounded hover:bg-(--ui-hover-bg) cursor-pointer"
                    title="Download"
                    on:click=on_download
                >
                    <Icon icon=DOWNLOAD_SIMPLE weight=IconWeight::Bold size="20px" />
                </button>
                <button
                    class="text-dim hover:text-bright p-1.5 rounded hover:bg-(--ui-hover-bg) cursor-pointer"
                    title="Close (Esc)"
                    on:click=move |e| {
                        e.stop_propagation();
                        on_close.run(());
                    }
                >
                    <Icon icon=X weight=IconWeight::Bold size="20px" />
                </button>
            </div>
        </div>
    }
}
