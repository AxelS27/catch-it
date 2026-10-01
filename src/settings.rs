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
    fn parse(text: &str) -> Result<Self> {
        let value = text
            .trim()
            .strip_prefix("auto_close=")
            .context("Invalid auto-close setting")?;
        Self::ALL
            .into_iter()
            .find(|choice| choice.value() == value)
            .context("Unsupported auto-close interval")
    }
    fn path() -> Result<PathBuf> {
        Ok(crate::storage::temp_directory()?
            .parent()
            .context("Missing settings directory")?
            .join("settings.txt"))
    }
    pub fn load() -> Self {
        let result = (|| -> Result<Self> {
            let path = Self::path()?;
            if !path.exists() {
                return Ok(Self::default());
            }
            Self::parse(&std::fs::read_to_string(path)?)
        })();
        result.unwrap_or_else(|error| {
            eprintln!("Settings ignored: {error:#}");
            Self::default()
        })
    }
    pub fn save(self) -> Result<()> {
        self.save_to(&Self::path()?)
    }
    fn save_to(self, path: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(path.parent().context("Missing settings directory")?)?;
        let pending = path.with_extension("txt.part");
        std::fs::write(&pending, format!("auto_close={}\n", self.value()))?;
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
    fn intervals_round_trip_and_invalid_values_are_rejected() {
        for choice in AutoClose::ALL {
            assert_eq!(
                AutoClose::parse(&format!("auto_close={}\n", choice.value())).unwrap(),
                choice
            );
        }
        for value in [
            "",
            "auto_close=0",
            "auto_close=999999",
            "auto_close=5\nextra=true",
        ] {
            assert!(AutoClose::parse(value).is_err());
        }
        assert_eq!(AutoClose::Never.duration(), None);
    }
    #[test]
    fn persisted_settings_replace_previous_choice() -> Result<()> {
        let folder = std::env::temp_dir().join(format!(
            "screenshot settings {} {}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let path = folder.join("settings.txt");
        AutoClose::Never.save_to(&path)?;
        AutoClose::FifteenSeconds.save_to(&path)?;
        assert_eq!(
            AutoClose::parse(&std::fs::read_to_string(&path)?)?,
            AutoClose::FifteenSeconds
        );
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }
}
