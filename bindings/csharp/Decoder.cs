// Checked CMIS / SFF-8636 / SFF-8472 aggregate decoders for the C# binding.
//
// Field-for-field ports of the reference implementations (bindings/go/cmis.go,
// bindings/typescript/src/cmis.ts, bindings/python/openxcvr/_sff8636.py,
// _sff8472.py, _common.py, bindings/go/sff8636.go, sff8472.go) over the
// generated page decoders, the generated revision-bound DPSM name projection
// (CmisDataPathStateProjections) and the generated media-interface resolver
// (CmisMediaInterface). Results serialize 1:1 to the golden JSON contract.
//
// Scope: like the Python/Go/PHP/TS/C aggregate decoders, this
// admits CMIS by major revision and decodes flat-union page fields. It is NOT
// the revision-qualified facade of, which stays outstanding.
//
// SFF-8472 calibration: telemetry in this result is uncalibrated: SFF-8472
// external calibration constants are not applied. Check
// calibration.externally_calibrated; calibrated values come from the Rust
// SFF-8472 adapter or the observation APIs.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using OpenXcvr.Generated;

namespace OpenXcvr
{
    /// <summary>
    /// Checked aggregate decoders returning JSON-shaped trees
    /// (<see cref="Dictionary{TKey, TValue}"/> of string to object?, with
    /// <see cref="List{T}"/> for arrays) that serialize 1:1 to the golden
    /// contract. Input-shape errors throw <see cref="ArgumentException"/>:
    /// a null page on every family, and a page shorter than the family's
    /// 128-byte minimum on CMIS and SFF-8636, as the TS CMIS page checks and
    /// the Python KeyError paths do. SFF-8472 pages of any length are
    /// zero-padded to 256 instead (see <see cref="DecodeSff8472"/>).
    /// </summary>
    public static class Decoder
    {
        // ─── shared helpers (ports of _common.py) ───────────────────────────

        private const string ExtSpec100gSr4Or25gSr = "base_100g_sr4_or_25g_sr";

        private static byte[] CheckPage(string name, byte[]? buf, bool required, int size = 128)
        {
            if (buf is null)
            {
                if (required)
                {
                    throw new ArgumentException($"{name} is required");
                }
                throw new ArgumentException($"{name} must not be null");
            }
            if (buf.Length < size)
            {
                throw new ArgumentException($"{name} must be at least {size} bytes, got {buf.Length}");
            }
            if (buf.Length == size)
            {
                return buf;
            }
            var truncated = new byte[size];
            Array.Copy(buf, truncated, size);
            return truncated;
        }

        internal static byte[] PadPage(byte[] buf, int size)
        {
            if (buf.Length >= size)
            {
                return buf;
            }
            var padded = new byte[size];
            Array.Copy(buf, padded, buf.Length);
            return padded;
        }

        private static string Hex(int value) => $"0x{value:X2}";

        private static string HexNibble(int value) => $"0x{value:X}";

        private static List<object?> HexBytes(byte[] buf, int start, int end)
        {
            var result = new List<object?>(end - start);
            for (var i = start; i < end; i++)
            {
                result.Add(Hex(buf[i]));
            }
            return result;
        }

        private static string Oui(byte[] buf) =>
            $"{buf[0]:X2}:{buf[1]:X2}:{buf[2]:X2}";

        /// <summary>Sums buf[start..end) (end exclusive) and compares to buf[storedOffset].</summary>
        private static Dictionary<string, object?> Checksum(byte[] buf, int start, int end, int storedOffset)
        {
            var computed = 0;
            for (var i = start; i < end; i++)
            {
                computed += buf[i];
            }
            computed &= 0xFF;
            var stored = buf[storedOffset];
            return new Dictionary<string, object?>
            {
                ["computed"] = Hex(computed),
                ["stored"] = Hex(stored),
                ["valid"] = computed == stored,
            };
        }

        // ─── CMIS ───────────────────────────────────────────────────────────

        /// <summary>
        /// Decodes a CMIS module's EEPROM pages into the golden JSON shape.
        /// <paramref name="page01h"/> and <paramref name="page11h"/> are
        /// optional; Page 11h present without Page 01h is an input-shape error
        /// (the Tx-bias multiplier lives in Page 01h:160).
        /// </summary>
        public static Dictionary<string, object?> DecodeCmis(
            byte[] lower,
            byte[] page00h,
            byte[]? page01h = null,
            byte[]? page11h = null,
            Dictionary<string, byte[]>? vdmPages = null)
        {
            var lowerBuf = CheckPage("lower", lower, required: true);
            var page00hBuf = CheckPage("page00h", page00h, required: true);
            var page01hBuf = page01h is null ? null : CheckPage("page01h", page01h, required: false);
            var page11hBuf = page11h is null ? null : CheckPage("page11h", page11h, required: false);

            // CMIS revision: byte 1, upper nibble = major, lower nibble = minor.
            var cmisByte = lowerBuf[1];
            var cmisMajor = (cmisByte >> 4) & 0x0F;
            var cmisMinor = cmisByte & 0x0F;

            if (cmisMajor != 3 && cmisMajor != 4 && cmisMajor != 5)
            {
                var hexByte = $"0x{cmisByte:X2}";
                return new Dictionary<string, object?>
                {
                    ["decode_status"] = "rejected",
                    ["rejection"] = new Dictionary<string, object?>
                    {
                        ["detail"] =
                            $"Lower memory byte 1 = {hexByte}; " +
                            "CMIS 5.3 \u00A7 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. " +
                            $"The fboss test corpus uses {hexByte} as an explicit anomalous-rev regression target.",
                        ["reason"] = $"unknown_major_revision: {hexByte}",
                    },
                };
            }

            var lowerRec = CmisLowerMemory.Decode(lowerBuf);
            var p00 = CmisPage00hAdministrative.Decode(page00hBuf);

            // ─── identity ───────────────────────────────────────────────────
            var identity = new Dictionary<string, object?>
            {
                ["date_code"] = p00.DateCodeYear + p00.DateCodeMonth + p00.DateCodeDayOfMonth,
                ["vendor_name"] = p00.VendorName,
                ["vendor_oui"] = Oui(p00.VendorOui),
                ["vendor_pn"] = p00.VendorPn,
                ["vendor_rev"] = p00.VendorRev,
                ["vendor_sn"] = p00.VendorSn,
            };

            // ─── module ─────────────────────────────────────────────────────
            var moduleStateName =
                Enum.GetName(typeof(CmisModuleState), lowerRec.ModuleState) ??
                $"Unknown{lowerRec.ModuleState}";

            var module = new Dictionary<string, object?>
            {
                ["cmis_revision"] = $"{cmisMajor}.{cmisMinor}",
                ["connector"] = new Dictionary<string, object?>
                {
                    ["name"] = Sff8024ConnectorTypeLabels.Label(p00.ConnectorType),
                    ["raw"] = Hex(p00.ConnectorType),
                },
                ["identifier"] = new Dictionary<string, object?>
                {
                    ["name"] = Sff8024IdentifierLabels.Label(lowerRec.Sff8024Identifier),
                    ["raw"] = Hex(lowerRec.Sff8024Identifier),
                },
                ["memory_model"] = lowerRec.MemoryModel ? "flat" : "paged",
                ["module_state"] = moduleStateName,
                ["module_type"] = null,
            };

            // ─── monitoring ─────────────────────────────────────────────────
            // TempMonValue: i16be s16 1/256 °C; VccMonVoltage: u16be ×100 µV.
            var monitoring = new Dictionary<string, object?>
            {
                ["module_temperature_c"] = lowerRec.TempMonValue / 256.0,
                ["module_voltage_v"] = lowerRec.VccMonVoltage * 100.0 / 1_000_000.0,
            };

            // ─── applications (paged modules only) ──────────────────────────
            // Lower Memory bytes 86-117 are AppSel 1-8; Page 01h bytes 223-250
            // (page-buffer 95-122) are AppSel 9-15 (additive numbering). A HIF
            // of 0xFF terminates; a MIF of 0 is skipped.
            object? applications = null;
            if (!lowerRec.MemoryModel)
            {
                var apps = new List<object?>();
                AppendApplications(
                    apps, baseCode: 1, mediaType: lowerRec.MediaType,
                    lowerRec.HostInterfaceIdApp, lowerRec.MediaInterfaceIdApp,
                    lowerRec.HostLaneCountApp, lowerRec.MediaLaneCountApp,
                    lowerRec.HostLaneAssignmentOptionsApp);
                if (page01hBuf is not null)
                {
                    var p01 = CmisPage01hAdvertising.Decode(page01hBuf);
                    AppendApplications(
                        apps, baseCode: 9, mediaType: lowerRec.MediaType,
                        p01.HostInterfaceIdApp, p01.MediaInterfaceIdApp,
                        p01.HostLaneCountApp, p01.MediaLaneCountApp,
                        p01.HostLaneAssignmentOptionsApp);
                }
                if (apps.Count > 0)
                {
                    applications = apps;
                }
            }

            // ─── lanes / flags (CMIS Page 11h; paged modules only) ───────────
            // The schema-owned DPSM name projection is bound ONCE here, from
            // the module's revision byte (Lower:1), before the lane loop: a
            // supported context uses the bound callable; a refused —
            // unsupported present — context (admitted major, byte outside the
            // six catalog revisions) keeps a null projection so the fallback
            // branch is selected before any call and every raw renders
            // Unknown<n>.
            object? lanes = null;
            object? flags = null;
            if (page11hBuf is not null && !lowerRec.MemoryModel)
            {
                if (page01hBuf is null)
                {
                    throw new ArgumentException(
                        "Page 11h present but Page 01h missing (Tx-bias multiplier)");
                }
                var p01 = CmisPage01hAdvertising.Decode(page01hBuf);
                var p11 = CmisPage11hLaneStatus.Decode(page11hBuf);

                // LaserBiasTx multiplier from Page 01h:160 bits 4-3 (CMIS 5.3
                // Table 8-52). Reserved 0b11: the multiplier is undefined, so
                // tx_bias_ma is unrepresentable — emit null on every lane.
                double? mult = p01.TxBiasCurrentScalingFactor switch
                {
                    0 => 1.0,
                    1 => 2.0,
                    2 => 4.0,
                    _ => null,
                };
                var projection = CmisDataPathStateProjections.ForRevision(cmisByte);

                var lanesList = new List<object?>(8);
                var laneFlags = new List<object?>(8);
                for (var i = 0; i < 8; i++)
                {
                    var nibble = p11.DpStateHostLane[i];
                    var dpState = projection?.Invoke(nibble) ?? $"Unknown{nibble}";
                    lanesList.Add(new Dictionary<string, object?>
                    {
                        ["index"] = i + 1,
                        ["dp_state"] = dpState,
                        // Per-lane status latches (§ 8.10.3): rx_los ← LosFlagRx<lane>
                        // (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87),
                        // both from the generated Page 11h decoder.
                        ["rx_los"] = p11.LosFlagRx[i],
                        ["tx_fault"] = p11.FailureFlagTx[i],
                        ["rx_power_mw"] = Math.Round(p11.OpticalPowerRx[i] * 0.0001, 4),
                        ["tx_bias_ma"] = mult is null
                            ? null
                            : Math.Round(p11.LaserBiasTx[i] * 0.002 * mult.Value, 3),
                        ["tx_power_mw"] = Math.Round(p11.OpticalPowerTx[i] * 0.0001, 4),
                    });
                    laneFlags.Add(new Dictionary<string, object?>
                    {
                        ["rx_power_high_alarm"] = p11.OpticalPowerHighAlarmFlagRx[i],
                        ["rx_power_low_alarm"] = p11.OpticalPowerLowAlarmFlagRx[i],
                        ["rx_power_high_warning"] = p11.OpticalPowerHighWarningFlagRx[i],
                        ["rx_power_low_warning"] = p11.OpticalPowerLowWarningFlagRx[i],
                        ["tx_bias_high_alarm"] = p11.LaserBiasHighAlarmFlagTx[i],
                        ["tx_bias_low_alarm"] = p11.LaserBiasLowAlarmFlagTx[i],
                        ["tx_bias_high_warning"] = p11.LaserBiasHighWarningFlagTx[i],
                        ["tx_bias_low_warning"] = p11.LaserBiasLowWarningFlagTx[i],
                        ["tx_power_high_alarm"] = p11.OpticalPowerHighAlarmFlagTx[i],
                        ["tx_power_low_alarm"] = p11.OpticalPowerLowAlarmFlagTx[i],
                        ["tx_power_high_warning"] = p11.OpticalPowerHighWarningFlagTx[i],
                        ["tx_power_low_warning"] = p11.OpticalPowerLowWarningFlagTx[i],
                    });
                }

                lanes = lanesList;
                flags = new Dictionary<string, object?>
                {
                    ["data_not_ready"] = null,
                    ["interrupt_asserted"] = !lowerRec.InterruptDeasserted,
                    ["temp_high_alarm"] = lowerRec.TempMonHighAlarmFlag,
                    ["temp_low_alarm"] = lowerRec.TempMonLowAlarmFlag,
                    ["supply_high_alarm"] = lowerRec.VccMonHighAlarmFlag,
                    ["supply_low_alarm"] = lowerRec.VccMonLowAlarmFlag,
                    ["lanes"] = laneFlags,
                };
            }

            var result = new Dictionary<string, object?>
            {
                ["decode_status"] = "ok",
                ["identity"] = identity,
                ["module"] = module,
                ["monitoring"] = monitoring,
                ["applications"] = applications,
                ["checksums"] = null,
                ["flags"] = flags,
                ["lanes"] = lanes,
            };

            // The `vdm` block is emitted only when the caller supplied the
            // complete VDM page set (20h-2Fh, 2Eh excluded): a partial set
            // yields no block, never one fabricated from zero-filled missing
            // pages.
            if (vdmPages is not null && VdmPagesComplete(vdmPages))
            {
                result["vdm"] = DecodeVdm(vdmPages);
            }

            return result;
        }

        // ─── VDM (Pages 20h-2Fh, CMIS § 8.27) ───────────────────────────────

        /// <summary>Complete VDM page set: descriptors 20h-23h, samples
        /// 24h-27h, threshold quads 28h-2Bh, TC flags 2Ch, TC masks 2Dh and
        /// the advertisement 2Fh (2Eh is not part of the feature).</summary>
        private static readonly string[] VdmPageTags =
        {
            "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
            "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
        };

        private static bool VdmPagesComplete(Dictionary<string, byte[]> vdmPages)
        {
            foreach (var tag in VdmPageTags)
            {
                if (!vdmPages.TryGetValue(tag, out var buf) || buf is null)
                {
                    return false;
                }
            }
            return true;
        }

        /// <summary>
        /// Builds the <c>vdm</c> block from the complete caller-supplied VDM
        /// page set, mirroring bindings/go/cmis_vdm.go and
        /// bindings/python/openxcvr/_cmis.py: group gating on the Page 2Fh
        /// advertisement, polymorphic per-observable-type X16 sample decode
        ///, threshold quads and TC flag/mask nibbles.
        /// Pure: no I/O; a short page of the supplied set throws (fail
        /// closed).
        /// </summary>
        private static Dictionary<string, object?> DecodeVdm(Dictionary<string, byte[]> pages)
        {
            var page2fh = pages["2fh"];
            if (page2fh.Length < 128)
            {
                throw new ArgumentException(
                    $"VDM page 2Fh missing or short: {page2fh.Length} bytes");
            }
            var ad = CmisPage2FhVdmAdvertisement.Decode(page2fh);

            // CmisVdmSupport code = advertised group count - 1; only
            // advertised groups' descriptor/sample pages are decoded.
            int groups = ad.VdmSupport + 1;

            var observables = new List<object?>();
            for (int group = 0; group < groups; group++)
            {
                var descBuf = pages[VdmPageTags[group]];
                if (descBuf.Length < 128)
                {
                    throw new ArgumentException(
                        $"VDM descriptor page {0x20 + group:X2}h missing or short: {descBuf.Length} bytes");
                }
                var sampleBuf = pages[VdmPageTags[4 + group]];
                if (sampleBuf.Length < 128)
                {
                    throw new ArgumentException(
                        $"VDM sample page {0x24 + group:X2}h missing or short: {sampleBuf.Length} bytes");
                }

                int[] localThresholdSetId;
                int[] monitoredResource;
                int[] observableType;
                int[] vdmSample;
                if (group == 0)
                {
                    var d = CmisPage20hVdmDescriptors1To64.Decode(descBuf);
                    localThresholdSetId = d.LocalThresholdSetId;
                    monitoredResource = d.MonitoredResource;
                    observableType = d.ObservableType;
                    vdmSample = CmisPage24hVdmSamples1To64.Decode(sampleBuf).VdmSample;
                }
                else if (group == 1)
                {
                    var d = CmisPage21hVdmDescriptors65To128.Decode(descBuf);
                    localThresholdSetId = d.LocalThresholdSetId;
                    monitoredResource = d.MonitoredResource;
                    observableType = d.ObservableType;
                    vdmSample = CmisPage25hVdmSamples65To128.Decode(sampleBuf).VdmSample;
                }
                else if (group == 2)
                {
                    var d = CmisPage22hVdmDescriptors129To192.Decode(descBuf);
                    localThresholdSetId = d.LocalThresholdSetId;
                    monitoredResource = d.MonitoredResource;
                    observableType = d.ObservableType;
                    vdmSample = CmisPage26hVdmSamples129To192.Decode(sampleBuf).VdmSample;
                }
                else
                {
                    var d = CmisPage23hVdmDescriptors193To256.Decode(descBuf);
                    localThresholdSetId = d.LocalThresholdSetId;
                    monitoredResource = d.MonitoredResource;
                    observableType = d.ObservableType;
                    vdmSample = CmisPage27hVdmSamples193To256.Decode(sampleBuf).VdmSample;
                }

                for (int local = 0; local < 64; local++)
                {
                    // An unused descriptor slot is 00h in both bytes (CMIS
                    // Table 8-178): observable type 0 means the instance is
                    // not active and its sample is ignored.
                    if (observableType[local] == 0)
                    {
                        continue;
                    }
                    int raw = vdmSample[local];
                    var observable = new Dictionary<string, object?>
                    {
                        ["instance"] = group * 64 + local + 1,
                        ["group"] = group + 1,
                        ["threshold_set_id"] = localThresholdSetId[local],
                        ["global_threshold_set_id"] = group * 16 + localThresholdSetId[local] + 1,
                        ["monitored_resource"] = monitoredResource[local],
                        ["observable_type"] = observableType[local],
                        ["active"] = true,
                        ["sample_raw"] = $"0x{raw:X4}",
                        ["sample"] = DecodeVdmSample(observableType[local], raw),
                    };
                    observables.Add(observable);
                }
            }

            // Supervision threshold quads (Pages 28h-2Bh, global sets 1-64).
            string[] thresholdTags = { "28h", "29h", "2Ah", "2Bh" };
            var thresholdQuads = new List<object?>();
            for (int pageIdx = 0; pageIdx < 4; pageIdx++)
            {
                string tag = thresholdTags[pageIdx];
                var buf = pages[tag.ToLowerInvariant()];
                if (buf.Length < 128)
                {
                    throw new ArgumentException(
                        $"VDM threshold page {tag} missing or short: {buf.Length} bytes");
                }
                int[] hiAlarm;
                int[] loAlarm;
                int[] hiWarning;
                int[] loWarning;
                if (pageIdx == 0)
                {
                    var q = CmisPage28hVdmThresholds1To16.Decode(buf);
                    hiAlarm = q.HighAlarmThreshold;
                    loAlarm = q.LowAlarmThreshold;
                    hiWarning = q.HighWarningThreshold;
                    loWarning = q.LowWarningThreshold;
                }
                else if (pageIdx == 1)
                {
                    var q = CmisPage29hVdmThresholds17To32.Decode(buf);
                    hiAlarm = q.HighAlarmThreshold;
                    loAlarm = q.LowAlarmThreshold;
                    hiWarning = q.HighWarningThreshold;
                    loWarning = q.LowWarningThreshold;
                }
                else if (pageIdx == 2)
                {
                    var q = CmisPage2AhVdmThresholds33To48.Decode(buf);
                    hiAlarm = q.HighAlarmThreshold;
                    loAlarm = q.LowAlarmThreshold;
                    hiWarning = q.HighWarningThreshold;
                    loWarning = q.LowWarningThreshold;
                }
                else
                {
                    var q = CmisPage2BhVdmThresholds49To64.Decode(buf);
                    hiAlarm = q.HighAlarmThreshold;
                    loAlarm = q.LowAlarmThreshold;
                    hiWarning = q.HighWarningThreshold;
                    loWarning = q.LowWarningThreshold;
                }
                for (int set = 0; set < 16; set++)
                {
                    int hiA = hiAlarm[set];
                    int loA = loAlarm[set];
                    int hiW = hiWarning[set];
                    int loW = loWarning[set];
                    // An all-zero quad is an undefined threshold set:
                    // emitted only when at least one carrier is non-zero.
                    if (hiA == 0 && loA == 0 && hiW == 0 && loW == 0)
                    {
                        continue;
                    }
                    thresholdQuads.Add(new Dictionary<string, object?>
                    {
                        ["page"] = tag,
                        ["set"] = set + 1,
                        ["high_alarm"] = $"0x{hiA:X4}",
                        ["low_alarm"] = $"0x{loA:X4}",
                        ["high_warning"] = $"0x{hiW:X4}",
                        ["low_warning"] = $"0x{loW:X4}",
                    });
                }
            }

            // TC flags and masks (Pages 2Ch / 2Dh, 256 instances).
            var flagsBuf = pages["2ch"];
            if (flagsBuf.Length < 128)
            {
                throw new ArgumentException(
                    $"VDM page 2Ch missing or short: {flagsBuf.Length} bytes");
            }
            var masksBuf = pages["2dh"];
            if (masksBuf.Length < 128)
            {
                throw new ArgumentException(
                    $"VDM page 2Dh missing or short: {masksBuf.Length} bytes");
            }
            var tc = CmisPage2ChVdmFlags.Decode(flagsBuf);
            var masks = CmisPage2DhVdmMasks.Decode(masksBuf);

            var support = new Dictionary<string, object?>
            {
                ["vdm_support"] = ad.VdmSupport,
                ["vdm_support_name"] = CmisVdmSupportLabels.Label(ad.VdmSupport),
                ["groups_advertised"] = groups,
                ["duty_cycle_support"] = ad.DutyCycleSupport,
                ["power_saving_support"] = ad.PowerSavingSupport,
                ["fine_interval_length_0p1ms"] = ad.FineIntervalLength,
                ["freeze_request"] = ad.FreezeRequest,
                ["power_saving_mode"] = ad.PowerSavingMode,
                ["monitoring_duty_cycle_k"] = ad.MonitoringDutyCycle,
                ["freeze_done"] = ad.FreezeDone,
                ["unfreeze_done"] = ad.UnfreezeDone,
            };

            return new Dictionary<string, object?>
            {
                ["support"] = support,
                ["observables"] = observables,
                ["threshold_quads"] = thresholdQuads,
                ["tc_flags"] = VdmNibbleEntries(
                    tc.HighAlarmTcFlag, tc.LowAlarmTcFlag,
                    tc.HighWarningTcFlag, tc.LowWarningTcFlag),
                ["tc_masks"] = VdmNibbleEntries(
                    masks.HighAlarmTcMask, masks.LowAlarmTcMask,
                    masks.HighWarningTcMask, masks.LowWarningTcMask),
            };
        }

        /// <summary>Projects 256 per-instance (high_alarm, low_alarm,
        /// high_warning, low_warning) bit slices into entries, emitting an
        /// instance only when at least one of its four bits is set (an
        /// all-zero nibble carries no threshold-crossing state).</summary>
        private static List<object?> VdmNibbleEntries(
            bool[] hiABits, bool[] loABits, bool[] hiWBits, bool[] loWBits)
        {
            var entries = new List<object?>();
            for (int i = 0; i < 256; i++)
            {
                bool hiA = hiABits[i];
                bool loA = loABits[i];
                bool hiW = hiWBits[i];
                bool loW = loWBits[i];
                if (hiA || loA || hiW || loW)
                {
                    entries.Add(new Dictionary<string, object?>
                    {
                        ["instance"] = i + 1,
                        ["high_alarm"] = hiA,
                        ["low_alarm"] = loA,
                        ["high_warning"] = hiW,
                        ["low_warning"] = loW,
                    });
                }
            }
            return entries;
        }

        /// <summary>
        /// Decodes one X16 sample register per the descriptor's observable
        /// type: S16 for
        /// TEC current / laser frequency error / laser temperature
        /// (0x02-0x04) and ELS input power (0x54); F16 (m * 10^(s-24)) for the
        /// Pre-FEC BER and FERC families (0x09-0x1A); raw U16 otherwise.
        /// The F16 product is converted via correctly-rounded decimal parsing
        /// (double.Parse of "&lt;m&gt;e&lt;s-24&gt;"), the strtod oracle the
        /// Rust reference and the C binding's openxcvr_cmis_f16_to_double
        /// are held to; Math.Pow carries no correctly-rounded guarantee.
        /// </summary>
        private static Dictionary<string, object?> DecodeVdmSample(int observableType, int raw)
        {
            if ((observableType >= 0x02 && observableType <= 0x04) || observableType == 0x54)
            {
                return new Dictionary<string, object?>
                {
                    ["storage"] = "i16",
                    ["value"] = (raw & 0x8000) != 0 ? raw - 0x10000 : raw,
                };
            }
            if (observableType >= 0x09 && observableType <= 0x1A)
            {
                int m = raw & 0x7FF;
                int s = (raw >> 11) & 0x1F;
                return new Dictionary<string, object?>
                {
                    ["storage"] = "f16",
                    ["value"] = double.Parse($"{m}e{s - 24}", CultureInfo.InvariantCulture),
                };
            }
            return new Dictionary<string, object?>
            {
                ["storage"] = "u16",
                ["value"] = raw,
            };
        }

        private static void AppendApplications(
            List<object?> apps,
            int baseCode,
            int mediaType,
            int[] hostInterfaceId,
            int[] mediaInterfaceId,
            int[] hostLaneCount,
            int[] mediaLaneCount,
            int[] hostLaneAssignmentOptions)
        {
            for (var i = 0; i < hostInterfaceId.Length; i++)
            {
                var hif = hostInterfaceId[i];
                if (hif == 0xFF)
                {
                    break;
                }
                var mif = mediaInterfaceId[i];
                if (mif == 0)
                {
                    continue;
                }
                var mediaName = CmisMediaInterface.Name(mediaType, mif);
                apps.Add(new Dictionary<string, object?>
                {
                    ["app_code"] = baseCode + i,
                    ["host_interface_id"] = hif,
                    ["host_interface_name"] = Sff8024HostElectricalInterfaceLabels.Label(hif),
                    ["host_lane_assignment_options"] = Hex(hostLaneAssignmentOptions[i]),
                    ["host_lane_count"] = hostLaneCount[i],
                    ["media_interface_id"] = mif,
                    ["media_interface_name"] = mediaName ?? "unknown",
                    ["media_lane_count"] = mediaLaneCount[i],
                });
            }
        }

        // ─── SFF-8636 ───────────────────────────────────────────────────────

        /// <summary>
        /// Decodes SFF-8636 module pages (lower memory + Page 00h upper
        /// administrative) into the shared golden JSON shape.
        /// </summary>
        public static Dictionary<string, object?> DecodeSff8636(byte[] lower, byte[] page00h)
        {
            var lowerBuf = CheckPage("lower", lower, required: true);
            var page00hBuf = CheckPage("page00h", page00h, required: true);

            var lowerRec = Sff8636LowerMemory.Decode(lowerBuf);
            var page00 = Sff8636Page00hUpperAdministrative.Decode(page00hBuf);

            object? extSpec = null;
            if (page00.ExtSpecCompliance != 0)
            {
                extSpec = new Dictionary<string, object?>
                {
                    ["name"] = page00.ExtSpecCompliance == 0x02 ? ExtSpec100gSr4Or25gSr : "unknown",
                    ["raw"] = Hex(page00.ExtSpecCompliance),
                };
            }

            var bitsSet = new List<object?>();
            if (page00.ComplianceExtended)
            {
                bitsSet.Add("compliance_extended");
            }
            if (page00.Compliance40gbaseSr4)
            {
                bitsSet.Add("compliance_40gbase_sr4");
            }

            object? moduleType = null;
            if (page00.ExtSpecCompliance == 0x02)
            {
                moduleType = "100GBASE-SR4 QSFP28";
            }
            else if (page00.Compliance40gbaseSr4)
            {
                moduleType = "40GBASE-SR4 QSFP+";
            }

            // Byte 12 == 0xFF is the sentinel for rates > 25.4 GBd; the
            // high-resolution rate is byte 66 in 250 MBd units (÷4 → GBd).
            var nominalRate = page00.NominalSignalingRate == 0xFF
                ? page00.ExtBaudRate / 4.0
                : page00.NominalSignalingRate / 10.0;

            var interruptAsserted = !lowerRec.InterruptDeasserted
                || lowerRec.TxLosFlag.Any(b => b)
                || lowerRec.RxLosFlag.Any(b => b)
                || lowerRec.RxPowerAlarmWarningFlag.Concat(lowerRec.TxBiasAlarmWarningFlag)
                    .Concat(lowerRec.TxPowerAlarmWarningFlag).Any(v => v);

            var extendedIdentifier = Hex(
                page00.PowerClassLow
                | (page00.PowerClassHigh << 6)
                | (page00.RxCdrPresent ? 1 << 2 : 0)
                | (page00.TxCdrPresent ? 1 << 3 : 0)
                | (page00.CleiPresent ? 1 << 4 : 0)
                | (page00.PowerClass8Implemented ? 1 << 5 : 0));

            var lanes = new List<object?>(4);
            for (var i = 0; i < 4; i++)
            {
                lanes.Add(new Dictionary<string, object?>
                {
                    ["index"] = i + 1,
                    ["rx_power_mw"] = Math.Round(lowerRec.RxPowerLane[i] * 0.0001, 4),
                    ["tx_bias_ma"] = Math.Round(lowerRec.TxBiasLane[i] * 0.002, 3),
                    ["tx_power_mw"] = Math.Round(lowerRec.TxPowerLane[i] * 0.0001, 4),
                });
            }

            return new Dictionary<string, object?>
            {
                ["decode_status"] = "ok",
                ["identity"] = new Dictionary<string, object?>
                {
                    ["date_code"] = page00.DateCodeYear + page00.DateCodeMonth + page00.DateCodeDay,
                    ["vendor_name"] = page00.VendorName,
                    ["vendor_oui"] = Oui(page00.VendorOui),
                    ["vendor_pn"] = page00.VendorPn,
                    ["vendor_rev"] = page00.VendorRev,
                    ["vendor_sn"] = page00.VendorSn,
                },
                ["module"] = new Dictionary<string, object?>
                {
                    ["cmis_revision"] = null,
                    ["connector"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8024ConnectorTypeLabels.Label(page00.ConnectorType),
                        ["raw"] = Hex(page00.ConnectorType),
                    },
                    ["encoding"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8024EncodingLabels.Label(page00.Encoding),
                        ["raw"] = Hex(page00.Encoding),
                    },
                    ["extended_identifier"] = extendedIdentifier,
                    ["identifier"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8024IdentifierLabels.Label(lowerRec.Sff8024Identifier),
                        ["raw"] = Hex(lowerRec.Sff8024Identifier),
                    },
                    ["memory_model"] = lowerRec.FlatMemory ? "flat" : "paged",
                    ["module_state"] = null,
                    ["module_type"] = moduleType,
                    ["nominal_bit_rate_gbps"] = nominalRate,
                },
                ["monitoring"] = new Dictionary<string, object?>
                {
                    ["module_temperature_c"] = lowerRec.Temperature / 256.0,
                    ["module_voltage_v"] = Math.Round(lowerRec.SupplyVoltage * 0.0001, 4),
                },
                ["compliance"] = new Dictionary<string, object?>
                {
                    ["bits_set"] = bitsSet,
                    ["device_technology"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8636DeviceTechnologyLabels.Label(page00.DeviceTechnology),
                        ["raw"] = HexNibble(page00.DeviceTechnology),
                    },
                    ["ext_spec_byte_192"] = extSpec,
                    ["raw_bytes_page_00h_3_10"] = HexBytes(page00hBuf, 3, 11),
                    ["reach"] = new Dictionary<string, object?>
                    {
                        ["om1_m"] = page00.LengthOm1OrCopperAttenuation25p78ghz,
                        ["om2_m"] = page00.LengthOm21m,
                        ["om3_2m_units"] = page00.LengthOm32m * 2,
                        ["om4_or_copper_m"] = page00.LengthCableOrOm4,
                        ["smf_km"] = page00.LengthSmfKm,
                    },
                },
                ["flags"] = new Dictionary<string, object?>
                {
                    ["data_not_ready"] = lowerRec.DataNotReady,
                    ["interrupt_asserted"] = interruptAsserted,
                },
                ["lanes"] = lanes,
                ["checksums"] = new Dictionary<string, object?>
                {
                    ["cc_base"] = Checksum(page00hBuf, 0, 63, 63),
                    ["cc_ext"] = Checksum(page00hBuf, 64, 95, 95),
                },
            };
        }

        // ─── SFF-8472 ───────────────────────────────────────────────────────

        /// <summary>
        /// Decodes SFF-8472 A0h/A2h EEPROM pages into the shared golden JSON
        /// shape. Like the Python facade's <c>_pad_page</c>, each page of any
        /// length is zero-padded to 256 bytes with no minimum size, and every
        /// caller byte is kept (A0h 128-255 vendor-specific, A2h 128-255 user
        /// EEPROM / paged region); only a null page throws. Telemetry in this
        /// result is uncalibrated: SFF-8472 external calibration constants
        /// are not applied. Check calibration.externally_calibrated;
        /// calibrated values come from the Rust SFF-8472 adapter or the
        /// observation APIs.
        /// </summary>
        public static Dictionary<string, object?> DecodeSff8472(byte[] a0h, byte[] a2h)
        {
            if (a0h is null)
            {
                throw new ArgumentException("a0h is required");
            }
            if (a2h is null)
            {
                throw new ArgumentException("a2h is required");
            }
            var a0hBuf = PadPage(a0h, 256);
            var a2hBuf = PadPage(a2h, 256);

            var a0 = PageA0hBaseMemory.Decode(a0hBuf);
            var a2 = PageA2hDiagnosticsBase.Decode(a2hBuf);

            object? moduleType = null;
            if (a0.TransceiverCompliance.Length > 3 && a0.TransceiverCompliance[3])
            {
                moduleType = "10GBASE-SR SFP+";
            }

            // SFF-8472 v12.5a § 5.6: nominal signaling rate byte 12 == 0xFF is
            // a sentinel for rates > 25.4 GBd; the high-resolution rate is in
            // byte 66 (250 MBd units → divide by 4 to express in GBd).
            var nominalBitRate = a0.SignalingRateNominal == 0xFF
                ? a0.SignalingRateMax / 4.0
                : a0.SignalingRateNominal / 10.0;

            // SFF-8472 § 8.8: DDM (A2h real-time monitoring) is optional. When
            // byte 92 bit 6 (digital_diagnostics_implemented) is clear, A2h
            // holds no meaningful monitoring/flag/lane data — emit null rather
            // than zero-filled structures.
            var ddm = a0.DigitalDiagnosticsImplemented;

            object? flagsOut = null;
            object? monitoringOut = null;
            object? lanesOut = null;
            if (ddm)
            {
                var alarmFlags = a2.AlarmWarningFlag;
                flagsOut = new Dictionary<string, object?>
                {
                    ["alarm_summary"] = new Dictionary<string, object?>
                    {
                        ["rx_power_high_alarm"] = alarmFlags[8],
                        ["rx_power_low_alarm"] = alarmFlags[9],
                        ["temp_high_alarm"] = alarmFlags[0],
                        ["temp_low_alarm"] = alarmFlags[1],
                        ["tx_bias_high_alarm"] = alarmFlags[4],
                        ["tx_bias_low_alarm"] = alarmFlags[5],
                        ["tx_power_high_alarm"] = alarmFlags[6],
                        ["tx_power_low_alarm"] = alarmFlags[7],
                        ["vcc_high_alarm"] = alarmFlags[2],
                        ["vcc_low_alarm"] = alarmFlags[3],
                    },
                    ["warning_summary"] = new Dictionary<string, object?>
                    {
                        ["rx_power_high_warning"] = alarmFlags[24],
                        ["rx_power_low_warning"] = alarmFlags[25],
                        ["temp_high_warning"] = alarmFlags[16],
                        ["temp_low_warning"] = alarmFlags[17],
                        ["tx_bias_high_warning"] = alarmFlags[20],
                        ["tx_bias_low_warning"] = alarmFlags[21],
                        ["tx_power_high_warning"] = alarmFlags[22],
                        ["tx_power_low_warning"] = alarmFlags[23],
                        ["vcc_high_warning"] = alarmFlags[18],
                        ["vcc_low_warning"] = alarmFlags[19],
                    },
                };
                monitoringOut = new Dictionary<string, object?>
                {
                    ["module_temperature_c"] = a2.Temperature / 256.0,
                    ["module_voltage_v"] = Math.Round(a2.SupplyVoltage * 0.0001, 4),
                };
                lanesOut = new List<object?>
                {
                    new Dictionary<string, object?>
                    {
                        ["index"] = 1,
                        // Status/control bits (A2h byte 110) read from the
                        // generated decoded fields, not re-shifted from the
                        // raw byte.
                        ["rx_los"] = a2.RxLosState,
                        ["rx_power_mw"] = Math.Round(a2.RxPower * 0.0001, 4),
                        ["tx_bias_ma"] = Math.Round(a2.TxBias * 0.002, 3),
                        ["tx_disable"] = a2.TxDisableState,
                        ["tx_fault"] = a2.TxFaultState,
                        ["tx_power_mw"] = Math.Round(a2.TxPower * 0.0001, 4),
                    },
                };
            }

            return new Dictionary<string, object?>
            {
                ["decode_status"] = "ok",
                ["identity"] = new Dictionary<string, object?>
                {
                    ["date_code"] = a0.DateCode.Length >= 6 ? a0.DateCode[..6] : a0.DateCode,
                    ["vendor_name"] = a0.VendorName,
                    ["vendor_oui"] = Oui(a0.VendorOui),
                    ["vendor_pn"] = a0.VendorPn,
                    ["vendor_sn"] = a0.VendorSn,
                },
                ["module"] = new Dictionary<string, object?>
                {
                    ["calibration"] = new Dictionary<string, object?>
                    {
                        ["ddm_implemented"] = a0.DigitalDiagnosticsImplemented,
                        ["externally_calibrated"] = a0.ExternallyCalibrated,
                        ["internally_calibrated"] = a0.InternallyCalibrated,
                        ["rx_power_kind"] = a0.RxPowerAverage ? "average" : "oma",
                    },
                    ["cmis_revision"] = null,
                    ["connector"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8024ConnectorTypeLabels.Label(a0.ConnectorType),
                        ["raw"] = Hex(a0.ConnectorType),
                    },
                    ["encoding"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8024EncodingLabels.Label(a0.Encoding),
                        ["raw"] = Hex(a0.Encoding),
                    },
                    ["identifier"] = new Dictionary<string, object?>
                    {
                        ["name"] = Sff8024IdentifierLabels.Label(a0.Sff8024Identifier),
                        ["raw"] = Hex(a0.Sff8024Identifier),
                    },
                    ["memory_model"] = null,
                    ["module_state"] = null,
                    ["module_type"] = moduleType,
                    ["nominal_bit_rate_gbps"] = nominalBitRate,
                },
                ["monitoring"] = monitoringOut,
                ["compliance"] = new Dictionary<string, object?>
                {
                    ["raw_bytes_a0h_3_10"] = HexBytes(a0hBuf, 3, 11),
                    ["reach"] = new Dictionary<string, object?>
                    {
                        ["om1_m"] = a0.LengthOm110m * 10,
                        ["om2_m"] = a0.LengthOm210m * 10,
                        // SFF-8472 byte 19 is OM3 length in units of 10 m
                        // (classic flat byte; base|multiplier<<6 == the raw
                        // byte). ×10 yields meters. Deliberate: the schema's
                        // base(u6)+multiplier(u2 enum) split is not the wire
                        // semantics — finisar (byte19=30, golden=300)
                        // falsifies the ×0.1 reading. Do not "fix".
                        ["om3_m"] = (a0.LengthOm3Base | (a0.LengthOm3Multiplier << 6)) * 10,
                        ["om4_m"] = a0.LengthOm4OrCopper1m,
                        ["smf_km"] = a0.LengthSmfKm,
                    },
                    ["wavelength_nm"] = a0.WavelengthOrCableCompliance,
                },
                ["flags"] = flagsOut,
                ["lanes"] = lanesOut,
                ["checksums"] = new Dictionary<string, object?>
                {
                    ["cc_base"] = Checksum(a0hBuf, 0, 63, 63),
                    ["cc_dmi"] = Checksum(a2hBuf, 0, 95, 95),
                    ["cc_ext"] = Checksum(a0hBuf, 64, 95, 95),
                },
            };
        }
    }
}
