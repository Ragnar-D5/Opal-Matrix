use colorsys::Hsl;
use leptos::prelude::*;
use shared::{
    get_color,
    timeline::RichTextSpan,
    user_profile::{MemberProfile, UserProfile},
};

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

pub trait MemberProfileExt {
    fn render_icon(self, size: usize) -> impl IntoView;
    fn render_name(self, font_size: usize) -> impl IntoView;
    fn to_span(&self) -> RichTextSpan;

    fn is_room(&self) -> bool;

    fn room(room_id: String) -> Self;

    fn get_color(&self) -> Hsl;
}

impl MemberProfileExt for MemberProfile {
    fn to_span(&self) -> RichTextSpan {
        if self.is_room() {
            return RichTextSpan::RoomMention;
        }

        RichTextSpan::UserMention {
            user_id: self.profile.user_id.clone(),
            display_name: self.get_name(),
        }
    }

    fn room(room_id: String) -> Self {
        Self {
            room_id: room_id.clone(),
            profile: UserProfile {
                user_id: room_id,
                display_name: Some("room".into()),
            },
        }
    }

    fn is_room(&self) -> bool {
        self.profile.user_id.starts_with("!")
    }

    fn render_icon(self, size: usize) -> impl IntoView {
        let url = format!("mxc://user/{}/room/{}", self.profile.user_id, self.room_id);
        let name = self.get_name();
        let size_str = format!("{}px", size);

        let first_char = name.chars().next().unwrap_or('?').to_string();
        let color = self.get_color();

        let circle_style = format!("height: {}; width: {};", size_str, size_str);

        let failed = RwSignal::new(true);
        let img_ref = NodeRef::<leptos::html::Img>::new();

        // A cached image can fire `load` before the on:load handler is attached
        // (happens when this avatar is recreated on a re-render), leaving `failed`
        // stuck on the fallback. Re-check `complete` once the element is mounted.
        Effect::new(move |_| {
            if let Some(img) = img_ref.get()
                && img.complete()
                && img.natural_width() > 0
            {
                failed.set(false);
            }
        });

        view! {
            <img
                node_ref=img_ref
                class="rounded-full object-cover bg-transparent block select-none"
                class:hidden=failed
                src=url
                style:height=size_str.clone()
                style:width=size_str
                alt=name
                on:error=move |_| failed.set(true)
                on:load=move |_| failed.set(false)
            />
            <TextCircle
                text=first_char
                color=color
                class="rounded-full select-none"
                class:hidden=move || !failed.get()
                style=circle_style
            />
        }
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        render_profile_name(self.get_name(), self.get_color(), font_size)
    }

    fn get_color(&self) -> Hsl {
        if self.is_room() {
            return Hsl::new(0.0, 0.0, 70.0, None);
        }

        get_color(&self.profile.user_id)
    }
}

impl MemberProfileExt for UserProfile {
    fn to_span(&self) -> RichTextSpan {
        RichTextSpan::UserMention {
            user_id: self.user_id.clone(),
            display_name: self.get_name(),
        }
    }

    fn is_room(&self) -> bool {
        self.user_id.starts_with("!")
    }

    fn room(room_id: String) -> Self {
        Self {
            user_id: room_id.clone(),
            display_name: Some("room".into()),
        }
    }

    fn render_icon(self, size: usize) -> impl IntoView {
        let url = format!("mxc://user/{}/avatar", self.user_id);
        let name = self.get_name();
        let size_str = format!("{}px", size);

        let first_char = name.chars().next().unwrap_or('?').to_string();
        let color = self.get_color();

        let circle_style = format!("height: {}; width: {};", size_str, size_str);

        let failed = RwSignal::new(true);
        let img_ref = NodeRef::<leptos::html::Img>::new();

        // A cached image can fire `load` before the on:load handler is attached
        // (happens when this avatar is recreated on a re-render), leaving `failed`
        // stuck on the fallback. Re-check `complete` once the element is mounted.
        Effect::new(move |_| {
            if let Some(img) = img_ref.get()
                && img.complete()
                && img.natural_width() > 0
            {
                failed.set(false);
            }
        });

        view! {
            <img
                node_ref=img_ref
                class="rounded-full object-cover bg-transparent block select-none"
                class:hidden=failed
                src=url
                style:height=size_str.clone()
                style:width=size_str
                alt=name
                on:error=move |_| failed.set(true)
                on:load=move |_| failed.set(false)
            />
            <TextCircle
                text=first_char
                color=color
                class="rounded-full select-none"
                class:hidden=move || !failed.get()
                style=circle_style
            />
        }
    }

    fn render_name(self, font_size: usize) -> impl IntoView {
        render_profile_name(self.get_name(), self.get_color(), font_size)
    }

    fn get_color(&self) -> Hsl {
        get_color(&self.user_id)
    }
}

pub trait MemberProfileMaybeExt {
    fn render_icon(self, size: usize) -> impl IntoView;
    fn render_name(self, font_size: usize) -> impl IntoView;
    fn get_color(&self) -> Hsl;
}

impl MemberProfileMaybeExt for Option<MemberProfile> {
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

    fn get_color(&self) -> Hsl {
        match self {
            Some(profile) => profile.get_color(),
            None => Hsl::new(0.0, 0.0, 70.0, None),
        }
    }
}

impl MemberProfileMaybeExt for Option<UserProfile> {
    fn get_color(&self) -> Hsl {
        match self {
            Some(profile) => profile.get_color(),
            None => Hsl::new(0.0, 0.0, 70.0, None),
        }
    }

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
