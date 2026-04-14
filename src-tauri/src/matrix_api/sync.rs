use crate::{construct_url, matrix_api::crypto, AppState, TauriError};
use log::info;
use reqwest::Client;

use ruma::api::{client::sync::sync_events::v3::Response as SyncResponse, IncomingResponse};
use tokio_util::sync::CancellationToken;

async fn matrix_sync(
    access_token: &String,
    matrix_url: &String,
    since: Option<String>,
) -> Result<SyncResponse, TauriError> {
    let client = Client::new();

    let mut url = construct_url(vec![
        matrix_url,
        &"_matrix".to_string(),
        &"client".to_string(),
        &"v3".to_string(),
        &"sync".to_string(),
    ])?;

    if let Some(since_token) = since {
        let params = [("since", since_token), ("timeout", 30000.to_string())];

        url = reqwest::Url::parse_with_params(url.as_str(), params)?;
    }

    let res = client
        .get(url)
        .timeout(std::time::Duration::from_secs(35))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = res.status();
    let headers = res.headers().clone();
    let body_bytes = res
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let mut builder = http::Response::builder().status(status);

    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    let http_response = builder
        .body(body_bytes.to_vec())
        .map_err(|e| format!("Failed to build HTTP response: {e}"))?;

    match SyncResponse::try_from_http_response(http_response) {
        Ok(sync_response) => Ok(sync_response),
        Err(e) => Err(format!("Failed to parse sync response: {e}").into()),
    }
}

impl AppState {
    pub(crate) async fn start_sync(self: &std::sync::Arc<Self>) -> Result<(), TauriError> {
        let mut task_guard = self.sync_task.lock().await;
        if task_guard.is_some() {
            return Ok(());
        }

        let cancel = CancellationToken::new();
        {
            let mut cancel_guard = self.sync_cancel_token.lock().await;
            *cancel_guard = Some(cancel.clone());
        }

        let state = self.clone();
        let handle = tauri::async_runtime::spawn(async move {
            if let Err(e) = run_sync_loop(state).await {
                log::error!("Sync loop error: {:?}", e);
            }
        });

        *task_guard = Some(handle);
        Ok(())
    }

    pub(crate) async fn stop_sync(&self) -> Result<(), TauriError> {
        if let Some(cancel) = self.sync_cancel_token.lock().await.take() {
            cancel.cancel();
        }

        if let Some(handle) = self.sync_task.lock().await.take() {
            let _ = handle.await;
        }

        Ok(())
    }

    pub(crate) async fn restart_sync(self: &std::sync::Arc<Self>) -> Result<(), TauriError> {
        self.stop_sync().await?;
        self.start_sync().await
    }
}

async fn run_sync_loop(state: std::sync::Arc<AppState>) -> Result<(), TauriError> {
    let mut since = {
        let guard = state.next_batch.read().await;
        guard.clone()
    };

    let cancel = {
        let cancel_guard = state.sync_cancel_token.lock().await;
        if let Some(cancel) = cancel_guard.as_ref() {
            cancel.clone()
        } else {
            return Err("Sync cancellation token not found".into());
        }
    };

    while !cancel.is_cancelled() {
        let access_token = state.check_token().await?;
        let matrix_url = {
            let guard = state.matrix_url.read().await;
            guard.as_ref().cloned().ok_or("Matrix URL not set")?
        };

        let sync_res = matrix_sync(&access_token, &matrix_url, since.clone()).await?;
        since = Some(sync_res.next_batch.clone());

        {
            let mut since_guard = state.next_batch.write().await;
            *since_guard = since.clone();
        }

        let olm_machine = {
            let guard = state.crypto_machine.lock().await;
            guard
                .as_ref()
                .cloned()
                .ok_or("Crypto machine not initialized")?
        };

        let res = crypto::process_sync_response(&olm_machine, sync_res, &access_token, &matrix_url)
            .await?;

        println!("Processed sync response: {:?}", res);

        state.save_session().await?;
    }

    Ok(())
}
