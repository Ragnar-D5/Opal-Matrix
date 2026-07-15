use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_md::{Markdown, MarkdownOptions};
use phosphor_leptos::{
    CHECK_CIRCLE, DOWNLOAD, INFO, Icon, IconWeight, SPINNER, WARNING, WARNING_DIAMOND,
};
use shared::api::{UpdateDownloadProgress, UpdateStatus};

use crate::components::settings::Settings;
use crate::components::settings::sections::{Spacer, SubSection, Toggle};
use crate::tauri_functions::{
    check_for_update, download_update, get_version, get_versions, install_update, recheck_update,
};

use crate::app::format_bytes;
use crate::state::AppState;

pub fn render_update_section() -> AnyView {
    let state: AppState = expect_context();

    let status = Memo::new(move |_| state.update_status.get());

    let button_color = move || match status.get() {
        UpdateStatus::UpdateAvailable(_) => "--success-color".to_string(),
        UpdateStatus::Error { .. } => "--error-color".to_string(),
        UpdateStatus::ReadyToInstall(_) => "--purple".to_string(),
        UpdateStatus::UpToDate => "--accent-color".to_string(),
        UpdateStatus::CheckingForUpdates => "--offline-color".to_string(),
        UpdateStatus::Downloading(_) => "--success-color".to_string(),
    };

    let header_view = move || {
        let status = status.get();
        let progress = state.update_progress.get();
        let app_version = state.app_version.get();

        let color = format!("var({})", button_color());

        let (text, icon) = match status {
            UpdateStatus::UpToDate => (format!("Up to date ({})", app_version), INFO),
            UpdateStatus::UpdateAvailable(info) => (
                format!(
                    "Update available ({} ⟶ {})",
                    info.current_version, info.version
                ),
                WARNING,
            ),
            UpdateStatus::Downloading(info) => {
                let text = match progress {
                    UpdateDownloadProgress::Finished => {
                        format!("Downloaded version {}", info.version)
                    }
                    UpdateDownloadProgress::InProgress { progress, total } => {
                        let percentage = if let Some(total) = total {
                            format!(" ({}%)", (progress as f64 / total as f64 * 100.0).round())
                        } else {
                            "".to_string()
                        };

                        format!("Downloading version {} {}", info.version, percentage)
                    }
                    UpdateDownloadProgress::Started => {
                        format!("Downloading version {}", info.version)
                    }
                };

                (text, DOWNLOAD)
            }
            UpdateStatus::ReadyToInstall(info) => (
                format!(
                    "Ready to install ({} ⟶ {})",
                    info.current_version, info.version
                ),
                CHECK_CIRCLE,
            ),
            UpdateStatus::Error { short, .. } => {
                (format!("Update error: {short}"), WARNING_DIAMOND)
            }
            UpdateStatus::CheckingForUpdates => ("Checking for updates...".to_string(), SPINNER),
        };

        let bg_color = format!("rgb(from {color} r g b / 20%)");

        view! {
            <div
                class="px-4 py-2 rounded-ui text-sm font-medium border border-(--tile-border-color) flex flex-row items-center gap-2"
                style=format!("background-color: {bg_color}; color: {color};")
            >
                <Icon icon=icon size="20px" />
                <span>{text}</span>
            </div>
        }
    };

    let is_downloading = move || matches!(status.get(), UpdateStatus::Downloading(_));

    let progress_percent = move || -> f64 {
        match state.update_progress.get() {
            UpdateDownloadProgress::InProgress { progress, total } => total
                .map(|total| (progress as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
                .unwrap_or(100.0),
            UpdateDownloadProgress::Finished => 100.0,
            UpdateDownloadProgress::Started => 0.0,
        }
    };

    let on_button_click = move |_| match status.get() {
        UpdateStatus::UpdateAvailable(_) => {
            state.update_progress.set(UpdateDownloadProgress::Started);
            download_update();
        }
        UpdateStatus::Error { .. } => {
            state.update_status.set(UpdateStatus::CheckingForUpdates);
            recheck_update();
        }
        UpdateStatus::ReadyToInstall(_) => {
            state.update_status.set(UpdateStatus::CheckingForUpdates);
            install_update();
        }
        UpdateStatus::UpToDate => {
            state.update_status.set(UpdateStatus::CheckingForUpdates);
            check_for_update();
        }
        _ => (),
    };

    let button_label = move || match status.get() {
        UpdateStatus::UpdateAvailable(_) => "Download",
        UpdateStatus::Error { .. } => "Retry",
        UpdateStatus::ReadyToInstall(_) => "Install",
        UpdateStatus::UpToDate => "Check for updates",
        UpdateStatus::CheckingForUpdates => "Waiting for update to download...",
        UpdateStatus::Downloading(_) => "Downloading...",
    };

    let button_content = move || {
        if is_downloading() {
            let text = move || match state.update_progress.get() {
                UpdateDownloadProgress::InProgress { progress, total } => {
                    if let Some(total) = total {
                        format!(
                            "Downloading... ({}/{})",
                            format_bytes(progress as u64).get(),
                            format_bytes(total).get()
                        )
                    } else {
                        format!("Downloading... ({})", format_bytes(progress as u64).get())
                    }
                }
                UpdateDownloadProgress::Finished => "Downloaded".to_string(),
                UpdateDownloadProgress::Started => "Starting download...".to_string(),
            };

            return view! {
                <div class="flex flex-col items-center justify-center gap-1.5 w-full px-4">
                    <span class="relative z-10 text-normal">{text}</span>
                    <div class="relative w-full h-2 rounded-full bg-white/15 overflow-hidden">
                        <div
                            class="absolute inset-y-0 left-0 rounded-full bg-(--success-color) animate-shimmer transition-[width] duration-300 ease-out"
                            style=move || format!("width: {}%;", progress_percent())
                        />
                    </div>
                </div>
            }
            .into_any();
        }

        let icon = match status.get() {
            UpdateStatus::UpdateAvailable(_) => DOWNLOAD,
            UpdateStatus::Error { .. } => WARNING_DIAMOND,
            UpdateStatus::ReadyToInstall(_) => CHECK_CIRCLE,
            UpdateStatus::UpToDate => INFO,
            _ => SPINNER,
        };

        view! { <Icon icon=icon size="40px" weight=IconWeight::Bold color="var(--ui-solid-bg)" /> }
            .into_any()
    };

    let settings: Settings = expect_context();

    let versions: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    spawn_local(async move {
        match get_versions().await {
            Ok(vs) => versions.set(vs),
            Err(e) => log::error!("Failed to get versions: {}", e),
        }
    });

    view! {
        <div class="flex flex-col gap-4 mb-4">
            {header_view} <div class="relative h-20">
                <div
                    class=move || {
                        let base = "relative shrink-0 flex items-center justify-center overflow-hidden text-white transition-all duration-300 ease-in-out h-20 border border-(--tile-border-color)";
                        if is_downloading() {
                            let color = if matches!(
                                state.update_status.get(),
                                UpdateStatus::Downloading(_)
                            ) {
                                "--ui-solid-hover-bg".to_string()
                            } else {
                                button_color()
                            };
                            format!("{base} w-100 bg-({})", color)
                        } else {
                            format!(
                                "{base} w-20 shadow-[0_4px_0_0_rgba(0,0,0,0.35)] active:shadow-[0_1px_0_0_rgba(0,0,0,0.35)] active:translate-y-[3px] bg-({})",
                                button_color(),
                            )
                        }
                    }
                    style:border-radius=move || {
                        if is_downloading() {
                            "var(--ui-border-radius)".to_string()
                        } else {
                            "40px".to_string()
                        }
                    }
                >
                    {button_content}
                </div>
                <button
                    class=move || {
                        let base = format!(
                            "absolute left-24 top-1/2 -translate-y-1/2 whitespace-nowrap text-sm transition-opacity duration-300 ease-in-out border border-(--tile-border-color) px-3 py-1 rounded-ui bg-(--overlay-bg-color) cursor-pointer hover:bg-(--ui-solid-hover-bg) text-({}) select-none {}",
                            button_color(),
                            if status.get() == UpdateStatus::CheckingForUpdates {
                                "pointer-events-none"
                            } else {
                                ""
                            },
                        );
                        if is_downloading() {
                            format!("{base} opacity-0 pointer-events-none")
                        } else {
                            format!("{base} opacity-100")
                        }
                    }
                    class=("opacity-0", is_downloading)
                    class=("opacity-100", move || !is_downloading())
                    disabled=is_downloading
                    on:click=on_button_click
                >
                    {button_label}
                </button>
            </div>
        </div>
        <Toggle field=settings.auto_download_update />
        <Toggle field=settings.notify_update />
        <Spacer />
        <For
            each=move || versions.get().into_iter().enumerate()
            key=|(_, version)| version.clone()
            children=move |(idx, version)| render_release(idx, StoredValue::new(version))
        />
    }
        .into_any()
}

fn render_release(idx: usize, version: StoredValue<String>) -> AnyView {
    let expanded = RwSignal::new(idx == 0);

    let notes = RwSignal::new(String::new());

    Effect::new(move || {
        if expanded.get() {
            spawn_local(async move {
                match get_version(&version.get_value()).await {
                    Ok(text) => notes.set(text),
                    Err(e) => log::error!("Failed to get notes: {e}"),
                }
            });
        }
    });

    view! {
        <SubSection title=version.get_value() expanded=expanded>
            {move || {
                let options = MarkdownOptions::new().without_code_theme();
                view! {
                    <Markdown content=notes.get() class="text-normal".to_string() options=options />
                }
            }}
        </SubSection>
    }
    .into_any()
}
