//! Desktop storage: `settings.json` in the config folder and the API key in the OS keyring.
//!
//! The layout and the keyring entry (service `aiassistant`, user `openrouter_api_key`) are the
//! Python AiAssistantLinux app's, so on Linux both apps share prompts and key. When no keyring is
//! available the key goes to `.api_key` beside the settings, readable only by the user.

use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use aiassist_core::AppSettings;
use aiassist_core::settings::MAX_SETTINGS_BYTES;

use crate::services::Store;

pub const KEYRING_SERVICE: &str = "aiassistant";
pub const KEYRING_USER: &str = "openrouter_api_key";
const SETTINGS_FILE: &str = "settings.json";
const API_KEY_FILE: &str = ".api_key";

pub struct FileStore {
    dir: PathBuf,
    use_keyring: bool,
    /// The key as last read or written, so the keyring is asked once.
    cached_key: Option<String>,
}

impl FileStore {
    /// Settings in `dir`, the API key in the OS keyring.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, use_keyring: true, cached_key: None }
    }

    /// Settings and API key both in `dir` (tests, and systems without a keyring).
    pub fn without_keyring(dir: PathBuf) -> Self {
        Self { dir, use_keyring: false, cached_key: None }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }

    fn api_key_path(&self) -> PathBuf {
        self.dir.join(API_KEY_FILE)
    }

    fn keyring_entry() -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
    }

    fn read_key_from_keyring(&self) -> Option<String> {
        if !self.use_keyring {
            return None;
        }
        match Self::keyring_entry().and_then(|e| e.get_password()) {
            Ok(key) if !key.is_empty() => Some(key),
            Ok(_) | Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                log::warn!("Keyring unavailable, falling back to file: {e}");
                None
            }
        }
    }

    fn write_key_to_keyring(&self, api_key: &str) -> Result<(), String> {
        if !self.use_keyring {
            return Err("keyring disabled".to_owned());
        }
        let entry = Self::keyring_entry().map_err(|e| e.to_string())?;
        let result = if api_key.is_empty() { entry.delete_credential() } else { entry.set_password(api_key) };
        match result {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn read_key_file(&self) -> String {
        match fs::read_to_string(self.api_key_path()) {
            Ok(key) => key.trim().to_owned(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                log::error!("Failed to read API key file: {e}");
                String::new()
            }
        }
    }

    fn write_key_file(&self, api_key: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.api_key_path();
        write_private(&path, api_key.as_bytes())
    }

    fn remove_key_file(&self) {
        match fs::remove_file(self.api_key_path()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => log::error!("Failed to remove API key file: {e}"),
        }
    }

    fn write_settings(&self, settings: &AppSettings) -> Result<(), String> {
        let json = settings.to_json().map_err(|e| e.to_string())?;
        fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let path = self.settings_path();
        // Write beside the file, then rename over it: a crash never leaves half a file behind.
        let tmp = self.dir.join(format!("{SETTINGS_FILE}.tmp"));
        fs::write(&tmp, json).and_then(|()| fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn backup_corrupt_file(&self) {
        let path = self.settings_path();
        let backup = self.dir.join(format!("{SETTINGS_FILE}.corrupt.{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
        match fs::rename(&path, &backup) {
            Ok(()) => log::warn!("Backed up corrupt file to {}", backup.display()),
            Err(e) => log::error!("Failed to backup corrupt settings file: {e}"),
        }
    }
}

impl Store for FileStore {
    fn load(&mut self) -> AppSettings {
        let path = self.settings_path();
        match read_capped(&path, MAX_SETTINGS_BYTES) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                log::warn!("Settings file not found. Creating defaults.");
                let settings = AppSettings::create_default();
                if let Err(e) = self.write_settings(&settings) {
                    log::error!("Failed to save settings: {e}");
                }
                settings
            }
            Err(e) => {
                log::error!("Failed to read settings file: {e}");
                AppSettings::create_default()
            }
            Ok(text) => match AppSettings::from_json(&text) {
                Ok(settings) => {
                    log::debug!("Settings loaded from {}.", path.display());
                    settings
                }
                Err(e) => {
                    log::error!("Invalid settings file: {e}");
                    self.backup_corrupt_file();
                    let settings = AppSettings::create_default();
                    if let Err(e) = self.write_settings(&settings) {
                        log::error!("Failed to save settings: {e}");
                    }
                    settings
                }
            },
        }
    }

    fn save(&mut self, settings: &AppSettings) -> Result<(), String> {
        self.write_settings(settings)?;
        log::debug!("Settings saved.");
        Ok(())
    }

    fn api_key(&mut self) -> String {
        if let Some(key) = &self.cached_key {
            return key.clone();
        }
        let key = self.read_key_from_keyring().unwrap_or_else(|| self.read_key_file());
        self.cached_key = Some(key.clone());
        key
    }

    fn set_api_key(&mut self, api_key: &str) -> Result<(), String> {
        let api_key = api_key.trim();
        match self.write_key_to_keyring(api_key) {
            Ok(()) => self.remove_key_file(),
            Err(e) => {
                if self.use_keyring {
                    log::warn!("Keyring unavailable, saving API key to file: {e}");
                }
                if api_key.is_empty() {
                    self.remove_key_file();
                } else {
                    self.write_key_file(api_key).map_err(|e| format!("{}: {e}", self.api_key_path().display()))?;
                }
            }
        }
        self.cached_key = Some(api_key.to_owned());
        Ok(())
    }
}

/// Reads a file, refusing one larger than `max` bytes without loading it.
fn read_capped(path: &Path, max: usize) -> io::Result<String> {
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    let mut text = String::new();
    fs::File::open(path)?.take(limit).read_to_string(&mut text)?;
    if text.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("larger than {max} bytes")));
    }
    Ok(text)
}

/// Writes a file only its owner can read (mode 0600 on Unix).
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    // An existing file keeps its old mode on open; tighten it explicitly.
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiassist_core::{PromptConfig, Theme};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("aiassist-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_file_is_created_with_defaults() {
        let dir = TempDir::new("missing");
        let mut store = FileStore::without_keyring(dir.0.clone());
        assert_eq!(store.load(), AppSettings::create_default());
        assert!(store.settings_path().is_file());
    }

    #[test]
    fn saves_and_loads_without_leaving_a_temp_file() {
        let dir = TempDir::new("roundtrip");
        let mut store = FileStore::without_keyring(dir.0.clone());
        let settings = AppSettings {
            prompts: vec![PromptConfig { title: "Перевод".into(), system_prompt: "Translate: ".into(), model: "m".into() }],
            theme: Theme::Dark,
            selected_prompt: "Перевод".into(),
        };
        store.save(&settings).unwrap();
        assert_eq!(FileStore::without_keyring(dir.0.clone()).load(), settings);
        assert!(!dir.0.join("settings.json.tmp").exists());
        // Non-ASCII text is stored as is, like the Python app does.
        assert!(fs::read_to_string(store.settings_path()).unwrap().contains("Перевод"));
    }

    #[test]
    fn a_corrupt_file_is_backed_up_and_replaced() {
        let dir = TempDir::new("corrupt");
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join("settings.json"), "{ not json").unwrap();
        let mut store = FileStore::without_keyring(dir.0.clone());
        assert_eq!(store.load(), AppSettings::create_default());
        let names: Vec<String> = fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().any(|n| n.starts_with("settings.json.corrupt.")), "{names:?}");
        assert!(AppSettings::from_json(&fs::read_to_string(store.settings_path()).unwrap()).is_ok());
    }

    #[test]
    fn the_api_key_falls_back_to_a_private_file() {
        let dir = TempDir::new("apikey");
        let mut store = FileStore::without_keyring(dir.0.clone());
        assert_eq!(store.api_key(), "");
        store.set_api_key(" sk-or-test ").unwrap();
        assert_eq!(FileStore::without_keyring(dir.0.clone()).api_key(), "sk-or-test");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(dir.0.join(".api_key")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        store.set_api_key("").unwrap();
        assert!(!dir.0.join(".api_key").exists());
        assert_eq!(FileStore::without_keyring(dir.0.clone()).api_key(), "");
    }

    #[test]
    fn an_oversized_file_is_refused() {
        let dir = TempDir::new("big");
        fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join("f");
        fs::write(&path, "x".repeat(11)).unwrap();
        assert!(read_capped(&path, 10).is_err());
        assert_eq!(read_capped(&path, 11).unwrap().len(), 11);
    }
}
