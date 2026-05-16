use colorsys::Hsl;
use leptos::prelude::*;

pub(crate) mod authentication;
pub(crate) mod chat;
pub(crate) mod input;
pub(crate) mod loading;
pub(crate) mod presence;
pub(crate) mod previews;
pub(crate) mod shader;
pub(crate) mod sidebar;
pub(crate) mod text;
pub(crate) mod user_profile;

pub fn get_color(string: String) -> Hsl {
    let mut hash: u32 = 0;
    for c in string.chars() {
        hash = (c as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
    }

    let hue = hash % 360;

    Hsl::new(hue as f64, 90.0, 70.0, None)
}

#[component]
pub fn FloatingTile(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    view! {
        <div
            class=format!(
                "servers flex flex-col items-center bg-[var(--tile-bg-color)] border-[1px] border-[var(--tile-border-color)] rounded-(--floating-border-radius) overflow-y-auto shadow-sm flex-shrink-0 backdrop-blur-2xl {class}",
            )
            style="scrollbar-width: none;"
        >
            {children()}
        </div>
    }
}

#[component]
pub fn SingleFloatingTile(
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] style: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="w-screen h-screen flex justify-center items-center" style=style>
            <FloatingTile class=class>{children()}</FloatingTile>
        </div>
    }
}

/// A circular avatar with a single letter, colored based on the provided color.
#[component]
pub fn TextCircle(
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] style: String,
    text: String,
    mut color: Hsl,
) -> impl IntoView {
    let letter_color = color.clone().to_css_string();

    color.set_lightness(10.0);
    let bg_color = color.to_css_string();

    view! {
        <div
            class=format!(
                "relative flex items-center justify-center aspect-square {class} font-bold overflow-hidden",
            )
            style=format!(
                "background-color: {bg_color}; container-type: size; color: {letter_color}; {style}",
            )
        >
            <div
                class="absolute inset-0 pointer-events-none"
                style=format!(
                    "box-shadow: inset 0 0 10cqmin 5cqmin {letter_color}; border-radius: inherit;",
                )
            ></div>

            <span class="relative" style="font-size: 50cqmin; line-height: 1;">
                {text.chars().next().unwrap_or('?').to_ascii_uppercase()}
            </span>
        </div>
    }
}
