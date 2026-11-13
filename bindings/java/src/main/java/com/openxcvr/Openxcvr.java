package com.openxcvr;

import com.openxcvr.generated.CmisDataPathState;
import com.openxcvr.generated.CmisLowerMemory;
import com.openxcvr.generated.CmisMediaInterface;
import com.openxcvr.generated.CmisPage00hAdministrative;
import com.openxcvr.generated.CmisPage01hAdvertising;
import com.openxcvr.generated.CmisPage11hLaneStatus;
import com.openxcvr.generated.CmisPage20hVdmDescriptors1To64;
import com.openxcvr.generated.CmisPage21hVdmDescriptors65To128;
import com.openxcvr.generated.CmisPage22hVdmDescriptors129To192;
import com.openxcvr.generated.CmisPage23hVdmDescriptors193To256;
import com.openxcvr.generated.CmisPage24hVdmSamples1To64;
import com.openxcvr.generated.CmisPage25hVdmSamples65To128;
import com.openxcvr.generated.CmisPage26hVdmSamples129To192;
import com.openxcvr.generated.CmisPage27hVdmSamples193To256;
import com.openxcvr.generated.CmisPage28hVdmThresholds1To16;
import com.openxcvr.generated.CmisPage29hVdmThresholds17To32;
import com.openxcvr.generated.CmisPage2AhVdmThresholds33To48;
import com.openxcvr.generated.CmisPage2BhVdmThresholds49To64;
import com.openxcvr.generated.CmisPage2ChVdmFlags;
import com.openxcvr.generated.CmisPage2DhVdmMasks;
import com.openxcvr.generated.CmisPage2FhVdmAdvertisement;
import com.openxcvr.generated.CmisVdmSupport;
import com.openxcvr.generated.PageA0hBaseMemory;
import com.openxcvr.generated.PageA2hDiagnosticsBase;
import com.openxcvr.generated.Sff8024ConnectorType;
import com.openxcvr.generated.Sff8024Encoding;
import com.openxcvr.generated.Sff8024HostElectricalInterface;
import com.openxcvr.generated.Sff8024Identifier;
import com.openxcvr.generated.Sff8636DeviceTechnology;
import com.openxcvr.generated.Sff8636LowerMemory;
import com.openxcvr.generated.Sff8636Page00hUpperAdministrative;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * Checked aggregate decoders for CMIS, SFF-8636 and SFF-8472 transceiver
 * memory, producing the shared golden JSON contract as an ordered
 * {@link Map} (built with {@link LinkedHashMap}/{@link ArrayList} so null
 * values are allowed). Field-for-field port of the Go/TypeScript CMIS
 * facades and the Python/Go SFF facades; all bit and offset math goes
 * through the generated page decoders and enum labels.
 *
 * <p>Scope: these decoders reproduce the existing aggregate
 * golden contract. Like Python/Go/PHP, they admit CMIS by major revision
 * and decode flat-union page fields. This is not the revision-qualified
 * facade of, which stays outstanding.
 */
public final class Openxcvr {

    private Openxcvr() {}

    /** Module state raw value → CamelCase name (matches the Python enum names). */
    private static final Map<Integer, String> MODULE_STATE_NAMES;

    static {
        Map<Integer, String> m = new LinkedHashMap<>();
        m.put(0x00, "Reserved0");
        m.put(0x01, "ModuleLowPwr");
        m.put(0x02, "ModulePwrUp");
        m.put(0x03, "ModuleReady");
        m.put(0x04, "ModulePwrDn");
        m.put(0x05, "ModuleFault");
        MODULE_STATE_NAMES = m;
    }

    // ─── CMIS ───────────────────────────────────────────────────────────────

    /**
     * Decodes CMIS lower memory plus Page 00h (required) and Page 01h /
     * Page 11h (optional, nullable) into the golden JSON shape. A module
     * whose lower-memory byte 1 carries an unknown major revision (not
     * 3.x/4.x/5.x) returns the {@code rejected} result instead of throwing.
     *
     * @throws IllegalArgumentException if a non-null page is shorter than 128 bytes
     */
    public static Map<String, Object> decodeCmis(
            byte[] lower, byte[] page00h, byte[] page01h, byte[] page11h) {
        return decodeCmis(lower, page00h, page01h, page11h, null);
    }

    /**
     * As {@link #decodeCmis(byte[], byte[], byte[], byte[])}, plus the VDM
     * page set (CMIS § 8.27): {@code vdmPages} keys pages by tag
     * {@code "20h"}..{@code "2fh"} (2Eh is not part of the feature). The
     * {@code vdm} block is decoded only when every page of the set is
     * supplied — a partial set yields no block rather than one fabricated
     * from zero-filled missing pages — and a short page of the supplied set
     * throws (fail closed).
     *
     * @throws IllegalArgumentException if a non-null page is shorter than 128 bytes
     */
    public static Map<String, Object> decodeCmis(
            byte[] lower,
            byte[] page00h,
            byte[] page01h,
            byte[] page11h,
            Map<String, byte[]> vdmPages) {
        requirePage("lower", lower);
        requirePage("page00h", page00h);
        if (page01h != null) {
            requirePage("page01h", page01h);
        }
        if (page11h != null) {
            requirePage("page11h", page11h);
        }

        // CMIS revision: byte 1, upper nibble = major, lower nibble = minor.
        int cmisByte = lower[1] & 0xFF;
        int cmisMajor = (cmisByte >> 4) & 0xF;
        int cmisMinor = cmisByte & 0xF;

        if (cmisMajor != 3 && cmisMajor != 4 && cmisMajor != 5) {
            String hexByte = hex(cmisByte);
            Map<String, Object> rejection = new LinkedHashMap<>();
            rejection.put("reason", "unknown_major_revision: " + hexByte);
            rejection.put(
                    "detail",
                    "Lower memory byte 1 = "
                            + hexByte
                            + "; CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, "
                            + "5.1, 5.3. The fboss test corpus uses "
                            + hexByte
                            + " as an explicit anomalous-rev regression target.");
            Map<String, Object> out = new LinkedHashMap<>();
            out.put("decode_status", "rejected");
            out.put("rejection", rejection);
            return out;
        }

        CmisLowerMemory low = CmisLowerMemory.decode(lower);
        CmisPage00hAdministrative p00 = CmisPage00hAdministrative.decode(page00h);
        CmisPage01hAdvertising p01 =
                page01h != null ? CmisPage01hAdvertising.decode(page01h) : null;

        // ─── identity ───
        Map<String, Object> identity = new LinkedHashMap<>();
        identity.put(
                "date_code",
                p00.dateCodeYear + p00.dateCodeMonth + p00.dateCodeDayOfMonth);
        identity.put("vendor_name", p00.vendorName);
        identity.put("vendor_oui", oui(p00.vendorOui));
        identity.put("vendor_pn", p00.vendorPn);
        identity.put("vendor_rev", p00.vendorRev);
        identity.put("vendor_sn", p00.vendorSn);

        // ─── module ───
        String moduleState = MODULE_STATE_NAMES.containsKey(low.moduleState)
                ? MODULE_STATE_NAMES.get(low.moduleState)
                : "Unknown" + low.moduleState;

        Map<String, Object> module = new LinkedHashMap<>();
        module.put("cmis_revision", cmisMajor + "." + cmisMinor);
        module.put(
                "connector",
                namedRaw(Sff8024ConnectorType.label(p00.connectorType), p00.connectorType));
        module.put(
                "identifier",
                namedRaw(Sff8024Identifier.label(low.sff8024Identifier), low.sff8024Identifier));
        module.put("memory_model", low.memoryModel ? "flat" : "paged");
        module.put("module_state", moduleState);
        module.put("module_type", null);

        // ─── monitoring ───
        // VccMonVoltage: u16 × 100 µV, rounded to 4 dp to avoid float drift.
        Map<String, Object> monitoring = new LinkedHashMap<>();
        monitoring.put("module_temperature_c", low.tempMonValue / 256.0);
        monitoring.put("module_voltage_v", round4(low.vccMonVoltage * 100e-6));

        // ─── applications (paged modules only; additive app numbering) ───
        List<Object> applications = null;
        if (!low.memoryModel) {
            List<Object> apps = new ArrayList<>();
            appendAppDescriptors(
                    apps,
                    low.hostInterfaceIdApp,
                    low.mediaInterfaceIdApp,
                    low.hostLaneCountApp,
                    low.mediaLaneCountApp,
                    low.hostLaneAssignmentOptionsApp,
                    1,
                    low.mediaType);
            if (p01 != null) {
                appendAppDescriptors(
                        apps,
                        p01.hostInterfaceIdApp,
                        p01.mediaInterfaceIdApp,
                        p01.hostLaneCountApp,
                        p01.mediaLaneCountApp,
                        p01.hostLaneAssignmentOptionsApp,
                        9,
                        low.mediaType);
            }
            if (!apps.isEmpty()) {
                applications = apps;
            }
        }

        // ─── lanes / flags (Page 11h; paged modules that expose it) ───
        // The DPSM name projection is bound once from the module's revision
        // byte before the lane loop; a null projection (revision outside the
        // catalog) or an unnamed raw value renders Unknown<n>.
        Object lanes = null;
        Object flags = null;
        if (page11h != null && !low.memoryModel) {
            if (p01 == null) {
                throw new IllegalArgumentException(
                        "Page 11h present but Page 01h missing (Tx-bias multiplier)");
            }
            CmisPage11hLaneStatus p11 = CmisPage11hLaneStatus.decode(page11h);

            // LaserBiasTx multiplier from Page 01h:160 bits 4-3
            // (CMIS 5.3 Table 8-52); 0b11 is reserved → null tx_bias_ma.
            double mult;
            boolean biasReserved;
            switch (p01.txBiasCurrentScalingFactor) {
                case 0:
                    mult = 1.0;
                    biasReserved = false;
                    break;
                case 1:
                    mult = 2.0;
                    biasReserved = false;
                    break;
                case 2:
                    mult = 4.0;
                    biasReserved = false;
                    break;
                default:
                    mult = 0.0;
                    biasReserved = true;
                    break;
            }

            CmisDataPathState.Projection projection = CmisDataPathState.forRevision(cmisByte);

            List<Object> laneList = new ArrayList<>(8);
            List<Object> laneFlagList = new ArrayList<>(8);
            for (int i = 0; i < 8; i++) {
                int nibble = p11.dpStateHostLane[i];
                String dpState = projection != null ? projection.label(nibble) : null;
                if (dpState == null) {
                    dpState = "Unknown" + nibble;
                }
                Map<String, Object> lane = new LinkedHashMap<>();
                lane.put("index", i + 1);
                lane.put("dp_state", dpState);
                // Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
                // (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87),
                // both from the generated Page 11h decoder.
                lane.put("rx_los", p11.losFlagRx[i]);
                lane.put("tx_fault", p11.failureFlagTx[i]);
                lane.put("rx_power_mw", round4(p11.opticalPowerRx[i] * 0.0001));
                lane.put("tx_bias_ma", biasReserved ? null : round3(p11.laserBiasTx[i] * 0.002 * mult));
                lane.put("tx_power_mw", round4(p11.opticalPowerTx[i] * 0.0001));
                laneList.add(lane);

                Map<String, Object> laneFlags = new LinkedHashMap<>();
                laneFlags.put("rx_power_high_alarm", p11.opticalPowerHighAlarmFlagRx[i]);
                laneFlags.put("rx_power_low_alarm", p11.opticalPowerLowAlarmFlagRx[i]);
                laneFlags.put("rx_power_high_warning", p11.opticalPowerHighWarningFlagRx[i]);
                laneFlags.put("rx_power_low_warning", p11.opticalPowerLowWarningFlagRx[i]);
                laneFlags.put("tx_bias_high_alarm", p11.laserBiasHighAlarmFlagTx[i]);
                laneFlags.put("tx_bias_low_alarm", p11.laserBiasLowAlarmFlagTx[i]);
                laneFlags.put("tx_bias_high_warning", p11.laserBiasHighWarningFlagTx[i]);
                laneFlags.put("tx_bias_low_warning", p11.laserBiasLowWarningFlagTx[i]);
                laneFlags.put("tx_power_high_alarm", p11.opticalPowerHighAlarmFlagTx[i]);
                laneFlags.put("tx_power_low_alarm", p11.opticalPowerLowAlarmFlagTx[i]);
                laneFlags.put("tx_power_high_warning", p11.opticalPowerHighWarningFlagTx[i]);
                laneFlags.put("tx_power_low_warning", p11.opticalPowerLowWarningFlagTx[i]);
                laneFlagList.add(laneFlags);
            }
            lanes = laneList;

            Map<String, Object> flagsMap = new LinkedHashMap<>();
            flagsMap.put("data_not_ready", null);
            flagsMap.put("interrupt_asserted", !low.interruptDeasserted);
            flagsMap.put("temp_high_alarm", low.tempMonHighAlarmFlag);
            flagsMap.put("temp_low_alarm", low.tempMonLowAlarmFlag);
            flagsMap.put("supply_high_alarm", low.vccMonHighAlarmFlag);
            flagsMap.put("supply_low_alarm", low.vccMonLowAlarmFlag);
            flagsMap.put("lanes", laneFlagList);
            flags = flagsMap;
        }

        Map<String, Object> out = new LinkedHashMap<>();
        out.put("decode_status", "ok");
        out.put("identity", identity);
        out.put("module", module);
        out.put("monitoring", monitoring);
        out.put("applications", applications);
        out.put("checksums", null);
        out.put("flags", flags);
        out.put("lanes", lanes);
        if (vdmPages != null && vdmPagesComplete(vdmPages)) {
            out.put("vdm", decodeVdm(vdmPages));
        }
        return out;
    }

    // ─── VDM (Pages 20h-2Fh, CMIS § 8.27) ───────────────────────────────────

    /** Complete VDM page set: descriptors 20h-23h, samples 24h-27h,
     * threshold quads 28h-2Bh, TC flags 2Ch, TC masks 2Dh, advertisement 2Fh
     * (2Eh is not part of the feature). */
    private static final String[] VDM_PAGE_TAGS = {
        "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
        "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
    };

    private static boolean vdmPagesComplete(Map<String, byte[]> vdmPages) {
        for (String tag : VDM_PAGE_TAGS) {
            if (vdmPages.get(tag) == null) {
                return false;
            }
        }
        return true;
    }

    /**
     * Builds the {@code vdm} block from the complete caller-supplied VDM
     * page set, mirroring bindings/go/cmis_vdm.go and
     * bindings/python/openxcvr/_cmis.py: group gating on the Page 2Fh
     * advertisement, polymorphic per-observable-type X16 sample decode
     *, threshold quads and TC flag/mask nibbles. Pure: no
     * I/O; a short page of the supplied set throws (fail closed).
     */
    private static Map<String, Object> decodeVdm(Map<String, byte[]> pages) {
        byte[] page2fh = pages.get("2fh");
        if (page2fh.length < 128) {
            throw new IllegalArgumentException(
                    "VDM page 2Fh missing or short: " + page2fh.length + " bytes");
        }
        CmisPage2FhVdmAdvertisement ad = CmisPage2FhVdmAdvertisement.decode(page2fh);

        // CmisVdmSupport code = advertised group count - 1; only advertised
        // groups' descriptor/sample pages are decoded.
        int groups = ad.vdmSupport + 1;

        List<Object> observables = new ArrayList<>();
        for (int group = 0; group < groups; group++) {
            byte[] descBuf = pages.get(VDM_PAGE_TAGS[group]);
            if (descBuf.length < 128) {
                throw new IllegalArgumentException(
                        String.format(
                                "VDM descriptor page %02Xh missing or short: %d bytes",
                                0x20 + group, descBuf.length));
            }
            byte[] sampleBuf = pages.get(VDM_PAGE_TAGS[4 + group]);
            if (sampleBuf.length < 128) {
                throw new IllegalArgumentException(
                        String.format(
                                "VDM sample page %02Xh missing or short: %d bytes",
                                0x24 + group, sampleBuf.length));
            }
            int[] localThresholdSetId;
            int[] monitoredResource;
            int[] observableType;
            int[] vdmSample;
            if (group == 0) {
                CmisPage20hVdmDescriptors1To64 d =
                        CmisPage20hVdmDescriptors1To64.decode(descBuf);
                localThresholdSetId = d.localThresholdSetId;
                monitoredResource = d.monitoredResource;
                observableType = d.observableType;
                vdmSample = CmisPage24hVdmSamples1To64.decode(sampleBuf).vdmSample;
            } else if (group == 1) {
                CmisPage21hVdmDescriptors65To128 d =
                        CmisPage21hVdmDescriptors65To128.decode(descBuf);
                localThresholdSetId = d.localThresholdSetId;
                monitoredResource = d.monitoredResource;
                observableType = d.observableType;
                vdmSample = CmisPage25hVdmSamples65To128.decode(sampleBuf).vdmSample;
            } else if (group == 2) {
                CmisPage22hVdmDescriptors129To192 d =
                        CmisPage22hVdmDescriptors129To192.decode(descBuf);
                localThresholdSetId = d.localThresholdSetId;
                monitoredResource = d.monitoredResource;
                observableType = d.observableType;
                vdmSample = CmisPage26hVdmSamples129To192.decode(sampleBuf).vdmSample;
            } else {
                CmisPage23hVdmDescriptors193To256 d =
                        CmisPage23hVdmDescriptors193To256.decode(descBuf);
                localThresholdSetId = d.localThresholdSetId;
                monitoredResource = d.monitoredResource;
                observableType = d.observableType;
                vdmSample = CmisPage27hVdmSamples193To256.decode(sampleBuf).vdmSample;
            }
            for (int local = 0; local < 64; local++) {
                // An unused descriptor slot is 00h in both bytes (CMIS Table
                // 8-178): observable type 0 means the instance is not active
                // and its sample is ignored.
                if (observableType[local] == 0) {
                    continue;
                }
                int raw = vdmSample[local];
                Map<String, Object> observable = new LinkedHashMap<>();
                observable.put("instance", group * 64 + local + 1);
                observable.put("group", group + 1);
                observable.put("threshold_set_id", localThresholdSetId[local]);
                observable.put(
                        "global_threshold_set_id",
                        group * 16 + localThresholdSetId[local] + 1);
                observable.put("monitored_resource", monitoredResource[local]);
                observable.put("observable_type", observableType[local]);
                observable.put("active", true);
                observable.put("sample_raw", String.format("0x%04X", raw));
                observable.put("sample", decodeVdmSample(observableType[local], raw));
                observables.add(observable);
            }
        }

        // Supervision threshold quads (Pages 28h-2Bh, global sets 1-64).
        String[] thresholdTags = {"28h", "29h", "2Ah", "2Bh"};
        List<Object> thresholdQuads = new ArrayList<>();
        for (int pageIdx = 0; pageIdx < 4; pageIdx++) {
            String tag = thresholdTags[pageIdx];
            byte[] buf = pages.get(tag.toLowerCase(java.util.Locale.ROOT));
            if (buf.length < 128) {
                throw new IllegalArgumentException(
                        "VDM threshold page " + tag + " missing or short: " + buf.length + " bytes");
            }
            int[] hiAlarm;
            int[] loAlarm;
            int[] hiWarning;
            int[] loWarning;
            if (pageIdx == 0) {
                CmisPage28hVdmThresholds1To16 q = CmisPage28hVdmThresholds1To16.decode(buf);
                hiAlarm = q.highAlarmThreshold;
                loAlarm = q.lowAlarmThreshold;
                hiWarning = q.highWarningThreshold;
                loWarning = q.lowWarningThreshold;
            } else if (pageIdx == 1) {
                CmisPage29hVdmThresholds17To32 q = CmisPage29hVdmThresholds17To32.decode(buf);
                hiAlarm = q.highAlarmThreshold;
                loAlarm = q.lowAlarmThreshold;
                hiWarning = q.highWarningThreshold;
                loWarning = q.lowWarningThreshold;
            } else if (pageIdx == 2) {
                CmisPage2AhVdmThresholds33To48 q = CmisPage2AhVdmThresholds33To48.decode(buf);
                hiAlarm = q.highAlarmThreshold;
                loAlarm = q.lowAlarmThreshold;
                hiWarning = q.highWarningThreshold;
                loWarning = q.lowWarningThreshold;
            } else {
                CmisPage2BhVdmThresholds49To64 q = CmisPage2BhVdmThresholds49To64.decode(buf);
                hiAlarm = q.highAlarmThreshold;
                loAlarm = q.lowAlarmThreshold;
                hiWarning = q.highWarningThreshold;
                loWarning = q.lowWarningThreshold;
            }
            for (int set = 0; set < 16; set++) {
                int hiA = hiAlarm[set];
                int loA = loAlarm[set];
                int hiW = hiWarning[set];
                int loW = loWarning[set];
                // An all-zero quad is an undefined threshold set: emitted
                // only when at least one carrier is non-zero.
                if (hiA == 0 && loA == 0 && hiW == 0 && loW == 0) {
                    continue;
                }
                Map<String, Object> quad = new LinkedHashMap<>();
                quad.put("page", tag);
                quad.put("set", set + 1);
                quad.put("high_alarm", String.format("0x%04X", hiA));
                quad.put("low_alarm", String.format("0x%04X", loA));
                quad.put("high_warning", String.format("0x%04X", hiW));
                quad.put("low_warning", String.format("0x%04X", loW));
                thresholdQuads.add(quad);
            }
        }

        // TC flags and masks (Pages 2Ch / 2Dh, 256 instances).
        byte[] flagsBuf = pages.get("2ch");
        if (flagsBuf.length < 128) {
            throw new IllegalArgumentException(
                    "VDM page 2Ch missing or short: " + flagsBuf.length + " bytes");
        }
        byte[] masksBuf = pages.get("2dh");
        if (masksBuf.length < 128) {
            throw new IllegalArgumentException(
                    "VDM page 2Dh missing or short: " + masksBuf.length + " bytes");
        }
        CmisPage2ChVdmFlags tc = CmisPage2ChVdmFlags.decode(flagsBuf);
        CmisPage2DhVdmMasks masks = CmisPage2DhVdmMasks.decode(masksBuf);

        Map<String, Object> out = new LinkedHashMap<>();
        Map<String, Object> support = new LinkedHashMap<>();
        support.put("vdm_support", ad.vdmSupport);
        support.put("vdm_support_name", CmisVdmSupport.label(ad.vdmSupport));
        support.put("groups_advertised", groups);
        support.put("duty_cycle_support", ad.dutyCycleSupport);
        support.put("power_saving_support", ad.powerSavingSupport);
        support.put("fine_interval_length_0p1ms", ad.fineIntervalLength);
        support.put("freeze_request", ad.freezeRequest);
        support.put("power_saving_mode", ad.powerSavingMode);
        support.put("monitoring_duty_cycle_k", ad.monitoringDutyCycle);
        support.put("freeze_done", ad.freezeDone);
        support.put("unfreeze_done", ad.unfreezeDone);
        out.put("support", support);
        out.put("observables", observables);
        out.put("threshold_quads", thresholdQuads);
        out.put("tc_flags", vdmNibbleEntries(tc.highAlarmTcFlag, tc.lowAlarmTcFlag, tc.highWarningTcFlag, tc.lowWarningTcFlag));
        out.put("tc_masks", vdmNibbleEntries(masks.highAlarmTcMask, masks.lowAlarmTcMask, masks.highWarningTcMask, masks.lowWarningTcMask));
        return out;
    }

    /** Projects 256 per-instance (high_alarm, low_alarm, high_warning,
     * low_warning) bit slices into entries, emitting an instance only when at
     * least one of its four bits is set (an all-zero nibble carries no
     * threshold-crossing state). */
    private static List<Object> vdmNibbleEntries(
            boolean[] hiABits, boolean[] loABits, boolean[] hiWBits, boolean[] loWBits) {
        List<Object> out = new ArrayList<>();
        for (int i = 0; i < 256; i++) {
            boolean hiA = hiABits[i];
            boolean loA = loABits[i];
            boolean hiW = hiWBits[i];
            boolean loW = loWBits[i];
            if (hiA || loA || hiW || loW) {
                Map<String, Object> entry = new LinkedHashMap<>();
                entry.put("instance", i + 1);
                entry.put("high_alarm", hiA);
                entry.put("low_alarm", loA);
                entry.put("high_warning", hiW);
                entry.put("low_warning", loW);
                out.add(entry);
            }
        }
        return out;
    }

    /**
     * Decodes one X16 sample register per the descriptor's observable type
     *: S16 for TEC
     * current / laser frequency error / laser temperature (0x02-0x04) and ELS
     * input power (0x54); F16 (m * 10^(s-24)) for the Pre-FEC BER and FERC
     * families (0x09-0x1A); raw U16 otherwise.
     *
     * <p>The F16 product is converted via correctly-rounded decimal parsing
     * ({@code Double.parseDouble(m + "e" + (s - 24))}), the strtod oracle the
     * Rust reference and the C binding's {@code openxcvr_cmis_f16_to_double}
     * are held to: {@code Math.pow(10, n)} carries no correctly-rounded
     * guarantee.
     */
    private static Map<String, Object> decodeVdmSample(int observableType, int raw) {
        Map<String, Object> sample = new LinkedHashMap<>();
        if ((observableType >= 0x02 && observableType <= 0x04) || observableType == 0x54) {
            sample.put("storage", "i16");
            sample.put("value", (raw & 0x8000) != 0 ? raw - 0x10000 : raw);
        } else if (observableType >= 0x09 && observableType <= 0x1A) {
            int m = raw & 0x7FF;
            int s = (raw >> 11) & 0x1F;
            sample.put("storage", "f16");
            sample.put("value", Double.parseDouble(m + "e" + (s - 24)));
        } else {
            sample.put("storage", "u16");
            sample.put("value", raw);
        }
        return sample;
    }

    // ─── SFF-8636 ───────────────────────────────────────────────────────────

    /**
     * Decodes SFF-8636 lower memory and Page 00h into the shared golden JSON
     * shape.
     *
     * @throws IllegalArgumentException if either page is shorter than 128 bytes
     */
    public static Map<String, Object> decodeSff8636(byte[] lower, byte[] page00h) {
        requirePage("lower", lower);
        requirePage("page00h", page00h);
        Sff8636LowerMemory low = Sff8636LowerMemory.decode(lower);
        Sff8636Page00hUpperAdministrative p00 = Sff8636Page00hUpperAdministrative.decode(page00h);

        Object extSpec = null;
        if (p00.extSpecCompliance != 0) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put(
                    "name",
                    p00.extSpecCompliance == 0x02 ? "base_100g_sr4_or_25g_sr" : "unknown");
            m.put("raw", hex(p00.extSpecCompliance));
            extSpec = m;
        }

        List<Object> bitsSet = new ArrayList<>();
        if (p00.complianceExtended) {
            bitsSet.add("compliance_extended");
        }
        if (p00.compliance40gbaseSr4) {
            bitsSet.add("compliance_40gbase_sr4");
        }

        String moduleType = null;
        if (p00.extSpecCompliance == 0x02) {
            moduleType = "100GBASE-SR4 QSFP28";
        } else if (p00.compliance40gbaseSr4) {
            moduleType = "40GBASE-SR4 QSFP+";
        }

        // SFF-8636 § 6.2: byte 234 == 0xFF is a sentinel for rates > 25.4 GBd;
        // the high-resolution rate is byte 221 (250 MBd units → /4 in GBd).
        double nominalRate =
                p00.nominalSignalingRate == 0xFF
                        ? p00.extBaudRate / 4.0
                        : p00.nominalSignalingRate / 10.0;

        boolean interruptAsserted = !low.interruptDeasserted;
        for (boolean b : low.txLosFlag) {
            interruptAsserted |= b;
        }
        for (boolean b : low.rxLosFlag) {
            interruptAsserted |= b;
        }
        for (boolean b : low.rxPowerAlarmWarningFlag) {
            interruptAsserted |= b;
        }
        for (boolean b : low.txBiasAlarmWarningFlag) {
            interruptAsserted |= b;
        }
        for (boolean b : low.txPowerAlarmWarningFlag) {
            interruptAsserted |= b;
        }

        Map<String, Object> identity = new LinkedHashMap<>();
        identity.put("date_code", p00.dateCodeYear + p00.dateCodeMonth + p00.dateCodeDay);
        identity.put("vendor_name", p00.vendorName);
        identity.put("vendor_oui", oui(p00.vendorOui));
        identity.put("vendor_pn", p00.vendorPn);
        identity.put("vendor_rev", p00.vendorRev);
        identity.put("vendor_sn", p00.vendorSn);

        int extendedIdentifier = p00.powerClassLow
                | (p00.powerClassHigh << 6)
                | (p00.rxCdrPresent ? (1 << 2) : 0)
                | (p00.txCdrPresent ? (1 << 3) : 0)
                | (p00.cleiPresent ? (1 << 4) : 0)
                | (p00.powerClass8Implemented ? (1 << 5) : 0);

        Map<String, Object> module = new LinkedHashMap<>();
        module.put("cmis_revision", null);
        module.put(
                "connector",
                namedRaw(Sff8024ConnectorType.label(p00.connectorType), p00.connectorType));
        module.put("encoding", namedRaw(Sff8024Encoding.label(p00.encoding), p00.encoding));
        module.put("extended_identifier", hex(extendedIdentifier));
        module.put(
                "identifier",
                namedRaw(Sff8024Identifier.label(low.sff8024Identifier), low.sff8024Identifier));
        module.put("memory_model", low.flatMemory ? "flat" : "paged");
        module.put("module_state", null);
        module.put("module_type", moduleType);
        module.put("nominal_bit_rate_gbps", nominalRate);

        Map<String, Object> monitoring = new LinkedHashMap<>();
        monitoring.put("module_temperature_c", low.temperature / 256.0);
        monitoring.put("module_voltage_v", round4(low.supplyVoltage * 0.0001));

        Map<String, Object> deviceTechnology = new LinkedHashMap<>();
        deviceTechnology.put("name", Sff8636DeviceTechnology.label(p00.deviceTechnology));
        deviceTechnology.put("raw", String.format("0x%X", p00.deviceTechnology));

        Map<String, Object> reach = new LinkedHashMap<>();
        reach.put("om1_m", p00.lengthOm1OrCopperAttenuation25p78ghz);
        reach.put("om2_m", p00.lengthOm21m);
        reach.put("om3_2m_units", p00.lengthOm32m * 2);
        reach.put("om4_or_copper_m", p00.lengthCableOrOm4);
        reach.put("smf_km", p00.lengthSmfKm);

        Map<String, Object> compliance = new LinkedHashMap<>();
        compliance.put("bits_set", bitsSet);
        compliance.put("device_technology", deviceTechnology);
        compliance.put("ext_spec_byte_192", extSpec);
        compliance.put("raw_bytes_page_00h_3_10", hexBytes(page00h, 3, 11));
        compliance.put("reach", reach);

        Map<String, Object> flags = new LinkedHashMap<>();
        flags.put("data_not_ready", low.dataNotReady);
        flags.put("interrupt_asserted", interruptAsserted);

        List<Object> laneList = new ArrayList<>(4);
        for (int i = 0; i < 4; i++) {
            Map<String, Object> lane = new LinkedHashMap<>();
            lane.put("index", i + 1);
            lane.put("rx_power_mw", round4(low.rxPowerLane[i] * 0.0001));
            lane.put("tx_bias_ma", round3(low.txBiasLane[i] * 0.002));
            lane.put("tx_power_mw", round4(low.txPowerLane[i] * 0.0001));
            laneList.add(lane);
        }

        Map<String, Object> checksums = new LinkedHashMap<>();
        checksums.put("cc_base", checksum(page00h, 0, 63, 63));
        checksums.put("cc_ext", checksum(page00h, 64, 95, 95));

        Map<String, Object> out = new LinkedHashMap<>();
        out.put("decode_status", "ok");
        out.put("identity", identity);
        out.put("module", module);
        out.put("monitoring", monitoring);
        out.put("compliance", compliance);
        out.put("flags", flags);
        out.put("lanes", laneList);
        out.put("checksums", checksums);
        return out;
    }

    // ─── SFF-8472 ───────────────────────────────────────────────────────────

    /**
     * Decodes SFF-8472 A0h and A2h pages (short inputs are zero-padded to
     * 256 bytes) into the shared golden JSON shape. When byte 92 bit 6
     * (digital diagnostics implemented) is clear, {@code flags},
     * {@code monitoring} and {@code lanes} are null.
     *
     * <p>Telemetry in this result is uncalibrated: SFF-8472 external
     * calibration constants are not applied. Check
     * {@code calibration.externally_calibrated}; calibrated values come
     * from the Rust SFF-8472 adapter or the observation APIs.
     *
     * @throws NullPointerException if either page is null
     */
    public static Map<String, Object> decodeSff8472(byte[] a0h, byte[] a2h) {
        Objects.requireNonNull(a0h, "a0h");
        Objects.requireNonNull(a2h, "a2h");
        byte[] a0 = padPage(a0h, 256);
        byte[] a2 = padPage(a2h, 256);
        PageA0hBaseMemory a0d = PageA0hBaseMemory.decode(a0);
        PageA2hDiagnosticsBase a2d = PageA2hDiagnosticsBase.decode(a2);

        String moduleType =
                a0d.transceiverCompliance.length > 3 && a0d.transceiverCompliance[3]
                        ? "10GBASE-SR SFP+"
                        : null;

        // SFF-8472 v12.5a § 5.6: nominal signaling rate byte 12 == 0xFF is a
        // sentinel for rates > 25.4 GBd; the high-resolution rate is in byte
        // 66 (250 MBd units → divide by 4 to express in GBd).
        double nominalBitRate =
                a0d.signalingRateNominal == 0xFF
                        ? a0d.signalingRateMax / 4.0
                        : a0d.signalingRateNominal / 10.0;

        // SFF-8472 § 8.8: DDM (A2h real-time monitoring) is optional. When
        // byte 92 bit 6 is clear, emit null rather than zero-filled blocks.
        boolean ddm = a0d.digitalDiagnosticsImplemented;

        Map<String, Object> flags = null;
        Map<String, Object> monitoring = null;
        List<Object> laneList = null;
        if (ddm) {
            boolean[] f = a2d.alarmWarningFlag;
            Map<String, Object> alarmSummary = new LinkedHashMap<>();
            alarmSummary.put("rx_power_high_alarm", f[8]);
            alarmSummary.put("rx_power_low_alarm", f[9]);
            alarmSummary.put("temp_high_alarm", f[0]);
            alarmSummary.put("temp_low_alarm", f[1]);
            alarmSummary.put("tx_bias_high_alarm", f[4]);
            alarmSummary.put("tx_bias_low_alarm", f[5]);
            alarmSummary.put("tx_power_high_alarm", f[6]);
            alarmSummary.put("tx_power_low_alarm", f[7]);
            alarmSummary.put("vcc_high_alarm", f[2]);
            alarmSummary.put("vcc_low_alarm", f[3]);

            Map<String, Object> warningSummary = new LinkedHashMap<>();
            warningSummary.put("rx_power_high_warning", f[24]);
            warningSummary.put("rx_power_low_warning", f[25]);
            warningSummary.put("temp_high_warning", f[16]);
            warningSummary.put("temp_low_warning", f[17]);
            warningSummary.put("tx_bias_high_warning", f[20]);
            warningSummary.put("tx_bias_low_warning", f[21]);
            warningSummary.put("tx_power_high_warning", f[22]);
            warningSummary.put("tx_power_low_warning", f[23]);
            warningSummary.put("vcc_high_warning", f[18]);
            warningSummary.put("vcc_low_warning", f[19]);

            flags = new LinkedHashMap<>();
            flags.put("alarm_summary", alarmSummary);
            flags.put("warning_summary", warningSummary);

            monitoring = new LinkedHashMap<>();
            monitoring.put("module_temperature_c", a2d.temperature / 256.0);
            monitoring.put("module_voltage_v", round4(a2d.supplyVoltage * 0.0001));

            Map<String, Object> lane = new LinkedHashMap<>();
            lane.put("index", 1);
            // Status/control bits (A2h byte 110) read from the generated
            // decoded fields, not re-shifted from the raw byte.
            lane.put("rx_los", a2d.rxLosState);
            lane.put("rx_power_mw", round4(a2d.rxPower * 0.0001));
            lane.put("tx_bias_ma", round3(a2d.txBias * 0.002));
            lane.put("tx_disable", a2d.txDisableState);
            lane.put("tx_fault", a2d.txFaultState);
            lane.put("tx_power_mw", round4(a2d.txPower * 0.0001));
            laneList = new ArrayList<>(1);
            laneList.add(lane);
        }

        String dateCode =
                a0d.dateCode.length() >= 6 ? a0d.dateCode.substring(0, 6) : a0d.dateCode;

        Map<String, Object> identity = new LinkedHashMap<>();
        identity.put("date_code", dateCode);
        identity.put("vendor_name", a0d.vendorName);
        identity.put("vendor_oui", oui(a0d.vendorOui));
        identity.put("vendor_pn", a0d.vendorPn);
        identity.put("vendor_sn", a0d.vendorSn);

        Map<String, Object> calibration = new LinkedHashMap<>();
        calibration.put("ddm_implemented", a0d.digitalDiagnosticsImplemented);
        calibration.put("externally_calibrated", a0d.externallyCalibrated);
        calibration.put("internally_calibrated", a0d.internallyCalibrated);
        calibration.put("rx_power_kind", a0d.rxPowerAverage ? "average" : "oma");

        Map<String, Object> module = new LinkedHashMap<>();
        module.put("calibration", calibration);
        module.put("cmis_revision", null);
        module.put(
                "connector",
                namedRaw(Sff8024ConnectorType.label(a0d.connectorType), a0d.connectorType));
        module.put("encoding", namedRaw(Sff8024Encoding.label(a0d.encoding), a0d.encoding));
        module.put(
                "identifier",
                namedRaw(
                        Sff8024Identifier.label(a0d.sff8024Identifier),
                        a0d.sff8024Identifier));
        module.put("memory_model", null);
        module.put("module_state", null);
        module.put("module_type", moduleType);
        module.put("nominal_bit_rate_gbps", nominalBitRate);

        Map<String, Object> reach = new LinkedHashMap<>();
        reach.put("om1_m", a0d.lengthOm110m * 10);
        reach.put("om2_m", a0d.lengthOm210m * 10);
        // Byte 19 is OM3 length in units of 10 m (classic flat byte;
        // base|multiplier<<6 == the raw byte); ×10 yields meters. The
        // schema's base(u6)+multiplier(u2) split is not the wire semantics —
        // finisar (byte19=30, golden=300) falsifies the ×0.1 reading.
        reach.put(
                "om3_m",
                (a0d.lengthOm3Base | (a0d.lengthOm3Multiplier << 6)) * 10);
        reach.put("om4_m", a0d.lengthOm4OrCopper1m);
        reach.put("smf_km", a0d.lengthSmfKm);

        Map<String, Object> compliance = new LinkedHashMap<>();
        compliance.put("raw_bytes_a0h_3_10", hexBytes(a0, 3, 11));
        compliance.put("reach", reach);
        compliance.put("wavelength_nm", a0d.wavelengthOrCableCompliance);

        Map<String, Object> checksums = new LinkedHashMap<>();
        checksums.put("cc_base", checksum(a0, 0, 63, 63));
        checksums.put("cc_dmi", checksum(a2, 0, 95, 95));
        checksums.put("cc_ext", checksum(a0, 64, 95, 95));

        Map<String, Object> out = new LinkedHashMap<>();
        out.put("decode_status", "ok");
        out.put("identity", identity);
        out.put("module", module);
        out.put("monitoring", monitoring);
        out.put("compliance", compliance);
        out.put("flags", flags);
        out.put("lanes", laneList);
        out.put("checksums", checksums);
        return out;
    }

    // ─── shared helpers ─────────────────────────────────────────────────────

    /** Appends application descriptors (lower memory 1-8, Page 01h 9-15). */
    private static void appendAppDescriptors(
            List<Object> apps,
            int[] hostInterfaceId,
            int[] mediaInterfaceId,
            int[] hostLaneCount,
            int[] mediaLaneCount,
            int[] hostLaneAssignmentOptions,
            int baseCode,
            int mediaType) {
        for (int i = 0; i < hostInterfaceId.length; i++) {
            int hif = hostInterfaceId[i];
            if (hif == 0xFF) {
                break;
            }
            int mif = mediaInterfaceId[i];
            if (mif == 0) {
                continue;
            }
            String mediaName = CmisMediaInterface.name(mediaType, mif);
            Map<String, Object> app = new LinkedHashMap<>();
            app.put("app_code", baseCode + i);
            app.put("host_interface_id", hif);
            app.put("host_interface_name", Sff8024HostElectricalInterface.label(hif));
            app.put(
                    "host_lane_assignment_options",
                    hex(hostLaneAssignmentOptions[i]));
            app.put("host_lane_count", hostLaneCount[i]);
            app.put("media_interface_id", mif);
            app.put("media_interface_name", mediaName != null ? mediaName : "unknown");
            app.put("media_lane_count", mediaLaneCount[i]);
            apps.add(app);
        }
    }

    private static void requirePage(String name, byte[] buf) {
        if (buf == null) {
            throw new IllegalArgumentException(name + " is required");
        }
        if (buf.length < 128) {
            throw new IllegalArgumentException(
                    name + " must be at least 128 bytes, got " + buf.length);
        }
    }

    private static byte[] padPage(byte[] buf, int size) {
        if (buf.length >= size) {
            return buf;
        }
        byte[] padded = new byte[size];
        System.arraycopy(buf, 0, padded, 0, buf.length);
        return padded;
    }

    private static Map<String, Object> namedRaw(String name, int raw) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("name", name);
        m.put("raw", hex(raw));
        return m;
    }

    private static String hex(int value) {
        return String.format("0x%02X", value & 0xFF);
    }

    private static String oui(byte[] buf) {
        return String.format("%02X:%02X:%02X", buf[0] & 0xFF, buf[1] & 0xFF, buf[2] & 0xFF);
    }

    private static List<Object> hexBytes(byte[] buf, int from, int to) {
        List<Object> out = new ArrayList<>(to - from);
        for (int i = from; i < to; i++) {
            out.add(hex(buf[i]));
        }
        return out;
    }

    private static Map<String, Object> checksum(byte[] buf, int start, int end, int storedOffset) {
        int computed = 0;
        for (int i = start; i < end; i++) {
            computed = (computed + (buf[i] & 0xFF)) & 0xFF;
        }
        int stored = buf[storedOffset] & 0xFF;
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("computed", hex(computed));
        m.put("stored", hex(stored));
        m.put("valid", computed == stored);
        return m;
    }

    private static double round3(double v) {
        return Math.round(v * 1e3) / 1e3;
    }

    private static double round4(double v) {
        return Math.round(v * 1e4) / 1e4;
    }
}
