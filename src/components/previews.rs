use leptos::prelude::*;
use shared::timeline::RichTextSpan;

use crate::tauri_functions::fetch_preview_data;

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
                <div class="animate-pulse bg-(--ui-solid-bg) w-full max-w-sm h-24 rounded-md mt-2">
                </div>
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
