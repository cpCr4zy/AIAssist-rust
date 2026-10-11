//! Browser storage: settings and API key in the site's `localStorage`.

use aiassist_core::AppSettings;

use crate::services::Store;

const SETTINGS_KEY: &str = "aiassistant.settings";
const API_KEY_KEY: &str = "aiassistant.api_key";

#[derive(Default)]
pub struct WebStore;

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn js_error(value: wasm_bindgen::JsValue) -> String {
    format!("{value:?}")
}

impl Store for WebStore {
    fn load(&mut self) -> AppSettings {
        let Some(storage) = local_storage() else {
            log::warn!("Browser storage unavailable; settings will not be kept.");
            return AppSettings::create_default();
        };
        match storage.get_item(SETTINGS_KEY) {
            Ok(Some(json)) => AppSettings::from_json(&json).unwrap_or_else(|e| {
                log::error!("Invalid stored settings, using defaults: {e}");
                AppSettings::create_default()
            }),
            Ok(None) => {
                log::warn!("No stored settings. Creating defaults.");
                AppSettings::create_default()
            }
            Err(e) => {
                log::error!("Failed to read stored settings: {}", js_error(e));
                AppSettings::create_default()
            }
        }
    }

    fn save(&mut self, settings: &AppSettings) -> Result<(), String> {
        let storage = local_storage().ok_or("browser storage unavailable")?;
        let json = settings.to_json().map_err(|e| e.to_string())?;
        storage.set_item(SETTINGS_KEY, &json).map_err(js_error)?;
        log::debug!("Settings saved.");
        Ok(())
    }

    fn api_key(&mut self) -> String {
        local_storage().and_then(|s| s.get_item(API_KEY_KEY).ok().flatten()).unwrap_or_default()
    }

    fn set_api_key(&mut self, api_key: &str) -> Result<(), String> {
        let storage = local_storage().ok_or("browser storage unavailable")?;
        let api_key = api_key.trim();
        if api_key.is_empty() { storage.remove_item(API_KEY_KEY) } else { storage.set_item(API_KEY_KEY, api_key) }.map_err(js_error)
    }

    fn api_key_note(&self) -> Option<&'static str> {
        Some("In the browser the key is kept in this site's local storage, readable by any script on the page.")
    }
}
