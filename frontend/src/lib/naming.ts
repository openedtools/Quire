// The name format, for the preview in Settings (plan PL2). The backend's `summary/naming.rs`
// makes the real names; both follow docs/vault-fixtures/naming.json.

export const DEFAULT_TITLE_FORMAT = '{date} {topic}';
const TITLE_CHARS = 120;
const UNSAFE = /[<>:"/\\|?*\u0000-\u001f\u007f-\u009f]/g;
const EDGES = /^[ \-·,_.]+|[ \-·,_.]+$/g;

export interface TitleFields {
  start: Date;
  topic: string;
  folder: string;
}

const two = (n: number) => String(n).padStart(2, '0');

function fill(format: string, f: TitleFields): string {
  const s = f.start;
  const filled = format
    .split('{date}').join(`${String(s.getFullYear()).padStart(4, '0')}-${two(s.getMonth() + 1)}-${two(s.getDate())}`)
    .split('{time}').join(`${two(s.getHours())}.${two(s.getMinutes())}`)
    .split('{topic}').join(f.topic)
    .split('{folder}').join(f.folder);
  const clean = filled.replace(UNSAFE, '').split(/\s+/).filter(Boolean).join(' ').replace(EDGES, '');
  return Array.from(clean).slice(0, TITLE_CHARS).join('').trimEnd();
}

/** The meeting's name in the person's format. An empty result falls back to the default format. */
export function formatTitle(format: string, fields: TitleFields): string {
  return fill(format, fields) || fill(DEFAULT_TITLE_FORMAT, fields);
}
