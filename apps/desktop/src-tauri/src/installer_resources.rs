use std::{fs, path::PathBuf, process::Command};
use tauri::{AppHandle, Manager};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialSetupFeatures {
    pub roblox: bool,
    pub erlcx: bool,
}

#[tauri::command]
pub fn consume_initial_setup_features() -> Result<Option<InitialSetupFeatures>, String> {
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
            RegKey,
        };

        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let key = match current_user
            .open_subkey_with_flags("Software\\ApexStudio\\Setup", KEY_READ | KEY_WRITE)
        {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not read installer setup choices: {error}")),
        };
        let choices = match key.get_value::<String, _>("InitialSetupFeatures") {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not read installer setup choices: {error}")),
        };
        key.delete_value("InitialSetupFeatures")
            .map_err(|error| format!("Could not clear installer setup choices: {error}"))?;
        let selected: Vec<_> = choices.split(',').collect();
        return Ok(Some(InitialSetupFeatures {
            roblox: selected.contains(&"roblox"),
            erlcx: selected.contains(&"erlcx"),
        }));
    }
    #[cfg(not(windows))]
    Ok(None)
}

const EXTENSION_RESOURCE: &str = "apex-erlcx-extension.zip";
const EXTENSION_DOWNLOAD_NAME: &str = "Apex-Studio-ERLCX-Extension.zip";

fn bundled_extension_path(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("Could not locate Apex Studio resources: {error}"))?;
    let path = resource_dir.join(EXTENSION_RESOURCE);
    if !path.is_file() {
        return Err(
            "The ERLCX extension package is not included in this installation. Reinstall Apex Studio to repair it.".into(),
        );
    }
    Ok(path)
}

fn extension_download_path(app: &AppHandle) -> Result<PathBuf, String> {
    let downloads = app
        .path()
        .download_dir()
        .map_err(|error| format!("Could not locate the Downloads folder: {error}"))?;
    Ok(downloads.join(EXTENSION_DOWNLOAD_NAME))
}

#[tauri::command]
pub fn copy_erlcx_extension_to_downloads(app: AppHandle) -> Result<String, String> {
    let source = bundled_extension_path(&app)?;
    let destination = extension_download_path(&app)?;
    fs::copy(&source, &destination)
        .map_err(|error| format!("Could not copy the ERLCX extension package: {error}"))?;
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_erlcx_extension_in_explorer(app: AppHandle) -> Result<(), String> {
    let path = extension_download_path(&app)?;
    if !path.is_file() {
        return Err("Copy the ERLCX extension package to Downloads first.".into());
    }
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map_err(|error| format!("Could not open the extension package in Explorer: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::EXTENSION_DOWNLOAD_NAME;

    #[test]
    fn download_name_is_stable_and_descriptive() {
        assert_eq!(EXTENSION_DOWNLOAD_NAME, "Apex-Studio-ERLCX-Extension.zip");
    }
}
