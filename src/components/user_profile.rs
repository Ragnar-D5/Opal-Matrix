use csscolorparser::Color;
use leptos::prelude::*;
use shared::{
    profile::{MemberProfile, RoomProfile, UserProfile},
    sidebar::RoomNodeInfo,
    timeline::{RichTextSpan, RoomIdFormat},
    unknown_color,
};
use wasm_bindgen::JsCast;
use web_sys::MouseEvent;

use crate::components::overlays::profile_card::ProfileCardState;

use super::TextCircle;

pub fn render_url_icon<S: AsRef<str>, T: AsRef<str>, U: AsRef<str>>(
    url: Option<String>,
    name: S,
    size_str: T,
    color: Color,
    rounding: U,
) -> AnyView {
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
                src=url.clone()
                style=stye_str
                class:hidden=is_failed
                alt=name
                on:error=move |_| {
                    log::warn!("Failed to load image for {}, showing fallback", url);
                    is_failed.set(true)
                }
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
    color: Color,
    user_id: Option<String>,
    room_id: Option<String>,
    font_size_str: T,
    popup: bool,
) -> impl IntoView {
    let font_size_str = font_size_str.as_ref().to_string();
    let card_state: ProfileCardState = expect_context();

    let on_click = move |ev: MouseEvent| {
        if !popup {
            return;
        }

        let Some(user_id) = user_id.clone() else {
            return;
        };

        if let Some(el) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            card_state.open(&el, user_id, room_id.clone());
        }
    };

    view! {
        <span
            style:font-size=font_size_str
            style:color=color.to_css_hsl()
            class="font-bold cursor-pointer hover:underline group-hover:underline"
            on:click=on_click
        >
            {name.clone()}
        </span>
    }
}

pub fn render_unknown_name<T: AsRef<str>>(font_size_str: T) -> impl IntoView {
    render_profile_name(
        "Unknown".to_string(),
        unknown_color(),
        None,
        None,
        font_size_str,
        false,
    )
}

pub trait RoomNodeExt {
    fn render_icon<T: AsRef<str>>(&self, size_str: T) -> AnyView;
}

impl RoomNodeExt for RoomNodeInfo {
    fn render_icon<T: AsRef<str>>(&self, size_str: T) -> AnyView {
        if let Some(url) = self.avatar_url() {
            render_url_icon(Some(url), &self.name, size_str, self.color.clone(), "full")
        } else {
            render_url_icon(None, &self.name, size_str, self.color.clone(), "full")
        }
    }
}

pub trait MemberProfileExt {
    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView;
    fn render_icon_room<T: AsRef<str>, U: AsRef<str>>(
        self,
        size_str: T,
        room_id: Option<U>,
    ) -> impl IntoView;
    fn render_name<T: AsRef<str>>(self, font_size_str: T, popup: bool) -> impl IntoView;
    fn render_name_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView;
    fn render_name_no_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView;
    fn to_span(&self) -> RichTextSpan;

    fn is_room(&self) -> bool;
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

    fn render_name<T: AsRef<str>>(self, font_size_str: T, popup: bool) -> impl IntoView {
        render_profile_name(
            self.get_name(),
            self.name_color(),
            Some(self.profile.user_id.clone()),
            Some(self.room_id.clone()),
            font_size_str,
            popup,
        )
    }

    fn render_name_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, true)
    }

    fn render_name_no_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, false)
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

        render_url_icon(url, self.get_name(), size_str, self.name_color(), "full")
    }

    fn render_icon<T: AsRef<str>>(self, size_str: T) -> impl IntoView {
        self.render_icon_room(size_str, None::<T>)
    }

    fn render_name<T: AsRef<str>>(self, font_size_str: T, popup: bool) -> impl IntoView {
        render_profile_name(
            self.get_name(),
            self.name_color(),
            Some(self.user_id),
            None,
            font_size_str,
            popup,
        )
    }

    fn render_name_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, true)
    }

    fn render_name_no_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, false)
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

    fn render_name<T: AsRef<str>>(self, font_size_str: T, popup: bool) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_name(font_size_str, popup).into_any()
        } else {
            render_unknown_name(font_size_str).into_any()
        }
    }

    fn render_name_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, true)
    }

    fn render_name_no_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, false)
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

    fn render_name<T: AsRef<str>>(self, font_size_str: T, popup: bool) -> impl IntoView {
        if let Some(profile) = self {
            profile.render_name(font_size_str, popup).into_any()
        } else {
            render_unknown_name(font_size_str).into_any()
        }
    }

    fn render_name_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, true)
    }

    fn render_name_no_popup<T: AsRef<str>>(self, font_size_str: T) -> impl IntoView {
        self.render_name(font_size_str, false)
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

pub fn render_user_profile_card(
    width: f64,
    height: f64,
    user_id: String,
    room_id: Option<String>,
) -> impl IntoView {
    use crate::{components::presence::PresenceBadge, state::ProfileStore};

    let store: ProfileStore = expect_context();
    let profile_sig = store.get_profile_signal(room_id, &user_id);
    let presence = store.get_presence(&user_id);

    let icon_sig = profile_sig.clone();
    let name_sig = profile_sig.clone();

    let icon_size = width;
    let icon_radius = icon_size / 2.0;
    let ring_width = 6.0;
    let left_offset = 16.0;
    let cutout_radius = icon_radius + ring_width;
    let smooth_cutout_radius = cutout_radius + 0.5;
    let cx = left_offset + icon_radius;
    let cy = height;
    let banner_mask = format!(
        "-webkit-mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
         mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
         -webkit-mask-composite: destination-out; \
         mask-composite: exclude;",
    );
    let banner_style = move || {
        format!(
            "height: {height}px; background-color: {}; {banner_mask}",
            profile_sig.banner_color()
        )
    };
    let icon_top = height - icon_radius;
    let icon_size_str = format!("{icon_size}px");
    let badge_size = (icon_size * 25.0 / 70.0) as f32;

    view! {
        <div class="relative flex flex-col w-full">
            <div class="w-full" style=banner_style></div>

            <div class="absolute left-4" style=format!("top: {icon_top}px;")>
                <PresenceBadge presence=presence.clone() size=badge_size>
                    {move || icon_sig.clone().icon(icon_size_str.clone())}
                </PresenceBadge>
            </div>

            <div class="px-4 pb-6" style=format!("padding-top: {icon_radius}px;")>
                {move || name_sig.clone().name("16px".to_string())}
            </div>
        </div>
    }
}
