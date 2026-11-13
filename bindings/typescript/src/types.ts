import type { components } from "../generated/openapi/cmis.js";

export type CmisIdentity = components["schemas"]["CmisIdentity"];
export type CmisModule = components["schemas"]["CmisModule"];
export type CmisNamedValue = components["schemas"]["CmisNamedValue"];
export type CmisMonitoring = components["schemas"]["CmisMonitoring"];
export type CmisApplication = components["schemas"]["CmisApplication"];
export type CmisLaneStatus = components["schemas"]["CmisLaneStatus"];
export type CmisModuleFlags = components["schemas"]["CmisModuleFlags"];
export type CmisLaneFlags = components["schemas"]["CmisLaneFlags"];
export type CmisRejection = components["schemas"]["CmisRejection"];

export interface CmisPages {
  lower: Uint8Array;
  page00h: Uint8Array;
  page01h?: Uint8Array;
  page11h?: Uint8Array;
  /* VDM page set (CMIS § 8.27), 2Eh excluded: the `vdm` block is decoded
   * only when every one of these is supplied. */
  page20h?: Uint8Array;
  page21h?: Uint8Array;
  page22h?: Uint8Array;
  page23h?: Uint8Array;
  page24h?: Uint8Array;
  page25h?: Uint8Array;
  page26h?: Uint8Array;
  page27h?: Uint8Array;
  page28h?: Uint8Array;
  page29h?: Uint8Array;
  page2ah?: Uint8Array;
  page2bh?: Uint8Array;
  page2ch?: Uint8Array;
  page2dh?: Uint8Array;
  page2fh?: Uint8Array;
}

// The `vdm` block (Pages 20h-2Fh, CMIS § 8.27) is a fixture-scoped
// extension of the decode surface — like the Go/Python facades, not (yet)
// part of the OpenAPI schema — so these types are hand-authored here.
export interface CmisVdmSample {
  storage: "u16" | "i16" | "f16";
  value: number;
}

export interface CmisVdmObservable {
  instance: number;
  group: number;
  threshold_set_id: number;
  global_threshold_set_id: number;
  monitored_resource: number;
  observable_type: number;
  active: boolean;
  sample_raw: string;
  sample: CmisVdmSample;
}

export interface CmisVdmThresholdQuad {
  page: string;
  set: number;
  high_alarm: string;
  low_alarm: string;
  high_warning: string;
  low_warning: string;
}

export interface CmisVdmTcEntry {
  instance: number;
  high_alarm: boolean;
  low_alarm: boolean;
  high_warning: boolean;
  low_warning: boolean;
}

export interface CmisVdmSupportBlock {
  vdm_support: number;
  vdm_support_name: string;
  groups_advertised: number;
  duty_cycle_support: boolean;
  power_saving_support: boolean;
  fine_interval_length_0p1ms: number;
  freeze_request: boolean;
  power_saving_mode: boolean;
  monitoring_duty_cycle_k: number;
  freeze_done: boolean;
  unfreeze_done: boolean;
}

export interface CmisVdmBlock {
  support: CmisVdmSupportBlock;
  observables: CmisVdmObservable[];
  threshold_quads: CmisVdmThresholdQuad[];
  tc_flags: CmisVdmTcEntry[];
  tc_masks: CmisVdmTcEntry[];
}

// The result union is generated from the OpenAPI schema (design-r3 R1/A4):
// oneOf [CmisDecodeOk, CmisDecodeRejected] discriminated on decode_status.
// openapi-typescript drops the allOf arms' `required` lists (it only marks the
// properties each arm itself declares), so the ok arm's required fields and
// the lanes/flags coupling are re-pinned by intersecting the generated arm
// with the pre-existing refinement. The arms' optional cross-arm fields are
// omitted back out so the exported surface is no weaker than the hand-written
// union it replaces: `result.rejection` on the ok arm and `result.identity`
// on the rejected arm stay compile errors, as before.
export type CmisDecodeOk = Omit<components["schemas"]["CmisDecodeOk"], "rejection"> & {
  identity: CmisIdentity;
  module: CmisModule;
  monitoring: CmisMonitoring;
  applications: CmisApplication[] | null;
  checksums: null;
} & (
  | {
      lanes: CmisLaneStatus[];
      flags: CmisModuleFlags;
    }
  | {
      lanes: null;
      flags: null;
    }
) & {
  /* Present only when the caller supplied the complete VDM page set
   * (20h-2Fh, 2Eh excluded). */
  vdm?: CmisVdmBlock;
};

export type CmisDecodeRejected = Omit<
  components["schemas"]["CmisDecodeRejected"],
  "identity" | "module" | "monitoring" | "applications" | "checksums" | "lanes" | "flags"
> & {
  rejection: CmisRejection;
};

export type CmisDecodeResult = CmisDecodeOk | CmisDecodeRejected;

// SFF results are ok-only (design-r2 A1): direct aliases of the generated
// components, which carry const decode_status "ok" and all-required fields.
// SFF-8472 flags/monitoring/lanes are nullable (DDM gate).
export type Sff8636DecodeResult = components["schemas"]["Sff8636DecodeResult"];
export type Sff8472DecodeResult = components["schemas"]["Sff8472DecodeResult"];
