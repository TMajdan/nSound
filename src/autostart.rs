//! "Start with Windows" support.
//!
//! Implemented through the per-user `Run` registry key, which needs no
//! administrator rights and is what the Windows "Startup apps" page shows.

#[cfg(target_os = "windows")]
mod imp {
    use std::process::Command;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "nSound";

    fn exe_path() -> Result<String, String> {
        std::env::current_exe()
            .map(|path| path.display().to_string())
            .map_err(|err| format!("cannot resolve the executable path: {err}"))
    }

    /// Whether nSound is registered to start with Windows.
    pub fn is_enabled() -> bool {
        Command::new("reg")
            .args(["query", RUN_KEY, "/v", VALUE_NAME])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Registers (or removes) nSound in the per-user Run key.
    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let output = if enabled {
            let exe = exe_path()?;
            Command::new("reg")
                .args([
                    "add",
                    RUN_KEY,
                    "/v",
                    VALUE_NAME,
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{exe}\""),
                    "/f",
                ])
                .output()
        } else {
            Command::new("reg")
                .args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
                .output()
        };

        match output {
            Ok(result) if result.status.success() => Ok(()),
            Ok(result) => Err(String::from_utf8_lossy(&result.stderr).trim().to_string()),
            Err(err) => Err(format!("cannot run reg.exe: {err}")),
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enabled: bool) -> Result<(), String> {
        Err("autostart is only implemented on Windows".to_string())
    }
}

pub use imp::{is_enabled, set_enabled};
