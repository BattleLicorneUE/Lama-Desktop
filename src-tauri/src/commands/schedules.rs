//! Tâches planifiées et rappels.

use crate::*;

#[tauri::command]
pub(crate) fn reminders_list() -> Result<Vec<reminders::Reminder>, String> {
    reminders::list()
}

#[tauri::command]
pub(crate) fn reminders_cancel(id: String) -> Result<bool, String> {
    reminders::cancel(&id)
}

#[tauri::command]
pub(crate) fn schedules_list() -> Result<Vec<schedules::Task>, String> {
    schedules::list()
}

#[tauri::command]
pub(crate) fn schedule_save(task: schedules::Task) -> Result<schedules::Task, String> {
    schedules::save(task)
}

#[tauri::command]
pub(crate) fn schedule_delete(id: String) -> Result<(), String> {
    schedules::delete(&id)
}

#[tauri::command]
pub(crate) fn schedule_done(id: String, next_at: Option<u64>, conv: Option<String>) -> Result<(), String> {
    schedules::done(&id, next_at, conv)
}
