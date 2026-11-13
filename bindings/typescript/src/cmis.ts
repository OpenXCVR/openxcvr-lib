import type {
  CmisApplication,
  CmisDecodeOk,
  CmisDecodeResult,
  CmisIdentity,
  CmisLaneFlags,
  CmisLaneStatus,
  CmisModule,
  CmisModuleFlags,
  CmisMonitoring,
  CmisPages,
} from "./types.js";
import { checkPage, hex, joinDateCode, oui } from "./common.js";
import { decodeCmisLowerMemory } from "../generated/cmis_lower_memory.js";
import { decodeCmisPage00hAdministrative } from "../generated/cmis_page00h_administrative.js";
import { decodeCmisPage01hAdvertising } from "../generated/cmis_page01h_advertising.js";
import { decodeCmisPage11hLaneStatus } from "../generated/cmis_page11h_lane_status.js";
import { Sff8024IdentifierLabel } from "../generated/enums/sff8024_identifier.js";
import { Sff8024ConnectorTypeLabel } from "../generated/enums/sff8024_connector_type.js";
import { Sff8024HostElectricalInterfaceLabel } from "../generated/enums/sff8024_host_electrical_interface.js";
import { CmisModuleState } from "../generated/enums/cmis_module_state.js";
import { CmisMediaInterfaceName } from "../generated/enums/cmis_media_interface.js";
import { CmisDataPathStateForRevision } from "../generated/enums/cmis_data_path_state.js";
import { CmisVdmSupportLabel } from "../generated/enums/cmis_vdm_support.js";
import { decodeCmisPage20hVdmDescriptors1To64 } from "../generated/cmis_page20h_vdm_descriptors1_to64.js";
import { decodeCmisPage21hVdmDescriptors65To128 } from "../generated/cmis_page21h_vdm_descriptors65_to128.js";
import { decodeCmisPage22hVdmDescriptors129To192 } from "../generated/cmis_page22h_vdm_descriptors129_to192.js";
import { decodeCmisPage23hVdmDescriptors193To256 } from "../generated/cmis_page23h_vdm_descriptors193_to256.js";
import { decodeCmisPage24hVdmSamples1To64 } from "../generated/cmis_page24h_vdm_samples1_to64.js";
import { decodeCmisPage25hVdmSamples65To128 } from "../generated/cmis_page25h_vdm_samples65_to128.js";
import { decodeCmisPage26hVdmSamples129To192 } from "../generated/cmis_page26h_vdm_samples129_to192.js";
import { decodeCmisPage27hVdmSamples193To256 } from "../generated/cmis_page27h_vdm_samples193_to256.js";
import { decodeCmisPage28hVdmThresholds1To16 } from "../generated/cmis_page28h_vdm_thresholds1_to16.js";
import { decodeCmisPage29hVdmThresholds17To32 } from "../generated/cmis_page29h_vdm_thresholds17_to32.js";
import { decodeCmisPage2AhVdmThresholds33To48 } from "../generated/cmis_page2_ah_vdm_thresholds33_to48.js";
import { decodeCmisPage2BhVdmThresholds49To64 } from "../generated/cmis_page2_bh_vdm_thresholds49_to64.js";
import { decodeCmisPage2ChVdmFlags } from "../generated/cmis_page2_ch_vdm_flags.js";
import { decodeCmisPage2DhVdmMasks } from "../generated/cmis_page2_dh_vdm_masks.js";
import { decodeCmisPage2FhVdmAdvertisement } from "../generated/cmis_page2_fh_vdm_advertisement.js";
import type {
  CmisVdmBlock,
  CmisVdmObservable,
  CmisVdmSample,
  CmisVdmTcEntry,
  CmisVdmThresholdQuad,
} from "./types.js";

const BIAS_MULT: Record<number, number> = { 0: 1, 1: 2, 2: 4 };

// ─── VDM (Pages 20h-2Fh, CMIS § 8.27) ─────────────────────────────────────────
// Mirrors bindings/go/cmis_vdm.go and bindings/python/openxcvr/_cmis.py: group
// gating on the Page 2Fh advertisement, polymorphic per-observable-type X16
// sample decode, threshold quads and TC flag/mask nibbles.
// Pure: decodes caller-supplied page bytes and never performs I/O. The block
// is emitted only from the COMPLETE 15-page VDM set (2Eh is not part of the
// feature); a short page of the supplied set throws (fail closed).

const VDM_PAGE_KEYS = [
  "page20h", "page21h", "page22h", "page23h",
  "page24h", "page25h", "page26h", "page27h",
  "page28h", "page29h", "page2ah", "page2bh",
  "page2ch", "page2dh", "page2fh",
] as const;

function vdmPage(pages: CmisPages, key: (typeof VDM_PAGE_KEYS)[number]): Uint8Array | undefined {
  return pages[key];
}

function vdmPagesComplete(pages: CmisPages): boolean {
  return VDM_PAGE_KEYS.every((key) => vdmPage(pages, key) !== undefined);
}

function hex16(value: number): string {
  return "0x" + value.toString(16).toUpperCase().padStart(4, "0");
}

// variant-table dispatch (page-24h-vdm-samples-1-64.yaml), mirroring
// the Rust decode_sample and the Go/Python facades: S16 for TEC current /
// laser frequency error / laser temperature (0x02-0x04) and ELS input power
// (0x54); F16 (m * 10^(s-24)) for the Pre-FEC BER and FERC families
// (0x09-0x1A); raw U16 otherwise.
//
// F16 conversion goes through correctly-rounded decimal parsing
// (Number("<m>e<s-24>"), the strtod oracle the Rust reference and the C
// binding's openxcvr_cmis_f16_to_double are held to) because V8's
// Math.pow(10, n) is not correctly rounded: 250 * Math.pow(10, -5) is
// 0.0024999999999999996, one ulp off the 0.0025 the reference emits.
function decodeVdmSample(observableType: number, raw: number): CmisVdmSample {
  if ((observableType >= 0x02 && observableType <= 0x04) || observableType === 0x54) {
    return { storage: "i16", value: raw & 0x8000 ? raw - 0x10000 : raw };
  }
  if (observableType >= 0x09 && observableType <= 0x1a) {
    const m = raw & 0x7ff;
    const s = (raw >> 11) & 0x1f;
    return { storage: "f16", value: Number(`${m}e${s - 24}`) };
  }
  return { storage: "u16", value: raw };
}

function decodeVdm(pages: CmisPages): CmisVdmBlock {
  const page2fh = vdmPage(pages, "page2fh")!;
  if (page2fh.length < 128) {
    throw new Error(`VDM page 2Fh missing or short: ${page2fh.length} bytes`);
  }
  const ad = decodeCmisPage2FhVdmAdvertisement(page2fh);

  // CmisVdmSupport code = advertised group count - 1; only advertised
  // groups' descriptor/sample pages are decoded.
  const groups = ad.vdmSupport + 1;

  const observables: CmisVdmObservable[] = [];
  for (let group = 0; group < groups; group++) {
    const descBuf = vdmPage(pages, VDM_PAGE_KEYS[group])!;
    if (descBuf.length < 128) {
      throw new Error(`VDM descriptor page ${(0x20 + group).toString(16).toUpperCase().padStart(2, "0")}h missing or short: ${descBuf.length} bytes`);
    }
    const sampleBuf = vdmPage(pages, VDM_PAGE_KEYS[4 + group])!;
    if (sampleBuf.length < 128) {
      throw new Error(`VDM sample page ${(0x24 + group).toString(16).toUpperCase().padStart(2, "0")}h missing or short: ${sampleBuf.length} bytes`);
    }
    const desc = group === 0
      ? decodeCmisPage20hVdmDescriptors1To64(descBuf)
      : group === 1
        ? decodeCmisPage21hVdmDescriptors65To128(descBuf)
        : group === 2
          ? decodeCmisPage22hVdmDescriptors129To192(descBuf)
          : decodeCmisPage23hVdmDescriptors193To256(descBuf);
    const samples = group === 0
      ? decodeCmisPage24hVdmSamples1To64(sampleBuf)
      : group === 1
        ? decodeCmisPage25hVdmSamples65To128(sampleBuf)
        : group === 2
          ? decodeCmisPage26hVdmSamples129To192(sampleBuf)
          : decodeCmisPage27hVdmSamples193To256(sampleBuf);
    for (let local = 0; local < 64; local++) {
      // An unused descriptor slot is 00h in both bytes (CMIS Table 8-178):
      // observable type 0 means the instance is not active and its sample
      // is ignored.
      if (desc.observableType[local] === 0) {
        continue;
      }
      const raw = samples.vdmSample[local];
      observables.push({
        instance: group * 64 + local + 1,
        group: group + 1,
        threshold_set_id: desc.localThresholdSetId[local],
        global_threshold_set_id: group * 16 + desc.localThresholdSetId[local] + 1,
        monitored_resource: desc.monitoredResource[local],
        observable_type: desc.observableType[local],
        active: true,
        sample_raw: hex16(raw),
        sample: decodeVdmSample(desc.observableType[local], raw),
      });
    }
  }

  const thresholdQuads: CmisVdmThresholdQuad[] = [];
  const thresholdPages: [Uint8Array | undefined, string, typeof decodeCmisPage28hVdmThresholds1To16][] = [
    [vdmPage(pages, "page28h"), "28h", decodeCmisPage28hVdmThresholds1To16],
    [vdmPage(pages, "page29h"), "29h", decodeCmisPage29hVdmThresholds17To32],
    [vdmPage(pages, "page2ah"), "2Ah", decodeCmisPage2AhVdmThresholds33To48],
    [vdmPage(pages, "page2bh"), "2Bh", decodeCmisPage2BhVdmThresholds49To64],
  ];
  for (const [buf, pageTag, decode] of thresholdPages) {
    if (buf === undefined || buf.length < 128) {
      throw new Error(`VDM threshold page ${pageTag} missing or short: ${buf?.length ?? 0} bytes`);
    }
    const quads = decode(buf);
    for (let set = 0; set < 16; set++) {
      const hiA = quads.highAlarmThreshold[set];
      const loA = quads.lowAlarmThreshold[set];
      const hiW = quads.highWarningThreshold[set];
      const loW = quads.lowWarningThreshold[set];
      // An all-zero quad is an undefined threshold set: emitted only when
      // at least one carrier is non-zero.
      if (hiA === 0 && loA === 0 && hiW === 0 && loW === 0) {
        continue;
      }
      thresholdQuads.push({
        page: pageTag,
        set: set + 1,
        high_alarm: hex16(hiA),
        low_alarm: hex16(loA),
        high_warning: hex16(hiW),
        low_warning: hex16(loW),
      });
    }
  }

  const flagsBuf = vdmPage(pages, "page2ch")!;
  if (flagsBuf.length < 128) {
    throw new Error(`VDM page 2Ch missing or short: ${flagsBuf.length} bytes`);
  }
  const masksBuf = vdmPage(pages, "page2dh")!;
  if (masksBuf.length < 128) {
    throw new Error(`VDM page 2Dh missing or short: ${masksBuf.length} bytes`);
  }
  const flags = decodeCmisPage2ChVdmFlags(flagsBuf);
  const masks = decodeCmisPage2DhVdmMasks(masksBuf);

  const nibbleEntries = (
    hiABits: readonly boolean[],
    loABits: readonly boolean[],
    hiWBits: readonly boolean[],
    loWBits: readonly boolean[],
  ): CmisVdmTcEntry[] => {
    const out: CmisVdmTcEntry[] = [];
    for (let i = 0; i < 256; i++) {
      const hiA = hiABits[i];
      const loA = loABits[i];
      const hiW = hiWBits[i];
      const loW = loWBits[i];
      // An all-zero nibble carries no threshold-crossing state.
      if (hiA || loA || hiW || loW) {
        out.push({
          instance: i + 1,
          high_alarm: hiA,
          low_alarm: loA,
          high_warning: hiW,
          low_warning: loW,
        });
      }
    }
    return out;
  };

  const tcFlags = nibbleEntries(
    flags.highAlarmTcFlag, flags.lowAlarmTcFlag,
    flags.highWarningTcFlag, flags.lowWarningTcFlag,
  );
  const tcMasks = nibbleEntries(
    masks.highAlarmTcMask, masks.lowAlarmTcMask,
    masks.highWarningTcMask, masks.lowWarningTcMask,
  );

  return {
    support: {
      vdm_support: ad.vdmSupport,
      vdm_support_name: CmisVdmSupportLabel(ad.vdmSupport),
      groups_advertised: groups,
      duty_cycle_support: ad.dutyCycleSupport,
      power_saving_support: ad.powerSavingSupport,
      fine_interval_length_0p1ms: ad.fineIntervalLength,
      freeze_request: ad.freezeRequest,
      power_saving_mode: ad.powerSavingMode,
      monitoring_duty_cycle_k: ad.monitoringDutyCycle,
      freeze_done: ad.freezeDone,
      unfreeze_done: ad.unfreezeDone,
    },
    observables,
    threshold_quads: thresholdQuads,
    tc_flags: tcFlags,
    tc_masks: tcMasks,
  };
}

export function decodeCmis(pages: CmisPages): CmisDecodeResult {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }

  const lowerBuf = checkPage("lower", pages.lower, true)!;
  const page00hBuf = checkPage("page00h", pages.page00h, true)!;
  const page01hBuf = checkPage("page01h", pages.page01h, false);
  const page11hBuf = checkPage("page11h", pages.page11h, false);

  const lower = decodeCmisLowerMemory(lowerBuf);
  const cmisByte = lower.cmisRevision;
  const cmisMajor = cmisByte >> 4;
  const cmisMinor = cmisByte & 0x0f;

  if (cmisMajor !== 3 && cmisMajor !== 4 && cmisMajor !== 5) {
    const hexByte = cmisByte.toString(16).toUpperCase().padStart(2, "0");
    return {
      decode_status: "rejected",
      rejection: {
        reason: `unknown_major_revision: 0x${hexByte}`,
        detail:
          `Lower memory byte 1 = 0x${hexByte}; ` +
          "CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. " +
          `The fboss test corpus uses 0x${hexByte} as an explicit anomalous-rev regression target.`,
      },
    };
  }

  const p00 = decodeCmisPage00hAdministrative(page00hBuf);
  const p01 = page01hBuf ? decodeCmisPage01hAdvertising(page01hBuf) : undefined;

  const vendorOui = oui(p00.vendorOui);
  const dateCode = joinDateCode(p00.dateCodeYear, p00.dateCodeMonth, p00.dateCodeDayOfMonth);

  const identity: CmisIdentity = {
    date_code: dateCode,
    vendor_name: p00.vendorName,
    vendor_oui: vendorOui,
    vendor_pn: p00.vendorPn,
    vendor_rev: p00.vendorRev,
    vendor_sn: p00.vendorSn,
  };

  const connectorRaw = p00.connectorType;
  const connectorHex = hex(connectorRaw);
  const connectorName = Sff8024ConnectorTypeLabel(connectorRaw);

  const sff8024Id = lower.sff8024Identifier;
  const idHex = hex(sff8024Id);
  const idName = Sff8024IdentifierLabel(sff8024Id);

  const memoryModel: "flat" | "paged" = lower.memoryModel ? "flat" : "paged";

  const rawState = lower.moduleState;
  const stateName = CmisModuleState[rawState];
  const moduleState = stateName !== undefined ? stateName : `Unknown${rawState}`;

  const module: CmisModule = {
    cmis_revision: `${cmisMajor}.${cmisMinor}`,
    connector: {
      name: connectorName,
      raw: connectorHex,
    },
    identifier: {
      name: idName,
      raw: idHex,
    },
    memory_model: memoryModel,
    module_state: moduleState,
    module_type: null,
  };

  const monitoring: CmisMonitoring = {
    module_temperature_c: lower.tempMonValue / 256.0,
    module_voltage_v: lower.vccMonVoltage / 10000,
  };

  let applications: CmisApplication[] | null = null;

  if (!lower.memoryModel) {
    const apps: CmisApplication[] = [];
    const mediaType = lower.mediaType;

    for (let i = 0; i < 8; i++) {
      const hif = lower.hostInterfaceIdApp[i];
      if (hif === 0xff) {
        break;
      }
      const mif = lower.mediaInterfaceIdApp[i];
      if (mif === 0) {
        continue;
      }
      const hlc = lower.hostLaneCountApp[i];
      const mlc = lower.mediaLaneCountApp[i];
      const hlao = lower.hostLaneAssignmentOptionsApp[i];
      const mediaName = CmisMediaInterfaceName(mediaType, mif);

      apps.push({
        app_code: i + 1,
        host_interface_id: hif,
        host_interface_name: Sff8024HostElectricalInterfaceLabel(hif),
        host_lane_assignment_options: hex(hlao),
        host_lane_count: hlc,
        media_interface_id: mif,
        media_interface_name: mediaName !== null ? mediaName : "unknown",
        media_lane_count: mlc,
      });
    }

    if (p01) {
      for (let i = 0; i < 7; i++) {
        const hif = p01.hostInterfaceIdApp[i];
        if (hif === 0xff) {
          break;
        }
        const mif = p01.mediaInterfaceIdApp[i];
        if (mif === 0) {
          continue;
        }
        const hlc = p01.hostLaneCountApp[i];
        const mlc = p01.mediaLaneCountApp[i];
        const hlao = p01.hostLaneAssignmentOptionsApp[i];
        const mediaName = CmisMediaInterfaceName(mediaType, mif);

        apps.push({
          app_code: i + 9,
          host_interface_id: hif,
          host_interface_name: Sff8024HostElectricalInterfaceLabel(hif),
          host_lane_assignment_options: hex(hlao),
          host_lane_count: hlc,
          media_interface_id: mif,
          media_interface_name: mediaName !== null ? mediaName : "unknown",
          media_lane_count: mlc,
        });
      }
    }

    if (apps.length > 0) {
      applications = apps;
    }
  }

  let lanesAndFlags:
    | { lanes: CmisLaneStatus[]; flags: CmisModuleFlags }
    | { lanes: null; flags: null };

  if (page11hBuf && !lower.memoryModel) {
    if (!p01) {
      throw new Error("Page 11h present but Page 01h missing (Tx-bias multiplier)");
    }
    const p11 = decodeCmisPage11hLaneStatus(page11hBuf);

    const mult = BIAS_MULT[p01.txBiasCurrentScalingFactor];
    const projection = CmisDataPathStateForRevision(cmisByte);

    const lanes: CmisLaneStatus[] = [];
    const laneFlags: CmisLaneFlags[] = [];

    for (let i = 0; i < 8; i++) {
      const nibble = p11.dpStateHostLane[i];
      const name = projection !== null ? projection(nibble) : null;
      lanes.push({
        index: i + 1,
        dp_state: name !== null ? name : `Unknown${nibble}`,
        // Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
        // (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87),
        // both from the generated Page 11h decoder.
        rx_los: p11.losFlagRx[i],
        tx_fault: p11.failureFlagTx[i],
        rx_power_mw: p11.opticalPowerRx[i] / 10000,
        tx_bias_ma: mult !== undefined ? (p11.laserBiasTx[i] * 2 * mult) / 1000 : null,
        tx_power_mw: p11.opticalPowerTx[i] / 10000,
      });

      laneFlags.push({
        rx_power_high_alarm: p11.opticalPowerHighAlarmFlagRx[i],
        rx_power_low_alarm: p11.opticalPowerLowAlarmFlagRx[i],
        rx_power_high_warning: p11.opticalPowerHighWarningFlagRx[i],
        rx_power_low_warning: p11.opticalPowerLowWarningFlagRx[i],
        tx_bias_high_alarm: p11.laserBiasHighAlarmFlagTx[i],
        tx_bias_low_alarm: p11.laserBiasLowAlarmFlagTx[i],
        tx_bias_high_warning: p11.laserBiasHighWarningFlagTx[i],
        tx_bias_low_warning: p11.laserBiasLowWarningFlagTx[i],
        tx_power_high_alarm: p11.opticalPowerHighAlarmFlagTx[i],
        tx_power_low_alarm: p11.opticalPowerLowAlarmFlagTx[i],
        tx_power_high_warning: p11.opticalPowerHighWarningFlagTx[i],
        tx_power_low_warning: p11.opticalPowerLowWarningFlagTx[i],
      });
    }

    const flags: CmisModuleFlags = {
      data_not_ready: null,
      interrupt_asserted: !lower.interruptDeasserted,
      temp_high_alarm: lower.tempMonHighAlarmFlag,
      temp_low_alarm: lower.tempMonLowAlarmFlag,
      supply_high_alarm: lower.vccMonHighAlarmFlag,
      supply_low_alarm: lower.vccMonLowAlarmFlag,
      lanes: laneFlags,
    };

    lanesAndFlags = { lanes, flags };
  } else {
    lanesAndFlags = { lanes: null, flags: null };
  }

  const okResult: CmisDecodeOk = {
    decode_status: "ok",
    identity,
    module,
    monitoring,
    applications,
    checksums: null,
    ...lanesAndFlags,
  };

  // The `vdm` block is emitted only when the caller supplied the complete
  // VDM page set (20h-2Fh, 2Eh excluded): a partial set yields no block,
  // never one fabricated from zero-filled missing pages.
  if (vdmPagesComplete(pages)) {
    okResult.vdm = decodeVdm(pages);
  }

  return okResult;
}
