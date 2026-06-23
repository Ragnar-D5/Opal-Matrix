use std::{collections::HashMap, str::FromStr};

use crate::{state::AudioManager, TauriError};
use cpal::{traits::DeviceTrait, Device, DeviceId};
use shared::api::{AudioDevice, AudioDeviceInfos};
use tauri::{command, AppHandle, Emitter, State};

fn is_relevant_device(driver: &str) -> bool {
    driver == "default" || (driver.starts_with("plughw:") && driver.contains(",DEV=0"))
}

#[command]
pub async fn set_output_device(
    id: String,
    manager: State<'_, AudioManager>,
    handle: AppHandle,
) -> Result<(), TauriError> {
    let device_id = DeviceId::from_str(&id)?;

    manager.inner().refresh_devices(handle)?;

    let device = {
        let output_devices = manager.output_devices.lock()?;
        let Some(device) = output_devices.get(&device_id) else {
            log::warn!("No device with id {id} found");
            return Ok(());
        };
        device.clone()
    };

    manager.try_setup_output_stream_for_device(&device)?;

    Ok(())
}

#[command]
pub async fn get_audio_devices(
    manager: State<'_, AudioManager>,
    handle: AppHandle,
) -> Result<(), TauriError> {
    manager.inner().refresh_devices(handle)?;

    Ok(())
}

#[command]
pub async fn set_input_device(
    id: String,
    manager: State<'_, AudioManager>,
    handle: AppHandle,
) -> Result<(), TauriError> {
    let device_id = DeviceId::from_str(&id)?;

    manager.inner().refresh_devices(handle)?;

    let device = {
        let input_devices = manager.input_devices.lock()?;
        let Some(device) = input_devices.get(&device_id) else {
            log::warn!("No device with id {id} found");
            return Ok(());
        };
        device.clone()
    };

    manager.try_setup_input_stream_for_device(&device)?;

    Ok(())
}

pub fn emit_devices_update(
    input_devices: HashMap<DeviceId, Device>,
    output_devices: HashMap<DeviceId, Device>,
    default_input_device_id: Option<String>,
    default_output_device_id: Option<String>,
    active_input_id: Option<String>,
    active_output_id: Option<String>,
    handle: AppHandle,
) {
    let input_devices = input_devices
        .iter()
        .filter_map(|(id, d)| {
            let desc = d.description().ok()?;
            if !is_relevant_device(desc.driver().unwrap_or_default()) {
                return None;
            }
            Some(AudioDevice {
                id: id.to_string(),
                name: desc.name().to_string(),
            })
        })
        .collect();

    let output_devices = output_devices
        .iter()
        .filter_map(|(id, d)| {
            let desc = d.description().ok()?;
            if !is_relevant_device(desc.driver().unwrap_or_default()) {
                return None;
            }
            Some(AudioDevice {
                id: id.to_string(),
                name: desc.name().to_string(),
            })
        })
        .collect();

    let payload = AudioDeviceInfos {
        input_devices,
        output_devices,

        default_input_device_id,
        default_output_device_id,

        active_input_device_id: active_input_id,
        active_output_devive_id: active_output_id,
    };

    log::debug!(
        "Emitting audio_device_update event with payload: {:?}",
        payload
    );

    handle.emit("audio_device_update", payload).unwrap();
}
