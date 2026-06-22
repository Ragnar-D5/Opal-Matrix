use crate::{TauriError, state::AudioManager};
use cpal::DeviceId;
use tauri::{State, command};

#[command]
pub async fn set_output_device(
    id: DeviceId,
    manager: State<'_, AudioManager>,
) -> Result<(), TauriError> {
    manager.inner().refresh_devices()?;

    let output_devices = manager.output_devices.lock().await;
    let Some(device) = output_devices.get(&id) else {
        log::warn!("No device with id {id} found");
        return Ok(());
    };

    manager.try_setup_output_stream_for_device(device)?;

    Ok(())
}

#[command]
pub async fn set_input_device(
    id: DeviceId,
    manager: State<'_, AudioManager>,
) -> Result<(), TauriError> {
    manager.inner().refresh_devices();

    let input_devices = manager.input_devices.lock().await;
    let Some(device) = input_devices.get(&id) else {
        log::warn!("No device with id {id} found");
        return Ok(());
    };

    manager.try_setup_input_stream_for_device(device)?;

    Ok(())
}
