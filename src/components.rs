use leptos::prelude::*;

#[component]
pub fn FloatingTile(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    view! {
        <div
            class=format!("servers flex flex-col items-center bg-[var(--floating-bg-color)] border-[1px] border-[var(--tile-border-color)] rounded-[16px] overflow-y-auto shadow-sm flex-shrink-0 backdrop-blur-2xl {class}")
            style="scrollbar-width: none;"
        >
            {children()}
        </div>
    }
}
