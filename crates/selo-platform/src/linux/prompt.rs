use crate::linux::{block_on, file_uri_to_path};
use selo_core::{Error, Result};
use std::path::PathBuf;

pub fn pick_file(title: &str) -> Result<Option<PathBuf>> {
    block_on(choose(title)).map_err(Error::Platform)
}

async fn choose(title: &str) -> std::result::Result<Option<PathBuf>, String> {
    use ashpd::desktop::file_chooser::SelectedFiles;

    let connection = crate::linux::portal_connection().await?;
    let request = SelectedFiles::open_file()
        .connection(Some(connection))
        .title(title)
        .modal(true)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let response = request.response().map_err(|err| err.to_string())?;
    Ok(response
        .uris()
        .first()
        .and_then(|uri| file_uri_to_path(uri.as_str())))
}
