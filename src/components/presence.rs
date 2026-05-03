use leptos::prelude::*;
use shared::user_profile::{PresenceInfo, PresenceStatus};

#[component]
pub fn PresenceBadge(
    presence: ArcRwSignal<PresenceInfo>,
    children: Children,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    let svg = view! {
        <svg
            width=10
            height=10
            viewBox="0 0 20 20"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
        >
            {move || match presence.get().status {
                PresenceStatus::Online => view! {
                    <circle cx="10" cy="10" r="10" fill="#23a55a" />
                }.into_any(),
                PresenceStatus::Unavailable => view! {
                    <defs>
                        <mask id="idle-mask">
                            <circle cx="10" cy="10" r="10" fill="white" />
                            <circle cx="6" cy="6" r="8" fill="black" />
                        </mask>
                    </defs>
                    <circle cx="10" cy="10" r="10" fill="#f0b232" mask="url(#idle-mask)" />
                }.into_any(),
                PresenceStatus::Busy => view! {
                    <circle cx="10" cy="10" r="10" fill="#f23f43" />
                    <rect x="4" y="8" width="12" height="4" rx="2" fill="white" />
                }.into_any(),
                PresenceStatus::Offline => view! {
                    <circle cx="10" cy="10" r="7.5" stroke="#80848e" stroke-width="5" fill="none" />
                }.into_any(),
            }}
        </svg>
    };

    let radius = 8.0;
    let smooth_radius = radius + 0.5;

    let mask_style = format!(
        "-webkit-mask: radial-gradient(circle at calc(100% - 5px) calc(100% - 5px), transparent {}px, black {}px); \
            mask: radial-gradient(circle at calc(100% - 5px) calc(100% - 5px), transparent {}px, black {}px);",
        radius, smooth_radius, radius, smooth_radius
    );

    view! {
        <div class="relative inline-flex shrink-0">
            <div class=format!("w-full h-full {class}") style=mask_style>
                {children()}
            </div>

            <div class="absolute -bottom-[3px] -right-[3px] flex items-center justify-center text-white text-[12px] font-extrabold w-4 h-4 rounded-full">
                {svg}
            </div>
        </div>
    }
}
