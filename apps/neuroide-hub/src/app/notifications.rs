use super::HubApp;
use eframe::egui;
use crate::screens::Screen;
use crate::workbench::{ActivityLane, BottomTab, WorkbenchState};
use crate::theme;
use crate::state::ServiceSnapshot;
use neurohid_types::config::UiMode;
use neurohid_types::control::RuntimeModeState;
use std::time::Duration;


impl HubApp {
    pub(crate) fn maybe_notify_latency_transition(&mut self) {
            let snapshot = &self.state.service_snapshot;
            let was_running = self.last_service_running.replace(snapshot.running);
            let was_degraded = self
                .last_latency_degraded
                .replace(snapshot.latency_degraded);

            if !self.state.config.service.notifications_enabled {
                return;
            }

            let (Some(was_running), Some(was_degraded)) = (was_running, was_degraded) else {
                return;
            };

            if !was_running || !snapshot.running || was_degraded == snapshot.latency_degraded {
                return;
            }

            if snapshot.latency_degraded {
                let message = snapshot
                    .latency_alert_message
                    .clone()
                    .unwrap_or_else(|| "Runtime latency exceeded configured thresholds.".to_string());
                self.send_desktop_notification("NeuroHID latency warning", &message);
            } else {
                self.send_desktop_notification(
                    "NeuroHID latency recovered",
                    "Runtime latency returned within configured thresholds.",
                );
            }
        }
    pub(crate) fn maybe_notify_runtime_mode_transition(&mut self) {
            let snapshot = &self.state.service_snapshot;
            let was_running = self.last_runtime_mode_running.replace(snapshot.running);
            let previous_mode = self
                .last_runtime_mode_state
                .replace(snapshot.runtime_mode_state);

            if !self.state.config.service.notifications_enabled {
                return;
            }

            let (Some(was_running), Some(previous_mode)) = (was_running, previous_mode) else {
                return;
            };
            if !was_running || !snapshot.running || previous_mode == snapshot.runtime_mode_state {
                return;
            }

            let (title, fallback_body) = match snapshot.runtime_mode_state {
                RuntimeModeState::Full => (
                    "NeuroHID runtime mode: full",
                    "Runtime recovered to full capability mode.",
                ),
                RuntimeModeState::Fallback => (
                    "NeuroHID runtime mode: fallback",
                    "Runtime entered fallback mode; capabilities may be limited.",
                ),
                RuntimeModeState::Degraded => (
                    "NeuroHID runtime mode: degraded",
                    "Runtime entered degraded mode; HID output may be limited or disabled.",
                ),
            };

            let body = snapshot
                .limited_capabilities_message
                .as_deref()
                .unwrap_or(fallback_body);
            self.send_desktop_notification(title, body);
        }
    pub(crate) fn send_desktop_notification(&self, title: &str, body: &str) {
            if let Err(error) = desktop_notify(title, body) {
                tracing::debug!(
                    title = title,
                    error = %error,
                    "Desktop notification dispatch failed"
                );
            }
        }

}
    pub(crate) fn desktop_notify(title: &str, body: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        desktop_notify_windows(title, body)
    }
    #[cfg(unix)]
    {
        return desktop_notify_unix(title, body);
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (title, body);
        Ok(())
    }
}

#[cfg(unix)]
    pub(crate) fn desktop_notify_unix(title: &str, body: &str) -> std::io::Result<()> {
    let status = std::process::Command::new("notify-send")
        .arg(title)
        .arg(body)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "notify-send exited with status {}",
            status
        )))
    }
}

#[cfg(target_os = "windows")]
    pub(crate) fn desktop_notify_windows(title: &str, body: &str) -> std::io::Result<()> {
    let script = "$ErrorActionPreference='Stop';\
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null;\
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] > $null;\
$title=[System.Security.SecurityElement]::Escape($env:NEUROHID_NOTIFY_TITLE);\
$body=[System.Security.SecurityElement]::Escape($env:NEUROHID_NOTIFY_BODY);\
$xml=\"<toast><visual><binding template='ToastGeneric'><text>$title</text><text>$body</text></binding></visual></toast>\";\
$doc=New-Object Windows.Data.Xml.Dom.XmlDocument;\
$doc.LoadXml($xml);\
$toast=[Windows.UI.Notifications.ToastNotification]::new($doc);\
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('NeuroHID').Show($toast);";

    let status = std::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .env("NEUROHID_NOTIFY_TITLE", title)
        .env("NEUROHID_NOTIFY_BODY", body)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "powershell exited with status {}",
            status
        )))
    }
}

