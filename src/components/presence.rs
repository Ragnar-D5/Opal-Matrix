use leptos::prelude::*;
use shared::profile::{PresenceInfo, PresenceStatus};

#[component]
pub fn PresenceBadge(
    presence: ArcRwSignal<PresenceInfo>,
    children: Children,
    #[prop(optional)] size: Option<f32>,
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] indicator_class: String,
) -> impl IntoView {
    let size_px = size.unwrap_or(16.0);
    let svg_px = size_px * 10.0 / 16.0;

    let svg = view! {
        <svg
            width=svg_px
            height=svg_px
            viewBox="0 0 20 20"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            class=indicator_class
        >
            {move || match presence.get().status {
                PresenceStatus::Online => {
                    view! { <circle cx="10" cy="10" r="10" fill="var(--online-color)" /> }
                        .into_any()
                }
                PresenceStatus::Unavailable => {
                    view! {
                        <defs>
                            <mask id="idle-mask">
                                <circle cx="10" cy="10" r="10" fill="white" />
                                <circle cx="6" cy="6" r="8" fill="black" />
                            </mask>
                        </defs>
                        <circle
                            cx="10"
                            cy="10"
                            r="10"
                            fill="var(--idle-color)"
                            mask="url(#idle-mask)"
                        />
                    }
                        .into_any()
                }
                PresenceStatus::Busy => {
                    view! {
                        <circle cx="10" cy="10" r="10" fill="var(--busy-color)" />
                        <rect x="4" y="8" width="12" height="4" rx="2" fill="white" />
                    }
                        .into_any()
                }
                PresenceStatus::Offline => {
                    view! {
                        <circle
                            cx="10"
                            cy="10"
                            r="7.5"
                            stroke="var(--offline-color)"
                            stroke-width="5"
                            fill="none"
                        />
                    }
                        .into_any()
                }
            }}
        </svg>
    };

    let radius = size_px / 2.0;
    let smooth_radius = radius + 0.5;
    let cutout_offset = size_px * 5.0 / 16.0;
    let badge_shift = size_px * 1.0 / 16.0;

    let mask_style = format!(
        "-webkit-mask: radial-gradient(circle at calc(100% - {cutout_offset}px) calc(100% - {cutout_offset}px), transparent {radius}px, black {smooth_radius}px); \
            mask: radial-gradient(circle at calc(100% - {cutout_offset}px) calc(100% - {cutout_offset}px), transparent {radius}px, black {smooth_radius}px);"
    );

    view! {
        <div class="relative inline-flex shrink-0">
            <div class=format!("w-full h-full {class}") style=mask_style>
                {children()}
            </div>

            <div
                class="absolute flex items-center justify-center text-white text-[12px] font-extrabold rounded-full z-10"
                style=format!(
                    "width: {size_px}px; height: {size_px}px; bottom: -{}px; right: -{}px;",
                    3.0 * badge_shift,
                    3.0 * badge_shift,
                )
            >
                {svg}
            </div>
        </div>
    }
}
