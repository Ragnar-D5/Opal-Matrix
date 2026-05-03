use colorsys::Hsl;
use leptos::prelude::*;

use super::TextCircle;

pub trait UserProfileExt {
    fn render_icon(self, size: usize) -> impl IntoView;
    fn render_name(self, font_size: usize) -> impl IntoView;
    fn get_user_color(&self) -> Hsl;

    fn get_color(string: String) -> Hsl;
}

impl UserProfileExt for shared::user_profile::UserProfile {
    fn render_icon(self, size: usize) -> impl IntoView {
        let size_str = format!("{}px", size);

        let name = self.display_name.clone().unwrap_or(self.user_id.clone());

        match &self.avatar_url {
            Some(url) => view! {
                <img
                    class="rounded-full object-cover bg-transparent block"
                    src=url
                    style:height=size_str.clone()
                    style:width=size_str
                    alt=name
                />
            }
            .into_any(),
            None => view! {
                <TextCircle
                    text=name
                    color_string=self.user_id.clone()
                    class="rounded-full"
                    style=format!("height: {}; width: {};", size_str, size_str)
                />
            }
            .into_any(),
        }
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        let name = self.display_name.as_ref().unwrap_or(&self.user_id);
        let font_size_str = format!("{}px", font_size);
        let color = self.get_user_color().to_css_string();

        view! {
            <span
                style:font-size=font_size_str
                style:color=color
                class="font-semibold"
            >
                {name.clone()}
            </span>
        }
    }

    fn get_user_color(&self) -> Hsl {
        Self::get_color(self.user_id.clone())
    }

    fn get_color(string: String) -> Hsl {
        let mut hash: u32 = 0;
        for c in string.chars() {
            hash = (c as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
        }

        let hue = hash % 360;

        Hsl::new(hue as f64, 90.0, 70.0, None)
    }
}
