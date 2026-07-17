use std::collections::HashMap;

use icondata as i;
use leptos::{portal::Portal, prelude::*};
use leptos_icons::Icon as LIcon;
use phosphor_leptos::{
    CARET_DOUBLE_RIGHT, Icon, IconWeight, IconWeightData, MAGNIFYING_GLASS, PENCIL_SIMPLE, QUESTION,
};
use shared::{api::UpdateStatus, settings::SettingsSection};
use web_sys::{KeyboardEvent, ScrollBehavior, ScrollIntoViewOptions, ScrollLogicalPosition};

use crate::{
    components::{
        CloseButton, FloatingTile,
        settings::sections::{
            appearance::render_appearance_section, chats::render_chats_section,
            general::render_general_section, profiles::render_profile_section,
            updates::render_update_section,
        },
        user_profile::MemberProfileExt,
    },
    state::{AppState, MediaCache, ProfileStore},
};

pub mod definition;
pub mod sections;

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
struct UiSettingsSection {
    title: &'static str,
    id: SettingsSection,
    icon: SettingsIcon,
    render_fn: fn() -> AnyView,
}

impl PartialEq for UiSettingsSection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Clone)]
enum SettingsItem {
    Section(UiSettingsSection),
    Divider,
}

impl SettingsItem {
    fn id(&self) -> SettingsSection {
        match self {
            SettingsItem::Section(section) => section.id,
            SettingsItem::Divider => SettingsSection::Divider,
        }
    }
}

const SETTINGS_SECTIONS: &[SettingsItem] = &[
    SettingsItem::Divider,
    SettingsItem::Section(UiSettingsSection {
        title: "General",
        id: SettingsSection::General,
        icon: SettingsIcon::Phosphor(phosphor_leptos::SLIDERS),
        render_fn: render_general_section,
    }),
    SettingsItem::Section(UiSettingsSection {
        title: "Appearance",
        id: SettingsSection::Appearance,
        icon: SettingsIcon::IconData(i::BsPalette),
        render_fn: render_appearance_section,
    }),
    SettingsItem::Section(UiSettingsSection {
        title: "Audio",
        id: SettingsSection::Audio,
        icon: SettingsIcon::Phosphor(phosphor_leptos::HEADPHONES),
        render_fn: || ().into_any(),
    }),
    SettingsItem::Section(UiSettingsSection {
        title: "Chats",
        id: SettingsSection::Chats,
        icon: SettingsIcon::Phosphor(phosphor_leptos::CHATS),
        render_fn: render_chats_section,
    }),
    SettingsItem::Divider,
    SettingsItem::Section(UiSettingsSection {
        title: "Updates",
        id: SettingsSection::Updates,
        icon: SettingsIcon::Phosphor(phosphor_leptos::ARROWS_CLOCKWISE),
        render_fn: render_update_section,
    }),
];

const PROFILE_SECTION: UiSettingsSection = UiSettingsSection {
    title: "Profile",
    id: SettingsSection::Profile,
    icon: SettingsIcon::Phosphor(PENCIL_SIMPLE),
    render_fn: render_profile_section,
};

fn scroll_to_setting(type_name: &'static str) {
    let selector = format!("[data-field=\"{type_name}\"]");
    let Some(el) = document().query_selector(&selector).ok().flatten() else {
        return;
    };

    let options = ScrollIntoViewOptions::new();
    options.set_behavior(ScrollBehavior::Smooth);
    options.set_block(ScrollLogicalPosition::Center);
    el.scroll_into_view_with_scroll_into_view_options(&options);

    el.class_list().add_1("animate-highlight").ok();
    set_timeout(
        move || {
            if let Some(el) = document().query_selector(&selector).ok().flatten() {
                el.class_list().remove_1("animate-highlight").ok();
            }
        },
        std::time::Duration::from_secs(4),
    );
}

#[component]
pub fn SettingsIcon(#[prop(into, optional)] class: String) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    let sections_dict: HashMap<SettingsSection, UiSettingsSection> = SETTINGS_SECTIONS
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

    let statuses: StoredValue<HashMap<SettingsSection, RwSignal<SectionStatus>>> = StoredValue::new(
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

    let update_sig = statuses.get_value()[&SettingsSection::Updates];

    let settings: Settings = expect_context();

    Effect::new(move |_| {
        let status = state.update_status.get();
        if matches!(status, UpdateStatus::Error { .. }) {
            update_sig.set(SectionStatus::Urgent);
            return;
        }

        if settings.notify_update.signal().get() {
            match state.update_status.get() {
                UpdateStatus::UpdateAvailable(_)
                | UpdateStatus::Downloading(_)
                | UpdateStatus::ReadyToInstall(_) => update_sig.set(SectionStatus::Warning),
                UpdateStatus::UpToDate | UpdateStatus::CheckingForUpdates => {
                    update_sig.set(SectionStatus::None)
                }
                _ => (),
            }
        } else {
            update_sig.set(SectionStatus::None)
        }
    });

    let selected_section = RwSignal::new(PROFILE_SECTION.id);
    let search_query = RwSignal::new(String::new());

    let user_sig = Memo::new(move |_| {
        store.get_user_profile(&state.user_id.get().expect("user_id is not set"))
    });

    let sections_dict_for_search: StoredValue<HashMap<SettingsSection, UiSettingsSection>> =
        StoredValue::new(sections_dict.clone());

    let current_section = Memo::new(move |_| {
        sections_dict
            .get(&selected_section.get())
            .unwrap_or(&PROFILE_SECTION)
            .clone()
    });

    let search_results = Memo::new(move |_| {
        let query = search_query.get();
        let trimmed = query.trim();
        if trimmed.is_empty() {
            Vec::new()
        } else {
            settings.search(trimmed)
        }
    });

    let section_bar_view = move |section: SettingsItem| match section {
        SettingsItem::Divider => {
            view! { <div class="border-t border-(--tile-border-color) my-1" /> }.into_any()
        }
        SettingsItem::Section(section) => {
            let status_sig = *statuses.get_value().get(&section.id).unwrap();

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
                        class="opacity-100 text-bright max-w-300 w-[80vw] h-full max-h-[95vh] min-h-[50vh] flex flex-row overflow-hidden z-50 ui-solid-bg"
                    >
                        <div class="border-r border-(--tile-border-color) w-60 h-full flex flex-col gap-(--gap)">
                            <div
                                class="flex items-center p-2 pb-0 gap-2 cursor-pointer group"
                                on:click=move |_| selected_section.set(PROFILE_SECTION.id)
                            >
                                <div
                                    class="border border-transparent group-hover:border-(--tile-border-color) flex flex-row rounded-[10px] p-1 items-center justify-center flex-1"
                                    class=(
                                        "bg-(--ui-solid-hover-bg)",
                                        move || selected_section.get() == PROFILE_SECTION.id,
                                    )
                                >
                                    {move || user_sig.get().get().render_icon("40px", cache)}
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
                                            "Edit profile(s)"
                                        </span>
                                    </div>
                                </div>
                            </div>
                            <div class="px-2 py-1">
                                <div class="relative flex items-center">
                                    <div class="absolute left-2 flex items-center pointer-events-none text-muted">
                                        <Icon
                                            icon=MAGNIFYING_GLASS
                                            weight=IconWeight::Bold
                                            size="14px"
                                        />
                                    </div>
                                    <input
                                        type="text"
                                        placeholder="Search settings..."
                                        class="w-full ui-solid-bg border border-(--tile-border-color) rounded-ui pl-7 pr-2 py-1 text-sm text-normal outline-none placeholder:text-muted"
                                        prop:value=move || search_query.get()
                                        on:input=move |ev| search_query.set(event_target_value(&ev))
                                    />
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
                                    {move || {
                                        if search_query.get().trim().is_empty() {
                                            current_section.get().title
                                        } else {
                                            "Search results"
                                        }
                                    }}
                                </span>
                                <CloseButton
                                    on_click=move |_| set_is_open.set(false)
                                    size="20px"
                                    inset="12px"
                                />
                            </div>
                            <div class="p-4 w-full h-full overflow-y-auto">
                                {move || {
                                    if search_query.get().trim().is_empty() {
                                        (current_section.get().render_fn)()
                                    } else {
                                        let results = search_results.get();
                                        if results.is_empty() {
                                            view! {
                                                <div class="text-dim text-sm italic p-2">
                                                    "No settings found."
                                                </div>
                                            }
                                                .into_any()
                                        } else {
                                            results
                                                .into_iter()
                                                .map(|(section, setting)| {
                                                    let section_title = sections_dict_for_search
                                                        .with_value(|d| { d.get(&section).map(|s| s.title) })
                                                        .unwrap_or(PROFILE_SECTION.title);
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="flex flex-row gap-0.5 w-full text-left px-3 py-2 rounded-lg border border-transparent hover:border-(--tile-border-color) hover:bg-(--tile-hover-color) items-center cursor-pointer text-normal"
                                                            on:click=move |_| {
                                                                selected_section.set(section);
                                                                search_query.set(String::new());
                                                                let type_name = setting.type_name;
                                                                request_animation_frame(move || {
                                                                    scroll_to_setting(type_name);
                                                                });
                                                            }
                                                        >
                                                            <h2 class="text-lg font-semibold">{section_title}</h2>
                                                            <Icon icon=CARET_DOUBLE_RIGHT size="20px" />
                                                            <span class="text-dim">{setting.human_readable}</span>
                                                            <div title=setting.description class="flex items-center">
                                                                <Icon
                                                                    icon=QUESTION
                                                                    size="14px"
                                                                    color="var(--dim-text-color)"
                                                                />
                                                            </div>
                                                        </button>
                                                    }
                                                        .into_any()
                                                })
                                                .collect_view()
                                                .into_any()
                                        }
                                    }
                                }}
                            </div>
                        </div>
                    </FloatingTile>
                </div>
            </Portal>
        </Show>
    }
}
