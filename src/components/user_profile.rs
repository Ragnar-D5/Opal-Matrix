use colorsys::Hsl;
use leptos::prelude::*;
use shared::user_profile::UserProfile;

use crate::components::get_color;

use super::TextCircle;

pub trait UserProfileExt {
    fn render_icon(self, size: usize) -> impl IntoView;
    fn render_name(self, font_size: usize) -> impl IntoView;

    fn get_color(&self) -> Hsl;

    fn is_room(&self) -> bool;
}

impl UserProfileExt for UserProfile {
    fn is_room(&self) -> bool {
        self.user_id.starts_with("!")
    }

    fn render_icon(self, size: usize) -> impl IntoView {
        let size_str = format!("{}px", size);

        let name = self.display_name.clone().unwrap_or(self.user_id.clone());

        match &self.avatar_url {
            Some(url) => view! {
                <img
                    class="rounded-full object-cover bg-transparent block select-none"
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
                    color=self.get_color()
                    class="rounded-full select-none"
                    style=format!("height: {}; width: {};", size_str, size_str)
                />
            }
            .into_any(),
        }
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        let name = self.display_name.as_ref().unwrap_or(&self.user_id);
        let font_size_str = format!("{}px", font_size);
        let color = self.get_color().to_css_string();

        view! {
            <span
                style:font-size=font_size_str
                style:color=color.clone()
                class="font-semibold cursor-pointer hover:underline"
            >
                {name.clone()}
            </span>
        }
    }

    fn get_color(&self) -> Hsl {
        if self.is_room() {
            return Hsl::new(0.0, 0.0, 70.0, None);
        }

        get_color(self.user_id.clone())
    }
}

pub trait UserProfileMaybeExt {
    fn render_icon(self, size: usize) -> impl IntoView;
    fn render_name(self, font_size: usize) -> impl IntoView;
}

impl UserProfileMaybeExt for Option<UserProfile> {
    fn render_icon(self, size: usize) -> impl IntoView {
        match self {
            Some(profile) => profile.render_icon(size).into_any(),
            None => view! {}.into_any(),
        }
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        match self {
            Some(profile) => profile.render_name(font_size).into_any(),
            None => view! {}.into_any(),
        }
    }
}
