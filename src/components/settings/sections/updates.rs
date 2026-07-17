use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_md::{Markdown, MarkdownOptions};
use phosphor_leptos::{
    ARROW_CLOCKWISE, CHECK_CIRCLE, CLOCK_CLOCKWISE, DOWNLOAD, INFO, Icon, IconWeight, SPINNER,
    WARNING, WARNING_DIAMOND,
};
use shared::api::{UpdateDownloadProgress, UpdateStatus};

use crate::components::settings::Settings;
use crate::components::settings::sections::{Spacer, SubSection, Toggle};
use crate::tauri_functions::{
    check_for_update, download_update, get_version, get_versions, install_update, recheck_update,
    restart,
};

use crate::app::format_bytes;
use crate::state::AppState;

pub fn render_update_section() -> AnyView {
    let state: AppState = expect_context();

    let status = Memo::new(move |_| state.update_status.get());

    let button_color = move || match status.get() {
        UpdateStatus::UpdateAvailable(_) => "var(--success-color)",
        UpdateStatus::Error { .. } => "var(--error-color)",
        UpdateStatus::ReadyToInstall(_) | UpdateStatus::Installing(_) => "var(--purple)",
        UpdateStatus::UpToDate => "var(--accent-color)",
        UpdateStatus::CheckingForUpdates => "var(--offline-color)",
        UpdateStatus::Downloading(_) => "var(--success-color)",
        UpdateStatus::RestartRequired => "var(--warning-color)",
    };

    let header_view = move || {
        let status = status.get();
        let progress = state.update_progress.get();
        let app_version = state.app_version.get();

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
            UpdateStatus::Installing(info) => (
                format!("Installing ({} ⟶ {})", info.current_version, info.version),
                CHECK_CIRCLE,
            ),
            UpdateStatus::Error { short, .. } => {
                (format!("Update error: {short}"), WARNING_DIAMOND)
            }
            UpdateStatus::CheckingForUpdates => ("Checking for updates...".to_string(), SPINNER),
            UpdateStatus::RestartRequired => (
                "Restart required to apply changes".to_string(),
                ARROW_CLOCKWISE,
            ),
        };

        let color = button_color();
        let bg_color = format!("rgb(from {color} r g b / 20%)");

        view! {
            <div
                class="p-2 rounded-ui text-sm font-medium border border-(--tile-border-color) flex flex-row items-center gap-2"
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
        UpdateStatus::UpdateAvailable(info) => {
            state.update_status.set(UpdateStatus::Downloading(info));
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
        UpdateStatus::RestartRequired => {
            state.update_status.set(UpdateStatus::RestartRequired);
            restart();
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
        UpdateStatus::Installing(_) => "Installing...",
        UpdateStatus::RestartRequired => "Restart",
    };

    let button_content = move || {
        if is_downloading() {
            let spans = move || match state.update_progress.get() {
                UpdateDownloadProgress::InProgress { progress, total } => {
                    if let Some(total) = total {
                        view! {
                            <span>"Downloading... ("</span>
                            <span class="font-mono">{format_bytes(progress as u64).get()}</span>
                            <span>"/"</span>
                            <span class="font-mono">{format_bytes(total).get()}</span>
                            <span>")"</span>
                        }
                        .into_any()
                    } else {
                        view! {
                            <span>"Downloading... ("</span>
                            <span class="font-mono">{format_bytes(progress as u64).get()}</span>
                            <span>")"</span>
                        }
                        .into_any()
                    }
                }
                UpdateDownloadProgress::Finished => view! { <span>"Downloaded"</span> }.into_any(),
                UpdateDownloadProgress::Started => {
                    view! { <span>"Starting download..."</span> }.into_any()
                }
            };

            return view! {
                <div class="flex flex-col items-center justify-center gap-1.5 w-full px-4">
                    <div class="relative z-10 text-normal">{spans}</div>
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

        let icon = if status.get().has_spinner() {
            SPINNER
        } else {
            match status.get() {
                UpdateStatus::UpdateAvailable(_) | UpdateStatus::Downloading(_) => DOWNLOAD,
                UpdateStatus::Error { .. } => WARNING_DIAMOND,
                UpdateStatus::ReadyToInstall(_) | UpdateStatus::Installing(_) => CHECK_CIRCLE,
                UpdateStatus::UpToDate => INFO,
                UpdateStatus::RestartRequired => ARROW_CLOCKWISE,
                UpdateStatus::CheckingForUpdates => CLOCK_CLOCKWISE,
            }
        };

        view! {
            <div class=("animate-spin", move || status.get().has_spinner())>
                <Icon icon=icon size="40px" weight=IconWeight::Bold color="var(--ui-solid-bg)" />
            </div>
        }
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
                            format!("{base} w-100")
                        } else {
                            format!(
                                "{base} w-20 shadow-[0_4px_0_0_rgba(0,0,0,0.35)] active:shadow-[0_1px_0_0_rgba(0,0,0,0.35)] active:translate-y-[3px] bg-({})",
                                button_color(),
                            )
                        }
                    }
                    style:color=move || {
                        if is_downloading() { "var(--ui-solid-hover-bg)" } else { button_color() }
                    }
                    style:background-color=move || {
                        if is_downloading() { "var(--ui-solid-hover-bg)" } else { button_color() }
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
                            "absolute left-24 top-1/2 -translate-y-1/2 whitespace-nowrap text-sm transition-opacity duration-300 ease-in-out border border-(--tile-border-color) px-3 py-1 rounded-ui bg-(--overlay-bg-color) cursor-pointer hover:bg-(--ui-solid-hover-bg) select-none {}",
                            if !status.get().has_action() { "pointer-events-none" } else { "" },
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
                    style:color=button_color
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
