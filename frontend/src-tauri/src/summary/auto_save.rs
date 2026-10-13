//! Auto-save of meeting summaries as Markdown files in a user-chosen folder.

use crate::database::repositories::meeting::MeetingsRepository;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_store::StoreExt;
use tracing::{error, info, warn};

const STORE_FILE: &str = "summary_auto_save.json";
const STORE_KEY: &str = "preferences";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SummaryAutoSavePreferences {
    pub enabled: bool,
    pub folder: PathBuf,
}

impl Default for SummaryAutoSavePreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            folder: dirs::document_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Meetily"),
        }
    }
}

fn load_preferences<R: Runtime>(app: &AppHandle<R>) -> SummaryAutoSavePreferences {
    let stored = app
        .store(STORE_FILE)
        .ok()
        .and_then(|store| store.get(STORE_KEY))
        .and_then(|value| serde_json::from_value(value).ok());
    stored.unwrap_or_default()
}

/// Writes the meeting's summary file if auto-save is enabled. Failures are only logged:
/// the summary is already saved in the database.
pub async fn auto_save_summary<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    markdown: &str,
) {
    let preferences = load_preferences(app);
    if !preferences.enabled {
        return;
    }
    let meeting = match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
        Ok(Some(meeting)) => meeting,
        Ok(None) => {
            warn!("Summary auto-save skipped: meeting {} not found", meeting_id);
            return;
        }
        Err(e) => {
            error!("Summary auto-save failed to load meeting {}: {}", meeting_id, e);
            return;
        }
    };
    let created_at = meeting.created_at.0.with_timezone(&Local);
    match write_summary_file(&preferences.folder, &meeting.title, created_at, markdown) {
        Ok(path) => info!("Summary auto-saved to {:?}", path),
        Err(e) => error!(
            "Summary auto-save failed for meeting {} in {:?}: {}",
            meeting_id, preferences.folder, e
        ),
    }
}

#[tauri::command]
pub async fn get_summary_auto_save_preferences<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SummaryAutoSavePreferences, String> {
    Ok(load_preferences(&app))
}

#[tauri::command]
pub async fn set_summary_auto_save_preferences<R: Runtime>(
    app: AppHandle<R>,
    preferences: SummaryAutoSavePreferences,
) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| format!("Failed to access store: {}", e))?;
    let value = serde_json::to_value(&preferences)
        .map_err(|e| format!("Failed to serialize preferences: {}", e))?;
    store.set(STORE_KEY, value);
    store
        .save()
        .map_err(|e| format!("Failed to save store to disk: {}", e))
}

#[tauri::command]
pub async fn select_summary_auto_save_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    let folder = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
        .await
        .map_err(|e| format!("Folder dialog failed: {}", e))?;
    Ok(folder.map(|path| path.to_string()))
}

#[tauri::command]
pub async fn open_summary_auto_save_folder<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let folder = load_preferences(&app).folder;
    std::fs::create_dir_all(&folder).map_err(|e| format!("Failed to create folder: {}", e))?;

    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opener = "xdg-open";

    std::process::Command::new(opener)
        .arg(&folder)
        .spawn()
        .map_err(|e| format!("Failed to open folder: {}", e))?;
    Ok(())
}

/// File name for a meeting summary: `YYYY-MM-DD - Title.md`.
pub fn summary_file_name(created_at: DateTime<Local>, title: &str) -> String {
    let safe_title: String = title
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let safe_title = if safe_title.is_empty() { "Meeting" } else { &safe_title };
    format!("{} - {}.md", created_at.format("%Y-%m-%d"), safe_title)
}

/// File content: title, date, then the summary markdown.
pub fn summary_file_content(title: &str, created_at: DateTime<Local>, markdown: &str) -> String {
    format!(
        "# {}\n\n**Date:** {}\n\n---\n\n{}\n",
        title.trim(),
        created_at.format("%Y-%m-%d %H:%M"),
        markdown.trim_end()
    )
}

/// Writes the summary file in `folder`, creating the folder and overwriting any previous file.
pub fn write_summary_file(
    folder: &Path,
    title: &str,
    created_at: DateTime<Local>,
    markdown: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(summary_file_name(created_at, title));
    std::fs::write(&path, summary_file_content(title, created_at, markdown))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn date() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 13, 14, 30, 0).unwrap()
    }

    #[test]
    fn file_name_starts_with_date_and_title() {
        assert_eq!(summary_file_name(date(), "Weekly sync"), "2026-10-13 - Weekly sync.md");
    }

    #[test]
    fn file_name_replaces_characters_forbidden_in_paths() {
        assert_eq!(
            summary_file_name(date(), "Q3/Q4: plan?"),
            "2026-10-13 - Q3-Q4- plan-.md"
        );
    }

    #[test]
    fn file_name_falls_back_when_title_is_blank() {
        assert_eq!(summary_file_name(date(), "  "), "2026-10-13 - Meeting.md");
    }

    #[test]
    fn content_has_title_date_and_summary() {
        assert_eq!(
            summary_file_content("Weekly sync", date(), "## Summary\n\nAll good."),
            "# Weekly sync\n\n**Date:** 2026-10-13 14:30\n\n---\n\n## Summary\n\nAll good.\n"
        );
    }

    #[test]
    fn write_creates_folder_and_overwrites_previous_version() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("summaries");

        write_summary_file(&folder, "Weekly sync", date(), "first").unwrap();
        let path = write_summary_file(&folder, "Weekly sync", date(), "second").unwrap();

        assert_eq!(path, folder.join("2026-10-13 - Weekly sync.md"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.ends_with("second\n"));
        assert!(!content.contains("first"));
    }
}
