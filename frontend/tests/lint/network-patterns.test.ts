import { describe, expect, test } from 'bun:test';
// @ts-ignore plain ES module without types
import { findCallSites } from '../../scripts/lint/network.mjs';

const untagged = (src: string) => findCallSites(src).filter((s: { tag: string | null }) => !s.tag).length;

describe('findCallSites, more kinds of call', () => {
  test('shells, other HTTP clients, raw connections and opener crates count', () => {
    expect(untagged('Command::new("osascript").arg(x).spawn();')).toBe(1);
    expect(untagged('Command::new("cmd").args(["/C", "start", u]);')).toBe(1);
    expect(untagged('Command::new("rundll32").arg(u);')).toBe(1);
    expect(untagged('let r = ureq::get(url).call()?;')).toBe(1);
    expect(untagged('let s = TcpStream::connect(addr)?;')).toBe(1);
    expect(untagged('open::that(url)?;')).toBe(1);
  });
});
