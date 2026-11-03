use super::models::{AssignSpeakerResult, MeetingSpeaker, Person, PersonDetail, PersonLine, PersonMeetingHit, PersonSummary, SpeakerSuggestion};
use crate::database::repositories::people::{PeopleError, PeopleRepository};
use crate::state::AppState;
use log::{error as log_error, info as log_info};

fn ui_error(context: &str, e: PeopleError) -> String {
    if matches!(e, PeopleError::Db(_)) {
        log_error!("{}: {}", context, e);
    }
    e.to_string()
}

/// People directory, optionally filtered by a name fragment. Feeds the speaker dropdown.
#[tauri::command]
pub async fn api_list_people(
    state: tauri::State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<PersonSummary>, String> {
    PeopleRepository::list_people(state.db_manager.pool(), query.as_deref())
        .await
        .map_err(|e| ui_error("api_list_people", e))
}

/// Create a person, or return the existing one with the same name (case-insensitive).
#[tauri::command]
pub async fn api_create_person(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<Person, String> {
    let person = PeopleRepository::get_or_create_person(state.db_manager.pool(), &name)
        .await
        .map_err(|e| ui_error("api_create_person", e))?;
    crate::vault::schedule(state.db_manager.pool(), crate::vault::DELAY);
    Ok(person)
}

#[tauri::command]
pub async fn api_rename_person(
    state: tauri::State<'_, AppState>,
    person_id: String,
    name: String,
) -> Result<Person, String> {
    let pool = state.db_manager.pool();
    let person = PeopleRepository::rename_person(pool, &person_id, &name)
        .await
        .map_err(|e| ui_error("api_rename_person", e))?;
    schedule_person(pool, &person_id).await;
    Ok(person)
}

/// Re-exports the meetings where a person is a named speaker (spec F-MD-5).
async fn schedule_person(pool: &sqlx::SqlitePool, person_id: &str) {
    if let Ok(ids) = crate::markdown_export::store::meeting_ids_for_person(pool, person_id).await {
        crate::markdown_export::schedule_many(pool, &ids, crate::markdown_export::CHANGE_DELAY);
    }
}

#[tauri::command]
pub async fn api_delete_person(
    state: tauri::State<'_, AppState>,
    person_id: String,
) -> Result<bool, String> {
    log_info!("api_delete_person called for {}", person_id);
    let pool = state.db_manager.pool();
    let named_in = crate::markdown_export::store::meeting_ids_for_person(pool, &person_id).await.unwrap_or_default();
    let deleted = PeopleRepository::delete_person(pool, &person_id)
        .await
        .map_err(|e| ui_error("api_delete_person", e))?;
    crate::markdown_export::schedule_many(pool, &named_in, crate::markdown_export::CHANGE_DELAY);
    Ok(deleted)
}

/// A person with every meeting they appear in.
#[tauri::command]
pub async fn api_get_person(
    state: tauri::State<'_, AppState>,
    person_id: String,
) -> Result<PersonDetail, String> {
    PeopleRepository::get_person_detail(state.db_manager.pool(), &person_id)
        .await
        .map_err(|e| ui_error("api_get_person", e))
}

/// Save notes about a person (blank clears them).
#[tauri::command]
pub async fn api_update_person_notes(
    state: tauri::State<'_, AppState>,
    person_id: String,
    notes: Option<String>,
) -> Result<Person, String> {
    let person = PeopleRepository::update_person_notes(state.db_manager.pool(), &person_id, notes.as_deref())
        .await
        .map_err(|e| ui_error("api_update_person_notes", e))?;
    crate::vault::schedule(state.db_manager.pool(), crate::vault::DELAY);
    Ok(person)
}

/// What a person said across all meetings, optionally filtered by text.
#[tauri::command]
pub async fn api_get_person_lines(
    state: tauri::State<'_, AppState>,
    person_id: String,
    query: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<PersonLine>, String> {
    let limit = limit.unwrap_or(200).min(1000) as usize;
    PeopleRepository::list_person_lines(state.db_manager.pool(), &person_id, query.as_deref(), limit)
        .await
        .map_err(|e| ui_error("api_get_person_lines", e))
}

/// Who the unnamed speakers of a meeting might be, from voices remembered in other meetings.
/// Suggestions only; nothing is assigned until the user accepts one.
#[tauri::command]
pub async fn api_get_speaker_suggestions(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<SpeakerSuggestion>, String> {
    PeopleRepository::suggest_people_for_meeting(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| ui_error("api_get_speaker_suggestions", e))
}

/// Erase the remembered voice of a person (their name and meetings stay).
#[tauri::command]
pub async fn api_forget_person_voice(
    state: tauri::State<'_, AppState>,
    person_id: String,
) -> Result<u64, String> {
    log_info!("api_forget_person_voice called for {}", person_id);
    PeopleRepository::forget_person_voice(state.db_manager.pool(), &person_id)
        .await
        .map_err(|e| ui_error("api_forget_person_voice", e))
}

/// Fold a duplicate person into another; returns the person who remains.
#[tauri::command]
pub async fn api_merge_people(
    state: tauri::State<'_, AppState>,
    from_id: String,
    into_id: String,
) -> Result<Person, String> {
    log_info!("api_merge_people called: {} -> {}", from_id, into_id);
    let pool = state.db_manager.pool();
    let named_in = crate::markdown_export::store::meeting_ids_for_person(pool, &from_id).await.unwrap_or_default();
    let person = PeopleRepository::merge_people(pool, &from_id, &into_id)
        .await
        .map_err(|e| ui_error("api_merge_people", e))?;
    crate::markdown_export::schedule_many(pool, &named_in, crate::markdown_export::CHANGE_DELAY);
    schedule_person(pool, &into_id).await;
    Ok(person)
}

/// Meetings where someone whose name matches the query spoke (sidebar "search by person").
#[tauri::command]
pub async fn api_search_meetings_by_person(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<Vec<PersonMeetingHit>, String> {
    PeopleRepository::search_meetings_by_person(state.db_manager.pool(), &query)
        .await
        .map_err(|e| ui_error("api_search_meetings_by_person", e))
}

#[tauri::command]
pub async fn api_get_meeting_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    PeopleRepository::get_meeting_speakers(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| ui_error("api_get_meeting_speakers", e))
}

/// Add the next speaker slot ("Speaker 3") to a meeting.
#[tauri::command]
pub async fn api_add_meeting_speaker(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    PeopleRepository::add_speaker(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| ui_error("api_add_meeting_speaker", e))
}

/// Name a speaker. Pass `new_person_name` to create-or-reuse a person by name, or
/// `person_id` to pick someone from the dropdown. Passing neither clears the name.
/// All lines from that speaker follow, because they only store the slot key.
#[tauri::command]
pub async fn api_assign_speaker_person(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    speaker_key: String,
    person_id: Option<String>,
    new_person_name: Option<String>,
) -> Result<AssignSpeakerResult, String> {
    let pool = state.db_manager.pool();

    let typed_name = new_person_name.filter(|n| !n.trim().is_empty());
    let (person, person_id) = match typed_name {
        Some(name) => {
            let person = PeopleRepository::get_or_create_person(pool, &name)
                .await
                .map_err(|e| ui_error("api_assign_speaker_person", e))?;
            let id = person.id.clone();
            (Some(person), Some(id))
        }
        None => (None, person_id),
    };

    let speakers =
        PeopleRepository::set_speaker_person(pool, &meeting_id, &speaker_key, person_id.as_deref())
            .await
            .map_err(|e| ui_error("api_assign_speaker_person", e))?;
    crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::CHANGE_DELAY);

    Ok(AssignSpeakerResult { speakers, person })
}

/// Move one transcript line to another speaker (or `None` to unlabel it).
#[tauri::command]
pub async fn api_set_transcript_speaker(
    state: tauri::State<'_, AppState>,
    transcript_id: String,
    speaker_key: Option<String>,
) -> Result<Vec<MeetingSpeaker>, String> {
    let pool = state.db_manager.pool();
    let speakers = PeopleRepository::set_transcript_speaker(pool, &transcript_id, speaker_key.as_deref())
        .await
        .map_err(|e| ui_error("api_set_transcript_speaker", e))?;
    if let Ok(Some(meeting_id)) = crate::markdown_export::store::meeting_id_for_transcript(pool, &transcript_id).await {
        crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::CHANGE_DELAY);
    }
    Ok(speakers)
}

#[tauri::command]
pub async fn api_merge_meeting_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    from_key: String,
    into_key: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    let pool = state.db_manager.pool();
    let speakers = PeopleRepository::merge_speakers(pool, &meeting_id, &from_key, &into_key)
        .await
        .map_err(|e| ui_error("api_merge_meeting_speakers", e))?;
    crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::CHANGE_DELAY);
    Ok(speakers)
}

#[tauri::command]
pub async fn api_delete_meeting_speaker(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    speaker_key: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    let pool = state.db_manager.pool();
    let speakers = PeopleRepository::delete_speaker(pool, &meeting_id, &speaker_key)
        .await
        .map_err(|e| ui_error("api_delete_meeting_speaker", e))?;
    crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::CHANGE_DELAY);
    Ok(speakers)
}
