import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { findCallSites, inScope } from '../../scripts/lint/network.mjs';

const untagged = (src: string) => findCallSites(src).filter((s: { tag: string | null }) => !s.tag).map((s: { line: number }) => s.line);

describe('findCallSites', () => {
  test('finds requests, openers and library downloads', () => {
    expect(untagged('let r = client\n  .get(url)\n  .send()\n  .await?;')).toEqual([3]);
    expect(untagged('reqwest::blocking::get(url)?;')).toEqual([1]);
    expect(untagged('Command::new("xdg-open").arg(&url).spawn();')).toEqual([1]);
    expect(untagged('let p = download_ffmpeg_package(url, &dir)?;')).toEqual([1]);
  });

  test('a tag on the line or up to six lines above covers the call', () => {
    expect(untagged('// net: model download the person starts\nlet r = client.get(url).send().await;')).toEqual([]);
    expect(untagged('let r = client.get(url).send().await; // net: model download')).toEqual([]);
    const far = '// net: too far\n' + 'a;\n'.repeat(7) + 'x.send()';
    expect(untagged(far)).toEqual([9]);
  });

  test('channel sends with arguments, comments and test modules are not call sites', () => {
    expect(untagged('tx.send(value).await;')).toEqual([]);
    expect(untagged('// call .send() later')).toEqual([]);
    expect(untagged('fn a() {}\n#[cfg(test)]\nmod tests {\n  fn t() { c.send() }\n}')).toEqual([]);
  });

  test('the tag text is kept for the inventory', () => {
    const [site] = findCallSites('// net: cloud switch on, Claude selected\nc.post(u).send()');
    expect(site.tag).toBe('cloud switch on, Claude selected');
  });
});

describe('inScope', () => {
  test('covers the Rust core, its build scripts and the helper', () => {
    expect(inScope('frontend/src-tauri/src/summary/llm_client.rs')).toBe(true);
    expect(inScope('frontend/src-tauri/build/ffmpeg.rs')).toBe(true);
    expect(inScope('frontend/src-tauri/build.rs')).toBe(true);
    expect(inScope('llama-helper/src/main.rs')).toBe(true);
    expect(inScope('frontend/src/app/page.tsx')).toBe(false);
  });
});
