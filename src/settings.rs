use anyhow::{Context, Result};
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoClose {
    #[default]
    FiveSeconds,
    FifteenSeconds,
    ThirtySeconds,
    FiveMinutes,
    TenMinutes,
    Never,
}
impl AutoClose {
    pub const ALL: [Self; 6] = [
        Self::FiveSeconds,
        Self::FifteenSeconds,
        Self::ThirtySeconds,
        Self::FiveMinutes,
        Self::TenMinutes,
        Self::Never,
    ];
    pub fn duration(self) -> Option<Duration> {
        match self {
            Self::Never => None,
            _ => Some(Duration::from_secs(match self {
                Self::FiveSeconds => 5,
                Self::FifteenSeconds => 15,
                Self::ThirtySeconds => 30,
                Self::FiveMinutes => 300,
                Self::TenMinutes => 600,
                Self::Never => unreachable!(),
            })),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::FiveSeconds => "5 seconds",
            Self::FifteenSeconds => "15 seconds",
            Self::ThirtySeconds => "30 seconds",
            Self::FiveMinutes => "5 minutes",
            Self::TenMinutes => "10 minutes",
            Self::Never => "Never",
        }
    }
    fn value(self) -> &'static str {
        match self {
            Self::FiveSeconds => "5",
            Self::FifteenSeconds => "15",
            Self::ThirtySeconds => "30",
            Self::FiveMinutes => "300",
            Self::TenMinutes => "600",
            Self::Never => "never",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.value() == value)
            .context("Unsupported auto-close interval")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}
impl Placement {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
        }
    }
    fn value(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopRight => "top_right",
            Self::BottomLeft => "bottom_left",
            Self::BottomRight => "bottom_right",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.value() == value)
            .context("Unsupported Quick Access position")
    }
    pub fn at_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }
    pub fn at_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub auto_close: AutoClose,
    pub placement: Placement,
}
impl Settings {
    fn path() -> Result<PathBuf> {
        Ok(crate::storage::temp_directory()?
            .parent()
            .context("Missing settings directory")?
            .join("settings.txt"))
    }
    fn parse(text: &str) -> Result<Self> {
        let mut settings = Self::default();
        let mut timeout_seen = false;
        let mut placement_seen = false;
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("auto_close=") {
                anyhow::ensure!(!timeout_seen, "Duplicate auto-close setting");
                settings.auto_close = AutoClose::parse(value)?;
                timeout_seen = true;
            } else if let Some(value) = line.strip_prefix("placement=") {
                anyhow::ensure!(!placement_seen, "Duplicate Quick Access position");
                settings.placement = Placement::parse(value)?;
                placement_seen = true;
            } else {
                anyhow::bail!("Unknown screenshot setting");
            }
        }
        anyhow::ensure!(timeout_seen, "Missing auto-close setting");
        Ok(settings)
    }
    pub fn load() -> Self {
        let result = (|| -> Result<Self> {
            let local =
                PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?);
            Self::load_from(
                &Self::path()?,
                &local.join("SimpleScreenshot").join("settings.txt"),
            )
        })();
        result.unwrap_or_else(|error| {
            eprintln!("Settings ignored: {error:#}");
            Self::default()
        })
    }
    fn load_from(path: &std::path::Path, legacy: &std::path::Path) -> Result<Self> {
        if path.exists() {
            return Self::parse(&std::fs::read_to_string(path)?);
        }
        if legacy.exists() {
            // Copy only preferences, never move the old PNGs: other programs may
            // still hold paths to those files. Repeated starts use the new copy.
            let settings = Self::parse(&std::fs::read_to_string(legacy)?)?;
            settings.save_to(path)?;
            return Ok(settings);
        }
        Ok(Self::default())
    }
    pub fn save(self) -> Result<()> {
        self.save_to(&Self::path()?)
    }
    fn save_to(self, path: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(path.parent().context("Missing settings directory")?)?;
        let pending = path.with_extension("txt.part");
        std::fs::write(
            &pending,
            format!(
                "auto_close={}\nplacement={}\n",
                self.auto_close.value(),
                self.placement.value()
            ),
        )?;
        let result = std::fs::rename(&pending, path).context("Cannot publish screenshot settings");
        if result.is_err() {
            let _ = std::fs::remove_file(pending);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_defaults_and_both_choices_round_trip() {
        for timeout in AutoClose::ALL {
            assert_eq!(
                Settings::parse(&format!("auto_close={}\n", timeout.value())).unwrap(),
                Settings {
                    auto_close: timeout,
                    placement: Placement::BottomRight
                }
            );
            for placement in Placement::ALL {
                assert_eq!(
                    Settings::parse(&format!(
                        "auto_close={}\nplacement={}\n",
                        timeout.value(),
                        placement.value()
                    ))
                    .unwrap(),
                    Settings {
                        auto_close: timeout,
                        placement
                    }
                );
            }
        }
        for text in [
            "",
            "auto_close=0",
            "auto_close=999999",
            "auto_close=5\nextra=true",
            "auto_close=5\nplacement=somewhere",
            "auto_close=5\nauto_close=15",
        ] {
            assert!(Settings::parse(text).is_err(), "{text}");
        }
        assert_eq!(AutoClose::Never.duration(), None);
    }
    #[test]
    fn rebrand_imports_preferences_once_without_moving_legacy_data() -> Result<()> {
        let folder = std::env::temp_dir().join(format!(
            "catch it migration {} {}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let legacy = folder.join("SimpleScreenshot").join("settings.txt");
        let current = folder.join("CatchIt").join("settings.txt");
        std::fs::create_dir_all(legacy.parent().unwrap())?;
        std::fs::write(&legacy, "auto_close=never\nplacement=top_left\n")?;
        assert_eq!(
            Settings::load_from(&current, &legacy)?,
            Settings {
                auto_close: AutoClose::Never,
                placement: Placement::TopLeft
            }
        );
        assert_eq!(
            std::fs::read_to_string(&legacy)?,
            "auto_close=never\nplacement=top_left\n"
        );
        std::fs::write(&legacy, "auto_close=5\n")?;
        assert_eq!(
            Settings::load_from(&current, &legacy)?.placement,
            Placement::TopLeft
        );
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }
    #[test]
    fn saving_one_choice_preserves_the_other() -> Result<()> {
        let folder = std::env::temp_dir().join(format!(
            "screenshot settings {} {}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let path = folder.join("settings.txt");
        let mut settings = Settings {
            auto_close: AutoClose::Never,
            placement: Placement::TopLeft,
        };
        settings.save_to(&path)?;
        settings.auto_close = AutoClose::FifteenSeconds;
        settings.save_to(&path)?;
        assert_eq!(Settings::parse(&std::fs::read_to_string(&path)?)?, settings);
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }
}
