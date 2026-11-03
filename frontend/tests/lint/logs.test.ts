import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { findSensitiveLogs, inScope } from '../../scripts/lint/logs.mjs';

const names = (src: string) => findSensitiveLogs(src).map((h: { names: string[] }) => h.names);

describe('findSensitiveLogs', () => {
  test('finds content, titles, names and folder paths in log arguments', () => {
    expect(names('info!("Saved {}", meeting_title);')).toEqual([['meeting_title']]);
    expect(names('log::debug!("Result: {}", result.text);')).toEqual([['text']]);
    expect(names('perf_debug!("cleaned {}", cleaned_result);')).toEqual([['cleaned_result']]);
    expect(names('log_info!("Opening {folder_path}");')).toEqual([['folder_path']]);
    expect(names('warn!(\n  "Failed for {}: {}",\n  notification.title,\n  notification.body\n);')).toEqual([['body', 'title']]);
  });

  test('paths, which carry meeting titles, are found too', () => {
    expect(names('info!("Copied audio to {}", dest_path.display());')).toEqual([['dest_path', 'display']]);
    expect(names('info!("Wrote {}", metadata_path.display());')).toEqual([['display', 'metadata_path']]);
    expect(names('log_info!("Saving to {file_path}");')).toEqual([['file_path']]);
  });

  test('ids, counts, words inside the message and reviewed calls pass', () => {
    expect(names('info!("Meeting {} saved with {} lines", meeting_id, count);')).toEqual([]);
    expect(names('info!("transcript text title notes saved");')).toEqual([]);
    expect(names('// log-ok: sample count only\ninfo!("Segment of {} samples", segment.samples.len());')).toEqual([]);
    expect(names('info!("Using {} in context", context.len());')).toEqual([]);
  });

  test('test modules are ignored, and brackets inside strings do not end the call early', () => {
    expect(names('fn a() {}\n#[cfg(test)]\nmod tests {\n  fn t() { info!("{}", title); }\n}')).toEqual([]);
    expect(names('info!("x (y", title);')).toEqual([['title']]);
  });

  test('only Rust app source is in scope', () => {
    expect(inScope('frontend/src-tauri/src/api/api.rs')).toBe(true);
    expect(inScope('frontend/src-tauri/build.rs')).toBe(false);
    expect(inScope('frontend/src/app/page.tsx')).toBe(false);
  });
});
