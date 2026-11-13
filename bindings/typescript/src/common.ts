// Shared decode helpers for the aggregate page decoders (CMIS, SFF-8636,
// SFF-8472). The page checks, hex/OUI/date helpers and the rejection hex
// formatting were extracted from src/cmis.ts (whose behavior is unchanged);
// the padded page check, nibble/byte-array hex and the checksum block are the
// SFF additions, mirroring bindings/python/openxcvr/_common.py.

export function checkPage(name: string, buf: unknown, required: boolean): Uint8Array | undefined {
  if (buf === undefined || buf === null) {
    if (required) {
      throw new TypeError(`${name} is required`);
    }
    return undefined;
  }
  if (!(buf instanceof Uint8Array)) {
    throw new TypeError(`${name} must be a Uint8Array`);
  }
  if (buf.length < 128) {
    throw new RangeError(`${name} must be at least 128 bytes, got ${buf.length}`);
  }
  return buf.subarray(0, 128);
}

// 256-byte page variant for SFF-8472 A0h/A2h: shorter inputs are zero-padded
// (mirroring _pad_page in bindings/python/openxcvr/_common.py) rather than
// truncated, because the page decoders read offsets up to 255. Longer inputs
// pass through unchanged, like _pad_page.
export function checkPagePadded(name: string, buf: unknown, size = 256): Uint8Array {
  if (buf === undefined || buf === null) {
    throw new TypeError(`${name} is required`);
  }
  if (!(buf instanceof Uint8Array)) {
    throw new TypeError(`${name} must be a Uint8Array`);
  }
  if (buf.length >= size) {
    return buf;
  }
  const padded = new Uint8Array(size);
  padded.set(buf);
  return padded;
}

// "0x" + two uppercase hex digits (_hex in _common.py).
export function hex(value: number): string {
  return "0x" + value.toString(16).toUpperCase().padStart(2, "0");
}

// "0x" + one uppercase hex digit (_hex_nibble in _common.py).
export function hexNibble(value: number): string {
  return "0x" + value.toString(16).toUpperCase();
}

export function hexBytes(buf: Uint8Array): string[] {
  return Array.from(buf, hex);
}

// OUI as colon-separated uppercase hex octets (_oui in _common.py).
export function oui(buf: Uint8Array): string {
  return Array.from(buf)
    .map((b) => b.toString(16).toUpperCase().padStart(2, "0"))
    .join(":");
}

export function joinDateCode(year: string, month: string, day: string): string {
  return year + month + day;
}

// Decimal rounding mirroring Python's round(x, n) on the golden value set:
// scale, round, divide back. The scaled products are never exact binary
// ties, so half-up, half-even and half-away-from-zero all agree here (Go's
// math.Round, proven against the same goldens, does the same).
export function roundTo(value: number, decimals: number): number {
  const scale = 10 ** decimals;
  return Math.round(value * scale) / scale;
}

// One EEPROM checksum: computed sum of buf[start:end] (end exclusive) vs the
// stored byte at storedOffset (_checksum in _common.py).
export function checksumBlock(
  buf: Uint8Array,
  start: number,
  end: number,
  storedOffset: number
): { computed: string; stored: string; valid: boolean } {
  let sum = 0;
  for (let i = start; i < end; i++) {
    sum += buf[i];
  }
  const computed = sum & 0xff;
  const stored = buf[storedOffset];
  return {
    computed: hex(computed),
    stored: hex(stored),
    valid: computed === stored,
  };
}
