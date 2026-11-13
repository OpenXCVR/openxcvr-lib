// Type-level tests for the OpenAPI-generated result components (design-r3
// R1/A4). Compiled with no emit by `tsc -p tsconfig.test.json`, which
// `npm test` runs before the node tests; never built into dist/ and never
// executed at runtime. `// @ts-expect-error` pins fail compilation when
// the expectation stops holding.

import { decodeCmis } from "../../index.js";
import type {
  CmisDecodeOk,
  CmisDecodeResult,
  CmisModuleFlags,
  Sff8472DecodeResult,
  Sff8636DecodeResult,
} from "../../index.js";
import type { components } from "../../generated/openapi/cmis.js";

type Expect<T extends true> = T;

const pages = { lower: new Uint8Array(128), page00h: new Uint8Array(128) };
const result: CmisDecodeResult = decodeCmis(pages);

// ─── narrowing on decode_status ──────────────────────────────────────────────

if (result.decode_status === "ok") {
  const identity = result.identity;
  void identity;

  // lanes/flags coupling survives the generated components (design-r3 A4):
  // both populated or both null.
  if (result.lanes !== null) {
    const coupled: CmisModuleFlags = result.flags;
    void coupled;
  } else {
    const coupled: null = result.flags;
    void coupled;
  }

  // @ts-expect-error rejection only exists on the rejected arm
  const rejection = result.rejection;
  void rejection;
}

if (result.decode_status === "rejected") {
  const rejection = result.rejection;
  void rejection;

  // @ts-expect-error identity only exists on the ok arm
  const identity = result.identity;
  void identity;
}

// ─── @ts-expect-error pins (design-r3 R1) ────────────────────────────────────

type OkWithoutIdentity = Omit<CmisDecodeOk, "identity">;
const okWithoutIdentity = {} as OkWithoutIdentity;
// @ts-expect-error the ok arm requires identity
const missingIdentity: CmisDecodeOk = okWithoutIdentity;
void missingIdentity;

type OkFields = Omit<CmisDecodeOk, "decode_status">;
const okFields = {} as OkFields;
// @ts-expect-error decode_status must be the literal "ok", not the component name "CmisDecodeOk"
const wrongStatus: CmisDecodeOk = { ...okFields, decode_status: "CmisDecodeOk" };
void wrongStatus;

// ─── generated-union assignability (design-r3 A4) ────────────────────────────

// decodeCmis's return type is assignable to the generated OpenAPI union.
export type DecodeCmisReturnsGeneratedUnion = Expect<
  ReturnType<typeof decodeCmis> extends components["schemas"]["CmisDecodeResult"] ? true : false
>;

// ─── SFF results are ok-only (design-r2 A1) ─────────────────────────────────

declare const sff8636: Sff8636DecodeResult;
const sff8636Status: "ok" = sff8636.decode_status;
void sff8636Status;
const lanePowers: number[] = sff8636.lanes.map((lane) => lane.rx_power_mw);
void lanePowers;

declare const sff8472: Sff8472DecodeResult;
const sff8472Status: "ok" = sff8472.decode_status;
void sff8472Status;
// DDM gate: monitoring (and flags/lanes) nullable.
const temperatureOrNull: number | null = sff8472.monitoring ? sff8472.monitoring.module_temperature_c : null;
void temperatureOrNull;
