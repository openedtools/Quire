// A stand-in for the Tauri core, injected before the page loads, so the UI renders in a
// plain browser for screenshots (plan WP6). Sample data only; nothing is real.
// window.__mock.onboarded = false before load shows the onboarding flow.
(() => {
  const params = new URLSearchParams(location.search);
  const onboarded = !params.has('onboarding');
  const now = '2026-10-30T14:00:00Z';

  const meetings = [
    { id: 'meeting-1', title: 'Weekly planning', folder_id: 'f1', created_at: now, updated_at: now },
    { id: 'meeting-2', title: 'Budget review with finance', folder_id: 'f1', created_at: now, updated_at: now },
    { id: 'meeting-3', title: 'Course design sync', folder_id: null, created_at: now, updated_at: now },
    { id: 'meeting-4', title: 'One to one', folder_id: 'f2', created_at: now, updated_at: now },
  ];
  const folders = [
    { id: 'f1', name: 'Planning', meeting_count: 2 },
    { id: 'f2', name: 'People', meeting_count: 1 },
  ];
  const people = [
    { id: 'p1', name: 'Alex Morgan', meeting_count: 3, last_meeting_at: now },
    { id: 'p2', name: 'Sam Lee', meeting_count: 2, last_meeting_at: now },
    { id: 'p3', name: 'Jordan Patel', meeting_count: 1, last_meeting_at: now },
  ];
  const lines = [
    ['S1', 'Thanks for joining. The goal today is to agree the plan for next month.'],
    ['S2', 'I looked at the numbers. We can cover the first two items without new money.'],
    ['S3', 'The third item needs a decision from the board, so we should write that up.'],
    ['S1', 'Agreed. Sam, can you draft the note by Friday?'],
    ['S2', 'Yes, I will send it round on Thursday so there is time for comments.'],
    ['S4', 'One risk: the venue has not confirmed the dates yet.'],
    ['S3', 'I will call them this afternoon and report back.'],
  ];
  const transcripts = lines.map(([speaker, text], i) => ({
    id: `t${i}`, text, timestamp: `14:0${i}:00`, audio_start_time: i * 9.5, audio_end_time: i * 9.5 + 8,
    speaker, sequence_id: i,
  }));
  const speakers = [
    { speaker_key: 'S1', person_id: 'p1', person_name: 'Alex Morgan', segment_count: 2 },
    { speaker_key: 'S2', person_id: 'p2', person_name: 'Sam Lee', segment_count: 2 },
    { speaker_key: 'S3', person_id: null, person_name: null, segment_count: 2 },
    { speaker_key: 'S4', person_id: null, person_name: null, segment_count: 1 },
  ];
  const summaryMarkdown = '# Weekly planning\n\n## Key points\n\n- The first two items are funded from the current budget.\n- The third item needs a board decision.\n\n## Action items\n\n- Sam drafts the board note by Friday.\n- Jordan confirms the venue dates.\n\n## Decisions\n\n- The plan for next month is agreed in principle.';
  const parakeetModels = [
    { name: 'parakeet-tdt-0.6b-v3-int8', path: '', size_mb: 670, accuracy: 'High', speed: 'Fast', status: 'Available', quantization: 'Int8', description: 'Fast and accurate' },
    { name: 'parakeet-tdt-0.6b-v3-fp32', path: '', size_mb: 2500, accuracy: 'High', speed: 'Medium', status: 'Missing', quantization: 'FP32', description: 'Full precision' },
  ];
  const builtinModels = [
    { name: 'qwen3.5:4b', display_name: 'Qwen 3.5 4B', status: { type: 'available' }, size_mb: 2700, context_size: 32768, description: 'Balanced quality and speed', gguf_file: 'Qwen3.5-4B-Q4_K_M.gguf' },
    { name: 'gemma3:1b', display_name: 'Gemma 3 1B', status: { type: 'not_downloaded' }, size_mb: 1000, context_size: 8192, description: 'Small and fast', gguf_file: 'gemma-3-1b-it-Q8_0.gguf' },
  ];

  const stores = new Map();
  // The first-run notice counts as seen, except with ?firstrun
  if (!params.has('firstrun')) stores.set('preferences.json', new Map([['first_run_notice_acknowledged', true]]));
  // For tests: read a stored value
  window.__storeValue = (path, key) => stores.get(path)?.get(key);
  let nextId = 1;
  const callbacks = new Map();

  // Event listeners, so a test or a handler can fire an app event with window.__emit
  const listeners = [];
  window.__emit = (event, payload) => {
    for (const l of listeners) if (l.event === event) callbacks.get(l.handler)?.({ event, id: l.handler, payload });
  };

  // Notes-first meetings (D5): made by api_create_meeting, with no recording until one is saved
  const empty = new Set();
  const notes = new Map();
  const isEmpty = (id) => empty.has(id);

  // Markdown export (Phase M). Off until a folder is picked and confirmed
  const exportState = { enabled: false, folder: null, cloud_provider: null, last_export_at: null, exported: 0, changed: [] };
  const exportStatus = () => ({ ...exportState, changed: [...exportState.changed] });

  const recordingPrefs = { save_folder: 'C:/Users/me/Documents/Recordings', auto_save: true, file_format: 'mp4', preferred_mic_device: null, preferred_system_device: null, transcript_only: false };
  // Audio retention (Phase B). meeting-1 keeps its audio for 7 days
  const retention = { default_retention: 'd7', detect_on_save: false };
  const inAWeek = new Date(Date.now() + 7 * 864e5).toISOString();
  const audio = { 'meeting-2': { meeting_id: 'meeting-2', retention: 'd7', expires_at: null, deleted_at: null, finalized_at: now, state: 'none', mode: 'transcript_only' }, 'meeting-1': { meeting_id: 'meeting-1', retention: 'd7', expires_at: inAWeek, deleted_at: null, finalized_at: now, state: 'present', mode: 'audio' } };

  const handlers = {
    api_retention_get_settings: () => ({ ...retention }),
    api_retention_set_settings: ({ settings }) => { Object.assign(retention, settings); return { ...retention }; },
    api_retention_status: () => ({ last_sweep_at: now, unmanaged: ['Old recording_2026-10-01_09-00-00_a1b2c3'] }),
    api_retention_apply_to_existing: ({ apply }) => ({ recordings: 3, bytes: 52000000, due_now: apply ? 0 : 1 }),
    api_retention_confirm_due: ({ count }) => ({ deleted: count, pending: 0, kept_outside: 0, finalized_late: 0, deletes_finished: 0, needs_confirmation: 0, clock_went_back: false }),
    api_finalize_recording: () => ({ finalized: true, retention: retention.default_retention, audio_state: 'present', deleted_now: false, kept_because: null }),
    api_meeting_audio: ({ meetingId }) => (audio[meetingId] ? { ...audio[meetingId] } : null),
    api_meeting_keep_audio: ({ meetingId }) => { if (audio[meetingId]) Object.assign(audio[meetingId], { retention: 'keep', expires_at: null }); return true; },
    api_meeting_delete_audio_now: ({ meetingId }) => { if (audio[meetingId]) Object.assign(audio[meetingId], { state: 'deleted', deleted_at: now }); return 'deleted'; },
    api_markdown_export_status: exportStatus,
    api_markdown_export_pick_folder: () => ({ path: '/Users/me/Library/Mobile Documents/iCloud~md~obsidian/Documents/Vault', cloud_provider: 'iCloud Drive' }),
    api_markdown_export_enable: ({ folder }) => {
      Object.assign(exportState, { enabled: true, folder, cloud_provider: /Mobile Documents/.test(folder) ? 'iCloud Drive' : null, last_export_at: now, exported: 3 });
      exportState.changed = [{ meeting_id: 'meeting-2', title: 'Budget review with finance', path: 'Planning/2026-10-30 1400 Budget review with finance.md', state: 'edited' }];
      return exportStatus();
    },
    api_markdown_export_disable: () => { exportState.enabled = false; return exportStatus(); },
    api_markdown_export_all: () => ({ written: 0, unchanged: 3, changed_outside: exportState.changed.length, removed: 0, failed: 0 }),
    api_markdown_export_replace: ({ meetingId }) => {
      exportState.changed = exportState.changed.filter((c) => c.meeting_id !== meetingId);
      return exportStatus();
    },
    get_onboarding_status: () => ({ version: '1', completed: onboarded, current_step: onboarded ? 4 : 1, model_status: { parakeet: onboarded ? 'downloaded' : 'not_downloaded', summary: onboarded ? 'downloaded' : 'not_downloaded', selected_summary_model: 'qwen3.5:4b' }, last_updated: now }),
    check_first_launch: () => !onboarded,
    api_get_meetings: () => meetings,
    api_list_folders: () => folders,
    api_get_meeting: ({ meetingId }) => ({ ...meetings.find((m) => m.id === meetingId) ?? meetings[0], transcripts: isEmpty(meetingId) ? [] : transcripts }),
    api_get_meeting_metadata: ({ meetingId }) => ({ ...meetings.find((m) => m.id === meetingId) ?? meetings[0], folder_path: isEmpty(meetingId) ? null : 'C:/Recordings/Weekly planning' }),
    api_get_meeting_transcripts: ({ meetingId }) => (isEmpty(meetingId)
      ? { transcripts: [], total_count: 0, has_more: false }
      : { transcripts, total_count: transcripts.length, has_more: false }),
    api_get_summary: ({ meetingId }) => (isEmpty(meetingId)
      ? { meeting_id: meetingId, status: 'idle', start: null, end: null, data: null, error: null, meetingName: null }
      : { meeting_id: meetingId, status: 'completed', start: 'a1', end: now, data: { markdown: summaryMarkdown }, error: null, meetingName: 'Weekly planning' }),
    api_create_meeting: ({ title }) => {
      const id = `meeting-new-${nextId++}`;
      meetings.unshift({ id, title, folder_id: null, created_at: now, updated_at: now });
      empty.add(id);
      return id;
    },
    api_discard_empty_meeting: ({ meetingId, defaultTitle }) => {
      const at = meetings.findIndex((m) => m.id === meetingId);
      if (at < 0 || !isEmpty(meetingId) || meetings[at].title !== defaultTitle || (notes.get(meetingId) ?? '').trim()) return false;
      meetings.splice(at, 1);
      return true;
    },
    api_get_meeting_speakers: () => speakers,
    api_get_speaker_suggestions: () => [{ speaker_key: 'S3', person_id: 'p3', person_name: 'Jordan Patel', similarity: 0.82, strength: 'likely', meetings: 2, is_best: true }],
    speaker_model_status: () => ({ downloaded: true, size_bytes: 28000000 }),
    api_get_user_notes: ({ meetingId }) => (meetingId === 'meeting-1' ? { meeting_id: meetingId, body_blocks: JSON.stringify([
      { id: 'n1', type: 'bulletListItem', props: {}, content: [{ type: 'text', text: 'First two items are funded from the current budget', styles: {} }], children: [] },
      { id: 'n2', type: 'bulletListItem', props: {}, content: [{ type: 'text', text: 'Sam drafts the board note by Friday', styles: {} }], children: [] },
    ]), body_markdown: '- First two items are funded from the current budget\n- Sam drafts the board note by Friday', updated_at: now } : null),
    // Ask (D3). ?noaskmodel shows the missing-model state, ?askcloud a provider off this computer
    api_ask_readiness: () => (params.has('noaskmodel')
      ? { providerLabel: 'the built-in model', model: 'qwen3.5:4b', local: true, ready: false, problem: 'Download a model in Settings' }
      : { providerLabel: params.has('askcloud') ? 'OpenAI' : 'the built-in model', model: 'qwen3.5:4b', local: !params.has('askcloud'), ready: true, problem: null }),
    api_ask_transcript: ({ question }) => (/hostile/i.test(question)
      ? { answer: '<img src=x onerror="window.__pwned=1"><script>window.__pwned=1</script> **Bold** [link](https://example.com/x) ![pic](https://example.com/i.png) at [00:09] and [far](#t=9999)', citations: [{ label: '00:09', seconds: 9 }], answeredFrom: 'Answered from the whole transcript' }
      : { answer: 'Sam drafts the board note by Friday [00:28].', citations: [{ label: '00:28', seconds: 28 }], answeredFrom: 'Answered from the whole transcript' }),
    api_cancel_ask: () => false,
    api_save_user_notes: ({ meetingId, bodyBlocks, bodyMarkdown }) => {
      notes.set(meetingId, bodyMarkdown);
      return { meeting_id: meetingId, body_blocks: bodyBlocks, body_markdown: bodyMarkdown, updated_at: now };
    },
    api_list_people: () => people,
    api_get_person: ({ personId }) => ({ person: { id: personId, name: people.find((p) => p.id === personId)?.name ?? 'Alex Morgan', created_at: now, updated_at: now, notes: 'Runs the planning group.' }, meetings: meetings.slice(0, 3).map((m) => ({ meeting_id: m.id, title: m.title, created_at: now, folder_id: m.folder_id, line_count: 4, first_line_start: 0 })), voice_meetings: 2 }),
    api_get_person_lines: () => transcripts.slice(0, 3).map((t) => ({ transcript_id: t.id, meeting_id: 'meeting-1', meeting_title: 'Weekly planning', meeting_created_at: now, text: t.text, audio_start_time: t.audio_start_time })),
    api_get_model_config: () => ({ provider: 'builtin-ai', model: 'qwen3.5:4b', whisperModel: 'large-v3', apiKey: null, ollamaEndpoint: null }),
    api_get_transcript_config: () => ({ provider: 'parakeet', model: 'parakeet-tdt-0.6b-v3-int8', apiKey: null }),
    api_get_api_key: () => '',
    api_get_custom_openai_config: () => null,
    api_get_auto_generate_setting: () => true,
    api_list_templates: () => [{ id: 'standard_meeting', name: 'Standard meeting', description: 'Key points, actions, decisions' }],
    api_get_meeting_summary_language: () => null,
    get_audio_devices: () => [{ name: 'Microphone (USB Audio)', device_type: 'Input' }, { name: 'Speakers (Realtek)', device_type: 'Output' }],
    get_recording_preferences: () => ({ ...recordingPrefs }),
    set_recording_preferences: ({ preferences }) => { Object.assign(recordingPrefs, preferences); return null; },
    get_default_recordings_folder_path: () => 'C:/Users/me/Documents/Recordings',
    get_notification_settings: () => ({ recording_notifications: true, time_based_reminders: true, meeting_reminders: true, respect_do_not_disturb: true, notification_sound: false, system_permission_granted: true, consent_given: true, manual_dnd_mode: false, notification_preferences: { show_recording_started: true, show_recording_stopped: true, show_recording_paused: true, show_recording_resumed: true, show_transcription_complete: true, show_meeting_reminders: true, show_system_errors: true, meeting_reminder_minutes: [5] } }),
    get_database_directory: () => 'C:/Users/me/AppData/Roaming/com.openedtools.notes',
    get_current_audio_backend: () => 'default',
    get_audio_backend_info: () => [],
    is_recording: () => false,
    get_recording_state: () => ({ is_recording: false, is_paused: false }),
    get_transcription_status: () => ({ chunks_in_queue: 0, is_processing: false, last_activity_ms: 0 }),
    has_audio_checkpoints: () => false,
    parakeet_get_available_models: () => parakeetModels,
    parakeet_has_available_models: () => true,
    parakeet_is_model_loaded: () => true,
    parakeet_get_current_model: () => 'parakeet-tdt-0.6b-v3-int8',
    parakeet_validate_model_ready: () => 'parakeet-tdt-0.6b-v3-int8',
    whisper_get_available_models: () => [],
    whisper_has_available_models: () => false,
    builtin_ai_list_models: () => builtinModels,
    builtin_ai_is_model_ready: () => true,
    builtin_ai_get_available_summary_model: () => 'qwen3.5:4b',
    builtin_ai_get_recommended_model: () => 'qwen3.5:4b',
    get_ollama_models: () => [],
    'plugin:app|version': () => '0.4.1',
    'plugin:app|name': () => 'Quire',
    'plugin:event|listen': ({ event, handler }) => { listeners.push({ event, handler }); return handler; },
    start_recording_with_devices_and_meeting: () => { setTimeout(() => window.__emit('recording-started', {}), 50); return null; },
    api_save_transcript: ({ intoMeetingId }) => ({ meeting_id: intoMeetingId ?? 'meeting-saved' }),
    'plugin:event|unlisten': () => null,
    'plugin:event|emit': () => null,
    'plugin:store|load': ({ path }) => { if (!stores.has(path)) stores.set(path, new Map()); const rid = nextId++; stores.set(rid, stores.get(path)); return rid; },
    'plugin:store|get_store': () => null,
    'plugin:store|get': ({ rid, key }) => { const s = stores.get(rid); return s && s.has(key) ? [s.get(key), true] : [null, false]; },
    'plugin:store|set': ({ rid, key, value }) => { stores.get(rid)?.set(key, value); return null; },
    'plugin:store|has': ({ rid, key }) => stores.get(rid)?.has(key) ?? false,
    'plugin:store|save': () => null,
    'plugin:store|keys': ({ rid }) => [...(stores.get(rid)?.keys() ?? [])],
    'plugin:store|entries': ({ rid }) => [...(stores.get(rid)?.entries() ?? [])],
    'plugin:notification|is_permission_granted': () => true,
    'plugin:window|set_theme': () => null,
    'plugin:window|set_title': () => null,
  };

  window.__mockCalls = [];
  window.__mockArgs = [];
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(callback) {
      const id = nextId++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id) { callbacks.delete(id); },
    convertFileSrc(path) { return path; },
    async invoke(cmd, args = {}) {
      window.__mockCalls.push(cmd);
      window.__mockArgs.push([cmd, args]);
      const handler = handlers[cmd];
      if (handler) return handler(args);
      if (cmd.startsWith('plugin:')) return null;
      return null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
})();
