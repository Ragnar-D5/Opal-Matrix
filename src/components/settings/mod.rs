use std::collections::HashMap;

use icondata as i;
use leptos::{portal::Portal, prelude::*};
use leptos_icons::Icon as LIcon;
use phosphor_leptos::{Icon, IconWeight, IconWeightData, PENCIL_SIMPLE};
use shared::api::UpdateStatus;
use web_sys::KeyboardEvent;

use crate::{
    components::{
        settings::sections::{
            appearance::render_appearance_section, chats::render_chats_section,
            general::render_general_section, profiles::render_profile_section,
            updates::render_update_section,
        },
        user_profile::MemberProfileExt,
        CloseButton, FloatingTile,
    },
    state::{AppState, ProfileStore},
};

pub mod definition;
pub mod sections;

pub use definition::EnumVariants;
pub use definition::MatrixSettingField;
pub use definition::Settings;

#[derive(Clone, PartialEq)]
enum SettingsIcon {
    IconData(i::Icon),
    Phosphor(&'static IconWeightData),
}

#[derive(Clone)]
enum SectionStatus {
    None,
    Warning,
    Urgent,
}

#[derive(Clone)]
struct SettingsSection {
    title: &'static str,
    id: &'static str,
    icon: SettingsIcon,
    render_fn: fn() -> AnyView,
}

impl PartialEq for SettingsSection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Clone)]
enum SettingsItem {
    Section(SettingsSection),
    Divider { id: &'static str },
}

impl SettingsItem {
    fn id(&self) -> &'static str {
        match self {
            SettingsItem::Section(section) => section.id,
            SettingsItem::Divider { id } => id,
        }
    }
}

const SETTINGS_SECTIONS: &[SettingsItem] = &[
    SettingsItem::Section(SettingsSection {
        title: "General",
        id: "general",
        icon: SettingsIcon::Phosphor(phosphor_leptos::SLIDERS),
        render_fn: render_general_section,
    }),
    SettingsItem::Section(SettingsSection {
        title: "Appearance",
        id: "appearance",
        icon: SettingsIcon::IconData(i::BsPalette),
        render_fn: render_appearance_section,
    }),
    SettingsItem::Section(SettingsSection {
        title: "Audio",
        id: "audio",
        icon: SettingsIcon::Phosphor(phosphor_leptos::HEADPHONES),
        render_fn: || ().into_any(),
    }),
    SettingsItem::Section(SettingsSection {
        title: "Chats",
        id: "chats",
        icon: SettingsIcon::Phosphor(phosphor_leptos::CHATS),
        render_fn: render_chats_section,
    }),
    SettingsItem::Divider { id: "divider-1" },
    SettingsItem::Section(SettingsSection {
        title: "Updates",
        id: "updates",
        icon: SettingsIcon::Phosphor(phosphor_leptos::ARROWS_CLOCKWISE),
        render_fn: render_update_section,
    }),
];

const PROFILE_SECTION: SettingsSection = SettingsSection {
    title: "Profile",
    id: "profile",
    icon: SettingsIcon::Phosphor(PENCIL_SIMPLE),
    render_fn: render_profile_section,
};

#[component]
pub fn SettingsIcon(#[prop(into, optional)] class: String) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let sections_dict: HashMap<&str, SettingsSection> = SETTINGS_SECTIONS
        .iter()
        .filter_map(|item| {
            if let SettingsItem::Section(section) = item {
                Some((section.id, section.clone()))
            } else {
                None
            }
        })
        .collect();

    let (is_open, set_is_open) = signal(false);

    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if is_open.try_get_untracked().unwrap_or(false) && ev.key() == "Escape" {
            set_is_open.set(false);
        }
    });

    let statuses: StoredValue<HashMap<&str, RwSignal<SectionStatus>>> = StoredValue::new(
        SETTINGS_SECTIONS
            .iter()
            .filter_map(|item| {
                if let SettingsItem::Section(section) = item {
                    Some((section.id, RwSignal::new(SectionStatus::None)))
                } else {
                    None
                }
            })
            .collect(),
    );

    let update_sig = statuses.get_value()["updates"];

    let settings: Settings = expect_context();

    Effect::new(move |_| {
        if settings.notify_update.signal().get() {
            match state.update_status.get() {
                UpdateStatus::UpdateAvailable(_)
                | UpdateStatus::Downloading(_)
                | UpdateStatus::ReadyToInstall(_) => update_sig.set(SectionStatus::Warning),
                UpdateStatus::Error { .. } => update_sig.set(SectionStatus::Urgent),
                UpdateStatus::UpToDate | UpdateStatus::CheckingForUpdates => {
                    update_sig.set(SectionStatus::None)
                }
            }
        }
    });

    let selected_section = RwSignal::new(PROFILE_SECTION.id);

    let user_sig = Memo::new(move |_| {
        let user_id = state.user_id.get();
        store.get_user_profile(&user_id)
    });

    let current_section = Memo::new(move |_| {
        sections_dict
            .get(&selected_section.get())
            .unwrap_or(&PROFILE_SECTION)
            .clone()
    });

    let section_bar_view = move |section: SettingsItem| match section {
        SettingsItem::Divider { .. } => {
            view! { <div class="border-t border-(--tile-border-color) my-1" /> }.into_any()
        }
        SettingsItem::Section(section) => {
            let status_sig = *statuses.get_value().get(section.id).unwrap();

            let status_color = move || match status_sig.get() {
                SectionStatus::None => "transparent",
                SectionStatus::Warning => "var(--warning-color)",
                SectionStatus::Urgent => "var(--error-color)",
            };

            view! {
                <button
                    class="flex flex-0 items-center gap-3 text-left text-dim hover:text-normal mx-2 rounded-[10px] cursor-pointer px-2 py-1 border border-transparent hover:border-(--tile-border-color)"
                    class=("bg-(--ui-solid-hover-bg)", move || section.id == selected_section.get())
                    class=("text-normal", move || section.id == selected_section.get())
                    on:click=move |_| selected_section.set(section.id)
                >
                    {match section.icon {
                        SettingsIcon::IconData(icon_data) => {
                            view! { <LIcon icon=icon_data height="18px" /> }.into_any()
                        }
                        SettingsIcon::Phosphor(phosphor_icon) => {
                            view! {
                                <Icon icon=phosphor_icon size="18px" weight=IconWeight::Fill />
                            }
                                .into_any()
                        }
                    }}
                    <span>{section.title}</span>
                    <div class="w-2 h-2 rounded-full" style:background=status_color />
                </button>
            }
                    .into_any()
        }
    };

    view! {
        <button
            on:click=move |_| set_is_open.update(|v| *v = !*v)
            class=format!(
                "text-muted hover:text-bright cursor-pointer transition-transform duration-300 ease-in-out hover:rotate-[90deg] {class}",
            )
        >
            <LIcon icon=i::BsGearWideConnected height="20px" />
        </button>

        <Show when=move || is_open.get() fallback=|| ()>
            <Portal>
                <div
                    on:click=move |_| set_is_open.set(false)
                    on:wheel=|e| e.stop_propagation()
                    class="fixed inset-0 z-40 bg-(--overlay-bg-color) backdrop-blur-sm flex items-center justify-center p-6 md:p-12"
                >
                    <FloatingTile
                        on:click=move |e| e.stop_propagation()
                        class="opacity-100 text-bright max-w-300 w-[80vw] h-full max-h-[95vh] min-h-[50vh] flex flex-row overflow-hidden z-50 bg-(--ui-solid-bg)"
                    >
                        <div class="border-r border-(--tile-border-color) w-50 h-full flex flex-col gap-1">
                            <div
                                class="flex items-center p-2 border-b border-(--tile-border-color) gap-2 cursor-pointer group"
                                on:click=move |_| selected_section.set(PROFILE_SECTION.id)
                            >
                                <div
                                    class="border border-transparent group-hover:border-(--tile-border-color) flex flex-row rounded-[10px] p-1 items-center justify-center flex-1"
                                    class=(
                                        "bg-(--ui-solid-hover-bg)",
                                        move || selected_section.get() == PROFILE_SECTION.id,
                                    )
                                >
                                    {move || user_sig.get().get().render_icon("40px")}
                                    <div class="flex flex-col p-2 rounded-[10px]">
                                        {move || user_sig.get().get().render_name_no_popup("16px")}
                                        <span
                                            class="text-xs group-hover:underline flex flex-row pr-2"
                                            class=(
                                                "text-normal",
                                                move || selected_section.get() == PROFILE_SECTION.id,
                                            )
                                            class=(
                                                "text-muted",
                                                move || selected_section.get() != PROFILE_SECTION.id,
                                            )
                                        >
                                            <Icon
                                                icon=PENCIL_SIMPLE
                                                size="12px"
                                                weight=IconWeight::Fill
                                            />
                                            "Edit profiles"
                                        </span>
                                    </div>
                                </div>
                            </div>
                            <For
                                each=move || SETTINGS_SECTIONS.iter().cloned()
                                key=|s| s.id()
                                children=move |section| section_bar_view(section)
                            />
                        </div>
                        <div class="w-full h-full flex flex-col">
                            <div class="w-full">
                                <span class="text-xl font-bold text-normal p-4 block border-b border-(--tile-border-color)">
                                    {move || current_section.get().title}
                                </span>
                                <CloseButton
                                    on_click=move |_| set_is_open.set(false)
                                    size="20px"
                                    inset="12px"
                                />
                            </div>
                            <div class="p-4 w-full h-full overflow-y-auto">
                                {move || current_section.get().render_fn}
                            </div>
                        </div>
                    </FloatingTile>
                </div>
            </Portal>
        </Show>
    }
}
