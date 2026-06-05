use colorsys::Hsl;
use leptos::prelude::*;
use shared::{
    get_color,
    profile::{MemberProfile, RoomProfile, UserProfile},
    timeline::{RichTextSpan, RoomIdFormat},
    unknown_color,
};

use super::TextCircle;

pub fn render_url_icon<S: AsRef<str>, T: AsRef<str>, U: AsRef<str>>(
    url: Option<String>,
    name: S,
    size_str: T,
    color: Hsl,
    rounding: U,
) -> impl IntoView {
    let stye_str = format!(
        "height: {}; width: {};",
        size_str.as_ref(),
        size_str.as_ref()
    );

    let is_failed = RwSignal::new(url.is_none());

    let name = name.as_ref().to_string();

    let fallback = view! {
        <TextCircle
            text=name.chars().next().unwrap_or('?').to_string()
            color=color
            class=format!("rounded-{} select-none", rounding.as_ref())
            style=&stye_str
            class:hidden=move || !is_failed.get()
        />
    }
    .into_any();

    if let Some(url) = url {
        view! {
            <img
                class=format!(
                    "rounded-{} object-cover bg-transparent block select-none",
                    rounding.as_ref(),
                )
                src=url
                style=stye_str
                class:hidden=is_failed
                alt=name
                on:error=move |_| is_failed.set(true)
                on:load=move |_| is_failed.set(false)
            />
            {fallback}
        }
        .into_any()
    } else {
        fallback
    }
}

pub fn render_unknown_icon<T: AsRef<str>>(size_str: T) -> impl IntoView {
    render_url_icon(None, "Unknown", size_str, unknown_color(), "full")
}

pub fn render_profile_name<T: AsRef<str>>(
    name: String,
    color: Hsl,
    font_size_str: T,
) -> impl IntoView {
    let font_size_str = format!("{}px", font_size_str.as_ref());

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

pub fn render_unknown_name<T: AsRef<str>>(font_size_str: T) -> impl IntoView {
    render_profile_name("Unknown".to_string(), unknown_color(), font_size_str)
}

pub trait MemberProfileExt {
    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView;
    fn render_icon_room<T: AsRef<str>, U: AsRef<str>>(
        self,
        size_str: T,
        room_id: Option<U>,
    ) -> impl IntoView;
    fn render_name<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView;
    fn to_span(&self) -> RichTextSpan;

    fn is_room(&self) -> bool;

    fn get_color(&self) -> Hsl;
}

impl MemberProfileExt for MemberProfile {
    fn to_span(&self) -> RichTextSpan {
        if self.is_room() {
            return RichTextSpan::RoomMention {
                room_id: RoomIdFormat::Id(self.profile.user_id.clone()),
                display_name: self.get_name(),
            };
        }

        RichTextSpan::UserMention {
            user_id: self.profile.user_id.clone(),
            display_name: self.get_name(),
        }
    }

    fn is_room(&self) -> bool {
        self.profile.user_id.starts_with("!")
    }

    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView {
        self.profile.render_icon_room(size_str, Some(self.room_id))
    }

    fn render_icon_room<T: AsRef<str>, U: AsRef<str>>(
        self,
        size_str: T,
        room_id: Option<U>,
    ) -> impl IntoView {
        self.profile.render_icon_room(size_str, room_id)
    }

    fn render_name<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        render_profile_name(self.get_name(), self.get_color(), font_size_str)
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

    fn render_icon_room<T: AsRef<str>, U: AsRef<str>>(
        self,
        size_str: T,
        room_id: Option<U>,
    ) -> impl IntoView {
        let url = if self.has_avatar {
            if let Some(room_id) = room_id {
                Some(format!(
                    "mxc://user/{}/room/{}",
                    self.user_id,
                    room_id.as_ref()
                ))
            } else {
                Some(format!("mxc://user/{}", self.user_id))
            }
        } else {
            None
        };

        render_url_icon(url, self.get_name(), size_str, self.get_color(), "full")
    }

    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView {
        self.render_icon_room(size_str, None::<T>)
    }

    fn render_name<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        render_profile_name(self.get_name(), self.get_color(), font_size_str)
    }

    fn get_color(&self) -> Hsl {
        get_color(&self.user_id)
    }
}

impl MemberProfileExt for Option<MemberProfile> {
    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_icon(size_str).into_any()
        } else {
            render_unknown_icon(size_str).into_any()
        }
    }

    fn render_icon_room<T: AsRef<str>, U: AsRef<str>>(
        self,
        size_str: T,
        room_id: Option<U>,
    ) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_icon_room(size_str, room_id).into_any()
        } else {
            render_unknown_icon(size_str).into_any()
        }
    }

    fn to_span(&self) -> RichTextSpan {
        match self {
            Some(profile) => profile.to_span(),
            None => RichTextSpan::Plain("".into()),
        }
    }

    fn is_room(&self) -> bool {
        match self {
            Some(profile) => profile.is_room(),
            None => false,
        }
    }

    fn render_name<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_name(font_size_str).into_any()
        } else {
            render_unknown_name(font_size_str).into_any()
        }
    }

    fn get_color(&self) -> Hsl {
        match self {
            Some(profile) => profile.get_color(),
            None => Hsl::new(0.0, 100.0, 70.0, None),
        }
    }
}

impl MemberProfileExt for Option<UserProfile> {
    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_icon(size_str).into_any()
        } else {
            render_unknown_icon(size_str).into_any()
        }
    }

    fn render_icon_room<T: AsRef<str>, U: AsRef<str>>(
        self,
        size_str: T,
        room_id: Option<U>,
    ) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_icon_room(size_str, room_id).into_any()
        } else {
            render_unknown_icon(size_str).into_any()
        }
    }

    fn to_span(&self) -> RichTextSpan {
        match self {
            Some(profile) => profile.to_span(),
            None => RichTextSpan::Plain("".into()),
        }
    }

    fn is_room(&self) -> bool {
        match self {
            Some(profile) => profile.is_room(),
            None => false,
        }
    }

    fn render_name<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_name(font_size_str).into_any()
        } else {
            render_unknown_name(font_size_str).into_any()
        }
    }

    fn get_color(&self) -> Hsl {
        match self {
            Some(profile) => profile.get_color(),
            None => Hsl::new(0.0, 0.0, 70.0, None),
        }
    }
}

pub fn room_as_profile<T: ToString>(room_id: T) -> MemberProfile {
    MemberProfile {
        room_id: room_id.to_string(),
        profile: UserProfile {
            user_id: room_id.to_string(),
            display_name: Some("room".to_string()),
            has_avatar: false,
        },
    }
}

pub trait RoomProfileExt {
    fn to_span(&self) -> RichTextSpan;
}

impl RoomProfileExt for RoomProfile {
    fn to_span(&self) -> RichTextSpan {
        RichTextSpan::RoomMention {
            room_id: RoomIdFormat::Id(self.room_id.clone()),
            display_name: self.get_name(),
        }
    }
}
