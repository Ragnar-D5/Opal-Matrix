use leptos::{portal::Portal, prelude::*};
use shared::synth::{signature_audio_src, SignatureEvent};
use web_sys::{Element, KeyboardEvent};

use crate::{components::presence::PresenceBadge, state::ProfileStore};

#[derive(Clone, Copy)]
pub struct ProfileCardState {
    user_id: RwSignal<Option<String>>,
    room_id: RwSignal<Option<String>>,
    anchor_rect: RwSignal<Option<(f64, f64, f64, f64)>>,
}

impl Default for ProfileCardState {
    fn default() -> Self {
        Self {
            user_id: RwSignal::new(None),
            room_id: RwSignal::new(None),
            anchor_rect: RwSignal::new(None),
        }
    }
}

impl ProfileCardState {
    pub fn open(&self, anchor: &Element, user_id: String, room_id: Option<String>) {
        let rect = anchor.get_bounding_client_rect();
        self.anchor_rect
            .set(Some((rect.left(), rect.top(), rect.right(), rect.bottom())));
        self.user_id.set(Some(user_id));
        self.room_id.set(room_id);
    }

    pub fn close(&self) {
        self.user_id.set(None);
        self.room_id.set(None);
        self.anchor_rect.set(None);
    }
}

#[component]
pub fn ProfileCardPortal() -> impl IntoView {
    let state: ProfileCardState = expect_context();
    let store = StoredValue::new(expect_context::<ProfileStore>());

    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if state.user_id.try_get_untracked().flatten().is_some() && ev.key() == "Escape" {
            state.close();
        }
    });

    let style = move || {
        let Some((left, top, _right, bottom)) = state.anchor_rect.get() else {
            return String::new();
        };

        let win = web_sys::window().unwrap();
        let vw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let vh = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);

        let card_w: f64 = 280.0;
        let card_h: f64 = 220.0;
        let offset = 8.0;

        let actual_w = card_w.min(vw - offset * 2.0);
        let actual_h = card_h.min(vh - offset * 2.0);

        let space_below = vh - bottom;
        let space_above = top;
        let place_above = space_above >= actual_h + offset || space_above >= space_below;

        let y_style = if place_above {
            format!("bottom:{}px;", vh - top + offset)
        } else {
            format!("top:{}px;", bottom + offset)
        };

        let preferred_left = left;
        let x_style = if preferred_left + actual_w <= vw - offset {
            format!("left:{}px;", preferred_left)
        } else {
            format!("left:{}px;", (vw - offset - actual_w).max(offset))
        };

        format!("{x_style}{y_style}width:{actual_w}px;")
    };

    let content = move || {
        let Some(user_id) = state.user_id.get() else {
            return ().into_any();
        };

        let presence = store.with_value(|s| s.get_presence(&user_id));
        let banner_height = 80.0_f64;
        let icon_size = 60.0_f64;
        let icon_radius = icon_size / 2.0;
        let ring_width = 5.0_f64;
        let left_offset = 12.0_f64;
        let cutout_radius = icon_radius + ring_width;
        let smooth_cutout_radius = cutout_radius + 0.5;
        let cx = left_offset + icon_radius;
        let cy = banner_height;
        let banner_mask = format!(
            "-webkit-mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
                mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
                -webkit-mask-composite: destination-out; \
                mask-composite: exclude;",
        );

        let signal = store.with_value(|s| s.get_profile_signal(state.room_id.get(), &user_id));

        let banner_sig = signal.clone();
        let banner_color = move || banner_sig.banner_color();

        let sonic_sig = signal.clone();
        let audio_src = move || signature_audio_src(&sonic_sig.signature(), SignatureEvent::Joined);

        let icon_sig = signal.clone();
        let profile_icon = move || icon_sig.clone().icon(format!("{icon_size}px"));

        let name_sig = signal.clone();
        let profile_name = move || name_sig.clone().name_no_popup("14px".to_string());

        view! {
            <audio src=audio_src autoplay=true />
            <div class="relative flex flex-col w-full">
                <div
                    class="w-full"
                    style=move || {
                        format!(
                            "height: {banner_height}px; background-color: {}; {banner_mask}",
                            banner_color(),
                        )
                    }
                ></div>

                <div
                    class="absolute left-3"
                    style=format!("top: {}px;", banner_height - icon_size / 2.0)
                >
                    <PresenceBadge presence=presence size=20.0>
                        {profile_icon}
                    </PresenceBadge>
                </div>

                <div class="px-3 pt-9 pb-4">
                    {profile_name} <p class="text-xs text-muted">{user_id}</p>
                </div>
            </div>
        }
        .into_any()
    };

    view! {
        <Show when=move || state.user_id.get().is_some()>
            <Portal>
                <div class="fixed inset-0 z-[999]" on:click=move |_| state.close() />

                <div
                    class="fixed z-[1000] bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--floating-border-radius) overflow-hidden"
                    style=style
                >
                    {content}
                </div>
            </Portal>
        </Show>
    }
}
