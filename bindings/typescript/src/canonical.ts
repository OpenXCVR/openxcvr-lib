// Canonical page-map adapters. Keys are the wire tags shared with the
// Python facade (`lower`, `00h`, `a0h`, …). A missing key is an absent
// page. A present buffer, even if empty, is supplied and is not rewritten
// into absence. An unknown key fails closed.

import type { CmisPages } from "./types.js";
import type { Sff8636Pages } from "./sff8636.js";
import type { Sff8472Pages } from "./sff8472.js";

/** Page tag to bytes. Undefined values are treated as absent keys. */
export type CanonicalPages = Readonly<Record<string, Uint8Array | undefined>>;

const CMIS_REQUIRED = ["lower", "00h"] as const;
const CMIS_OPTIONAL = [
  "01h",
  "11h",
  "20h",
  "21h",
  "22h",
  "23h",
  "24h",
  "25h",
  "26h",
  "27h",
  "28h",
  "29h",
  "2ah",
  "2bh",
  "2ch",
  "2dh",
  "2fh",
] as const;

const CMIS_FIELD: Record<(typeof CMIS_OPTIONAL)[number], keyof CmisPages> = {
  "01h": "page01h",
  "11h": "page11h",
  "20h": "page20h",
  "21h": "page21h",
  "22h": "page22h",
  "23h": "page23h",
  "24h": "page24h",
  "25h": "page25h",
  "26h": "page26h",
  "27h": "page27h",
  "28h": "page28h",
  "29h": "page29h",
  "2ah": "page2ah",
  "2bh": "page2bh",
  "2ch": "page2ch",
  "2dh": "page2dh",
  "2fh": "page2fh",
};

function assertMap(pages: CanonicalPages, family: string): void {
  if (!pages || typeof pages !== "object" || pages instanceof Uint8Array) {
    throw new TypeError(family + " pages must be an object");
  }
}

function present(pages: CanonicalPages, key: string): Uint8Array | undefined {
  const value = pages[key];
  if (value === undefined) {
    return undefined;
  }
  if (!(value instanceof Uint8Array)) {
    throw new TypeError(key + " must be a Uint8Array");
  }
  return value;
}

function requireKey(pages: CanonicalPages, key: string, family: string): Uint8Array {
  if (!Object.prototype.hasOwnProperty.call(pages, key) || pages[key] === undefined) {
    throw new TypeError(family + " pages: missing required key \"" + key + "\"");
  }
  const value = pages[key];
  if (!(value instanceof Uint8Array)) {
    throw new TypeError(key + " must be a Uint8Array");
  }
  return value;
}

function rejectUnknown(pages: CanonicalPages, allowed: ReadonlySet<string>, family: string): void {
  for (const key of Object.keys(pages)) {
    if (!allowed.has(key)) {
      throw new TypeError(family + " pages: unknown key \"" + key + "\"");
    }
  }
}

/** Map canonical CMIS keys onto {@link CmisPages}. */
export function cmisPagesFromCanonical(pages: CanonicalPages): CmisPages {
  assertMap(pages, "cmis");
  const allowed = new Set<string>([...CMIS_REQUIRED, ...CMIS_OPTIONAL]);
  rejectUnknown(pages, allowed, "cmis");
  const out: CmisPages = {
    lower: requireKey(pages, "lower", "cmis"),
    page00h: requireKey(pages, "00h", "cmis"),
  };
  for (const key of CMIS_OPTIONAL) {
    const value = present(pages, key);
    if (value !== undefined) {
      out[CMIS_FIELD[key]] = value;
    }
  }
  return out;
}

/** Map canonical SFF-8636 keys onto {@link Sff8636Pages}. `03h` stays absent when omitted. */
export function sff8636PagesFromCanonical(pages: CanonicalPages): Sff8636Pages {
  assertMap(pages, "sff-8636");
  rejectUnknown(pages, new Set(["lower", "00h", "03h"]), "sff-8636");
  const out: Sff8636Pages = {
    lower: requireKey(pages, "lower", "sff-8636"),
    page00h: requireKey(pages, "00h", "sff-8636"),
  };
  const page03h = present(pages, "03h");
  if (page03h !== undefined) {
    out.page03h = page03h;
  }
  return out;
}

/** Map canonical SFF-8472 keys onto {@link Sff8472Pages}. */
export function sff8472PagesFromCanonical(pages: CanonicalPages): Sff8472Pages {
  assertMap(pages, "sff-8472");
  rejectUnknown(pages, new Set(["a0h", "a2h"]), "sff-8472");
  return {
    a0h: requireKey(pages, "a0h", "sff-8472"),
    a2h: requireKey(pages, "a2h", "sff-8472"),
  };
}
