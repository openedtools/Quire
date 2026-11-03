export interface Message {
  id: string;
  content: string;
  timestamp: string;
}

export interface Transcript {
  id: string;
  text: string;
  timestamp: string; // Wall-clock time (e.g., "14:30:05")
  sequence_id?: number;
  chunk_start_time?: number; // Legacy field
  is_partial?: boolean;
  confidence?: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time?: number; // Seconds from recording start (e.g., 125.3)
  audio_end_time?: number;   // Seconds from recording start (e.g., 128.6)
  duration?: number;          // Segment duration in seconds (e.g., 3.3)
  speaker?: string;           // Speaker slot key ("S1", "S2", ...); names come from MeetingSpeaker
}

export interface TranscriptUpdate {
  text: string;
  timestamp: string; // Wall-clock time for reference
  source: string;
  sequence_id: number;
  chunk_start_time: number; // Legacy field
  is_partial: boolean;
  confidence: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time: number; // Seconds from recording start
  audio_end_time: number;   // Seconds from recording start
  duration: number;          // Segment duration in seconds
}

export interface Block {
  id: string;
  type: string;
  content: string;
  color: string;
}

export interface Section {
  title: string;
  blocks: Block[];
}

export interface Summary {
  [key: string]: Section;
}

export interface ApiResponse {
  message: string;
  num_chunks: number;
  data: any[];
}

export interface SummaryResponse {
  status: string;
  summary: Summary;
  raw_summary?: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

// BlockNote-specific types
export type SummaryFormat = 'legacy' | 'markdown' | 'blocknote';

export interface BlockNoteBlock {
  id: string;
  type: string;
  props?: Record<string, any>;
  content?: any[];
  children?: BlockNoteBlock[];
}

export interface SummaryDataResponse {
  markdown?: string;
  summary_json?: BlockNoteBlock[];
  reasoning_stripped?: boolean;
  normalization_fallback?: boolean;
  // Legacy format fields
  MeetingName?: string;
  _section_order?: string[];
  [key: string]: any; // For legacy section data
}

export type MeetingSummary = Summary | SummaryDataResponse;

export type SummaryProcessStatus =
  | 'pending'
  | 'processing'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'error'
  | 'idle';

export interface ProcessTranscriptResponse {
  message: string;
  process_id: string;
}

export interface CancelSummaryResponse {
  cancelled: boolean;
  message: string;
  meeting_id: string;
}

export interface SummaryProcessResponse {
  status: SummaryProcessStatus;
  meetingName: string | null;
  meeting_id: string;
  start: string | null;
  end: string | null;
  data: unknown | null;
  error: string | null;
}

// Pagination types for optimized transcript loading
export interface MeetingMetadata {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  folder_path?: string;
}

export interface PaginatedTranscriptsResponse {
  transcripts: Transcript[];
  total_count: number;
  has_more: boolean;
}

// Transcript segment data for virtualized display
export interface TranscriptSegmentData {
  id: string;
  timestamp: number; // audio_start_time in seconds
  endTime?: number; // audio_end_time in seconds
  text: string;
  confidence?: number;
  speaker?: string; // Speaker slot key ("S1", "S2", ...)
}

// Folders for organizing meetings
export interface Folder {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface FolderSummary {
  id: string;
  name: string;
  meeting_count: number;
}

// People directory and per-meeting speakers
export interface Person {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  notes: string | null;
}

export interface PersonSummary {
  id: string;
  name: string;
  meeting_count: number;
  last_meeting_at: string | null;
}

export interface PersonMeeting {
  meeting_id: string;
  title: string;
  created_at: string;
  folder_id: string | null;
  line_count: number;
  first_line_start: number | null; // seconds from recording start
}

export interface PersonLine {
  transcript_id: string;
  meeting_id: string;
  meeting_title: string;
  meeting_created_at: string;
  text: string;
  audio_start_time: number | null;
}

export interface PersonDetail {
  person: Person;
  meetings: PersonMeeting[];
  voice_meetings: number; // in how many meetings their voice is remembered
}

export interface SpeakerModelStatus {
  downloaded: boolean;
  size_bytes: number;
}

export interface SpeakerSuggestion {
  speaker_key: string;
  person_id: string;
  person_name: string;
  similarity: number; // cosine similarity of the voices
  strength: 'likely' | 'possible';
  meetings: number; // how many meetings the remembered voice comes from
  is_best: boolean; // the one to offer first for this speaker
}

export interface DetectSpeakersResult {
  speakers: MeetingSpeaker[];
  lines_labeled: number;
  lines_total: number;
  suggestions: SpeakerSuggestion[];
}

export interface MeetingSpeaker {
  speaker_key: string; // "S1", "S2", ...
  person_id: string | null;
  person_name: string | null;
  segment_count: number;
}

export interface AssignSpeakerResult {
  speakers: MeetingSpeaker[];
  person: Person | null;
}
