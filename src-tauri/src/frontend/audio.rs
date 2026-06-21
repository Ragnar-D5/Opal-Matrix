use crate::{TauriError, state::AudioManager};
use cpal::{
    Device, DeviceId,
    traits::{DeviceTrait, HostTrait},
};
use tauri::{State, command};

// #[command]
pub async fn get_output_devices(
    manager: State<'_, AudioManager>,
) -> Result<Vec<Device>, TauriError> {
    let context = manager.lock().await;
    let devices = context.host.devices()?;
    Ok(devices.filter(|d| d.supports_output()).collect())
}

#[command]
pub async fn set_output_device(
    id: DeviceId,
    manager: State<'_, AudioManager>,
) -> Result<(), TauriError> {
    let mut context = manager.lock().await;

    let devices = context.host.devices()?;
    let mut found_device = None;

    for device in devices {
        if device.id()? == id {
            found_device = Some(device);
            break;
        }
    }

    let device = found_device.ok_or(format!("No device found with id '{}'", id))?;
    if !device.supports_output() {
        Err("Device does not support output")?
    }

    context.output_device = Some(device);

    Ok(())
}

// #[command]
pub async fn get_input_devices(
    manager: State<'_, AudioManager>,
) -> Result<Vec<Device>, TauriError> {
    let context = manager.lock().await;
    let devices = context.host.devices()?;
    Ok(devices.filter(|d| d.supports_input()).collect())
}

#[command]
pub async fn set_input_device(
    id: DeviceId,
    manager: State<'_, AudioManager>,
) -> Result<(), TauriError> {
    let mut context = manager.lock().await;

    let devices = context.host.devices()?;
    let mut found_device = None;

    for device in devices {
        if device.id()? == id {
            found_device = Some(device);
            break;
        }
    }

    let device = found_device.ok_or(format!("No device found with id '{}'", id))?;
    if !device.supports_input() {
        Err("Device does not support input")?
    }

    context.input_device = Some(device);

    Ok(())
}
