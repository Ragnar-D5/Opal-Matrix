use colorsys::Hsl;
use leptos::prelude::*;
use shared::user_profile::UserProfile;

use crate::components::get_color;

use super::TextCircle;

pub fn render_profile_icon(
    url: Option<String>,
    name: String,
    size: usize,
    color: Hsl,
) -> impl IntoView {
    let size_str = format!("{}px", size);

    match url {
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
                text=name.chars().next().unwrap_or('?').to_string()
                color=color
                class="rounded-full select-none"
                style=format!("height: {}; width: {};", size_str, size_str)
            />
        }
        .into_any(),
    }
}

pub fn render_profile_name(name: String, color: Hsl, font_size: usize) -> impl IntoView {
    let font_size_str = format!("{}px", font_size);

    view! {
        <span
            style:font-size=font_size_str
            style:color=color.to_css_string()
            class="font-semibold cursor-pointer hover:underline"
        >
            {name.clone()}
        </span>
    }
}

pub trait UserProfileExt {
    fn render_icon(self, size: usize) -> impl IntoView;
    fn render_name(self, font_size: usize) -> impl IntoView;
    fn get_name(&self) -> String;

    fn get_color(&self) -> Hsl;

    fn is_room(&self) -> bool;
}

impl UserProfileExt for UserProfile {
    fn is_room(&self) -> bool {
        self.user_id.starts_with("!")
    }

    fn get_name(&self) -> String {
        self.display_name.clone().unwrap_or(self.user_id.clone())
    }

    fn render_icon(self, size: usize) -> impl IntoView {
        render_profile_icon(
            self.avatar_url.clone(),
            self.get_name(),
            size,
            self.get_color(),
        )
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        render_profile_name(self.get_name(), self.get_color(), font_size)
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
            None => ().into_any(),
        }
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        match self {
            Some(profile) => profile.render_name(font_size).into_any(),
            None => ().into_any(),
        }
    }
}
