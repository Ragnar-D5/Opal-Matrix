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
                    <path
                        d="M10 20c-5.523 0-10-4.477-10-10S4.477 0 10 0a9.985 9.985 0 0 1 7.64 3.546 1 1 0 1 1-1.542 1.272A7.983 7.983 0 0 0 10 2c-4.418 0-8 3.582-8 8s3.582 8 8 8c1.614 0 3.123-.48 4.388-1.303a1 1 0 1 1 1.096 1.67A9.972 9.972 0 0 1 10 20z"
                        fill="#f0b232"
                    />
                }.into_any(),
                PresenceStatus::Busy => view! {
                    <circle cx="10" cy="10" r="10" fill="#f23f43" />
                    <rect x="4" y="8" width="12" height="4" rx="2" fill="white" />
                }.into_any(),
                PresenceStatus::Offline => view! {
                    <circle cx="10" cy="10" r="7" stroke="#80848e" stroke-width="6" fill="none" />
                }.into_any(),
            }}
        </svg>
    };

    let mask_style = "-webkit-mask: radial-gradient(circle 11px at calc(100% - 8px) calc(100% - 8px), transparent 11px, black 11.5px); mask: radial-gradient(circle 11px at calc(100% - 8px) calc(100% - 8px), transparent 11px, black 11.5px);";

    view! {
        <div class="relative w-fit h-fit">
            <div class=format!("w-full h-full {class}") style=mask_style>
                {children()}
            </div>

            <div class="absolute -bottom-0 -right-0 flex items-center justify-center text-white text-[12px] font-extrabold w-4 h-4 rounded-full">
                {svg}
            </div>
        </div>
    }
}
