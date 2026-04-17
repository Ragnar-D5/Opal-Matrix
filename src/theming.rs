use std::collections::HashMap;

use leptos::{leptos_dom::logging::console_error, prelude::*};
use wasm_bindgen::{JsCast, JsValue};

#[component]
pub fn ThemeProvider(children: Children) -> impl IntoView {
    let (accent_color, set_accent_color) = signal("#5865F2".to_string());
    let (bg_color, set_bg_color) = signal("#1e1e2e".to_string());

    provide_context(set_accent_color);
    provide_context(set_bg_color);

    Effect::new(move |_| {
        let current_color = accent_color.get();

        if let Some(doc_element) = document().document_element() {
            if let Ok(html_el) = doc_element.dyn_into::<web_sys::HtmlElement>() {
                if let Err(e) = set_properties(
                    &html_el,
                    HashMap::from([
                        ("--accent-color", &current_color),
                        ("--bg-color", &bg_color.get()),
                        ("--border-color", &"#f00".to_string()),
                    ]),
                ) {
                    console_error(&format!("Failed to set CSS properties: {:?}", e));
                }
            }
        }
    });

    children()
}

fn set_properties(
    element: &web_sys::HtmlElement,
    properties: HashMap<&str, &String>,
) -> Result<(), JsValue> {
    let style = element.style();

    for (key, value) in properties {
        style.set_property(key, value)?;
    }

    Ok(())
}
