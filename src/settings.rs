//! What is remembered between runs: the port and the panel's size and
//! type, in a small TOML file in the user's configuration directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ddp;
use crate::panel::PanelType;
use crate::size::Size;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The UDP port to listen on.
    pub port: u16,
    /// Whether the size is guessed from the frames (and guessed again when
    /// their length changes) or held at `width` and `height`.
    pub auto_size: bool,
    /// The size last shown: the one set, or the last guess.
    pub width: u32,
    pub height: u32,
    /// What the pixels look like.
    pub panel: PanelType,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: ddp::PORT,
            auto_size: true,
            width: 64,
            height: 64,
            panel: PanelType::PixelArt,
        }
    }
}

impl Settings {
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Where the file is: `~/Library/Application Support/ddpview` on macOS,
    /// `~/.config/ddpview` on Linux, `%APPDATA%\ddpview\config` on Windows.
    /// `None` when the system does not say where the user's home is.
    pub fn path() -> Option<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "ddpview")?;
        Some(dirs.config_dir().join("settings.toml"))
    }

    /// Reads the file. No file is the defaults; a file that cannot be read
    /// as settings is the defaults too, with the reason to tell the user,
    /// and is replaced the next time something is saved.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => return (Self::default(), Some(format!("{}: {e}", path.display()))),
        };
        match toml::from_str::<Self>(&text) {
            Ok(settings) if settings.size().fits() && settings.port != 0 => (settings, None),
            Ok(_) => (
                Self::default(),
                Some(format!("{}: a size or port out of range", path.display())),
            ),
            Err(e) => (
                Self::default(),
                Some(format!("{}: {}", path.display(), e.message())),
            ),
        }
    }

    /// Writes the file, by way of a temporary one beside it so that a
    /// crash part-way leaves the old settings, not half of the new.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of this test's own under the system's temporary one.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ddpview-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn settings_come_back_as_saved() {
        let dir = folder("round-trip");
        let path = dir.join("nested").join("settings.toml");
        assert_eq!(
            Settings::load(&path),
            (Settings::default(), None),
            "no file yet"
        );

        let settings = Settings {
            port: 4049,
            auto_size: false,
            width: 128,
            height: 64,
            panel: PanelType::P3Smd2121,
        };
        settings.save(&path).expect("the folder is made as needed");
        assert_eq!(Settings::load(&path), (settings, None));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "port = 4049\nauto_size = false\nwidth = 128\nheight = 64\npanel = \"p3-smd2121\"\n",
            "plain enough to edit by hand"
        );
        assert!(!path.with_extension("toml.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_may_leave_settings_out() {
        let dir = folder("partial");
        let path = dir.join("settings.toml");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "port = 5000\n").unwrap();
        let (settings, warning) = Settings::load(&path);
        assert_eq!(
            settings,
            Settings {
                port: 5000,
                ..Settings::default()
            }
        );
        assert_eq!(warning, None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_bad_file_is_the_defaults_and_a_warning() {
        let dir = folder("bad");
        let path = dir.join("settings.toml");
        fs::create_dir_all(&dir).unwrap();
        for bad in [
            "port = \"many\"\n",
            "width = 0\n",
            "height = 5000\n",
            "port = 0\n",
            "panel = \"p10\"\n",
            "not toml",
        ] {
            fs::write(&path, bad).unwrap();
            let (settings, warning) = Settings::load(&path);
            assert_eq!(settings, Settings::default(), "{bad:?}");
            assert!(
                warning.expect("a reason").contains("settings.toml"),
                "{bad:?}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_defaults_are_ddp_on_a_64x64_panel_guessed() {
        let d = Settings::default();
        assert_eq!(
            (d.port, d.auto_size, d.size(), d.panel),
            (4048, true, Size::new(64, 64), PanelType::PixelArt)
        );
    }
}
