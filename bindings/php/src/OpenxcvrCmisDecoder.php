<?php
declare(strict_types=1);
namespace Openxcvr;

use Openxcvr\Generated\CmisLowerMemory;
use Openxcvr\Generated\CmisPage00hAdministrative;
use Openxcvr\Generated\CmisPage11hLaneStatus;
use Openxcvr\Generated\CmisPage20hVdmDescriptors1To64;
use Openxcvr\Generated\CmisPage21hVdmDescriptors65To128;
use Openxcvr\Generated\CmisPage22hVdmDescriptors129To192;
use Openxcvr\Generated\CmisPage23hVdmDescriptors193To256;
use Openxcvr\Generated\CmisPage24hVdmSamples1To64;
use Openxcvr\Generated\CmisPage25hVdmSamples65To128;
use Openxcvr\Generated\CmisPage26hVdmSamples129To192;
use Openxcvr\Generated\CmisPage27hVdmSamples193To256;
use Openxcvr\Generated\CmisPage28hVdmThresholds1To16;
use Openxcvr\Generated\CmisPage29hVdmThresholds17To32;
use Openxcvr\Generated\CmisPage2AhVdmThresholds33To48;
use Openxcvr\Generated\CmisPage2BhVdmThresholds49To64;
use Openxcvr\Generated\CmisPage2ChVdmFlags;
use Openxcvr\Generated\CmisPage2DhVdmMasks;
use Openxcvr\Generated\CmisPage2FhVdmAdvertisement;
use Openxcvr\Generated\Enums\CmisDataPathState;
use Openxcvr\Generated\Enums\CmisVdmSupport;

trait OpenxcvrCmisDecoder
{
    private static array $MODULE_STATE_NAMES = [
        0x00 => 'Reserved0',
        0x01 => 'ModuleLowPwr',
        0x02 => 'ModulePwrUp',
        0x03 => 'ModuleReady',
        0x04 => 'ModulePwrDn',
        0x05 => 'ModuleFault',
    ];

    /**
     * @param string $page01h optional; empty string means absent
     * @param string $page11h optional; empty string means absent
     * @param array<string, string> $vdmPages optional VDM page set (CMIS
     *     § 8.27) keyed '20h'..'2fh' (2Eh excluded); the `vdm` block is
     *     decoded only when every page of the set is supplied
     */
    public static function decodeCmis(
        string $lower,
        string $page00h,
        string $page01h = '',
        string $page11h = '',
        array $vdmPages = []
    ): array {
        if (strlen($lower) < 128) {
            throw new \InvalidArgumentException(
                'lower page too short: ' . strlen($lower) . ' bytes'
            );
        }

        $cmisByte = ord($lower[1]);
        $cmisM    = ($cmisByte >> 4) & 0xF;
        $cmism    = $cmisByte & 0xF;

        if ($cmisM !== 3 && $cmisM !== 4 && $cmisM !== 5) {
            return [
                'decode_status' => 'rejected',
                'rejection'     => [
                    'detail' => sprintf(
                        'Lower memory byte 1 = 0x%02X; '
                        . 'CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. '
                        . 'The fboss test corpus uses 0x%02X as an explicit anomalous-rev regression target.',
                        $cmisByte, $cmisByte
                    ),
                    'reason' => sprintf('unknown_major_revision: 0x%02X', $cmisByte),
                ],
            ];
        }

        $lowerDecoded   = CmisLowerMemory::decode($lower);
        $page00hDecoded = CmisPage00hAdministrative::decode($page00h);

        // ─── identity ────────────────────────────────────────────────────────
        $vendorOUI = sprintf(
            '%02X:%02X:%02X',
            ord($page00h[17]),
            ord($page00h[18]),
            ord($page00h[19])
        );

        // Read string fields directly from $page00h, trimming only trailing ASCII
        // spaces (0x20) — not null bytes. Null-padded fields must preserve their
        // null bytes to match the golden JSON expectation.
        $vendorName = rtrim(substr($page00h, 1, 16), " ");
        $vendorPn   = rtrim(substr($page00h, 20, 16), " ");
        $vendorRev  = rtrim(substr($page00h, 36, 2), " ");
        $vendorSn   = rtrim(substr($page00h, 38, 16), " ");
        $dateCode   = rtrim(substr($page00h, 54, 2), " ")
            . rtrim(substr($page00h, 56, 2), " ")
            . rtrim(substr($page00h, 58, 2), " ");

        $identity = [
            'date_code'   => $dateCode,
            'vendor_name' => $vendorName,
            'vendor_oui'  => $vendorOUI,
            'vendor_pn'   => $vendorPn,
            'vendor_rev'  => $vendorRev,
            'vendor_sn'   => $vendorSn,
        ];

        // ─── module ───────────────────────────────────────────────────────────
        $memoryModel = $lowerDecoded->memoryModel ? 'flat' : 'paged';

        $moduleStateName = self::$MODULE_STATE_NAMES[$lowerDecoded->moduleState]
            ?? sprintf('Unknown%d', $lowerDecoded->moduleState);

        $identID = $lowerDecoded->sff8024Identifier;
        $connID  = $page00hDecoded->connectorType;

        $module = [
            'cmis_revision' => sprintf('%d.%d', $cmisM, $cmism),
            'connector'     => [
                'name' => self::lookupConnector($connID),
                'raw'  => sprintf('0x%02X', $connID),
            ],
            'identifier'    => [
                'name' => self::lookupIdentifier($identID),
                'raw'  => sprintf('0x%02X', $identID),
            ],
            'memory_model'  => $memoryModel,
            'module_state'  => $moduleStateName,
            'module_type'   => null,
        ];

        // ─── monitoring ───────────────────────────────────────────────────────
        $voltageRaw = $lowerDecoded->vccMonVoltage * 100e-6;
        $voltageV   = round($voltageRaw * 1e4) / 1e4;

        $monitoring = [
            'module_temperature_c' => $lowerDecoded->tempMonValue / 256.0,
            'module_voltage_v'     => $voltageV,
        ];

        // ─── applications ─────────────────────────────────────────────────────
        $appsSlice = [];

        if (!$lowerDecoded->memoryModel) {
            $appsSlice = array_merge(
                $appsSlice,
                self::parseAppDescriptors($lower, 86, 8, 0, ord($lower[85]))
            );
            if (strlen($page01h) >= 128) {
                $appsSlice = array_merge(
                    $appsSlice,
                    self::parseAppDescriptors($page01h, 95, 7, 8, ord($lower[85]))
                );
            }
        }

        $applications = count($appsSlice) > 0 ? $appsSlice : null;

        // ─── lanes / flags (CMIS Page 11h, issue #105) ───────────────────────
        // Present only for paged modules that expose Page 11h; flat-memory
        // modules (no Page 11h) keep both null, matching the OpenAPI contract
        // and the Go/C/Python aggregators. The schema-owned DPSM name
        // projection is bound ONCE here, from the module's revision byte
        // (Lower:1 = $cmisByte), before the lane loop: a supported context
        // uses the bound closure; a refused — unsupported present — context
        // (admitted major, byte outside the six catalog revisions) keeps a
        // null closure so the fallback branch is selected before any call and
        // every raw renders Unknown<n>.
        $lanes = null;
        $flags = null;
        if (strlen($page11h) >= 128 && !$lowerDecoded->memoryModel) {
            $projection = CmisDataPathState::forRevision($cmisByte);
            [$lanes, $flags] = self::decodeCmisLanesFlags(
                $lower,
                $page01h,
                $page11h,
                $projection
            );
        }

        $result = [
            'decode_status' => 'ok',
            'identity'      => $identity,
            'module'        => $module,
            'monitoring'    => $monitoring,
            'applications'  => $applications,
            'checksums'     => null,
            'flags'         => $flags,
            'lanes'         => $lanes,
        ];

        // ─── VDM (Pages 20h-2Fh) ────────────────────────────────────────────
        // Emitted only when the caller supplies the COMPLETE VDM page set
        // (20h-2fh, 2Eh excluded): the block is decoded as a whole from a
        // whole page set; a partial set yields no block rather than one
        // fabricated from zero-filled missing pages, and within the block the
        // Page 2Fh advertisement gates which groups decode (fail closed on
        // any short page).
        $vdmKeys = [
            '20h', '21h', '22h', '23h', '24h', '25h', '26h', '27h',
            '28h', '29h', '2ah', '2bh', '2ch', '2dh', '2fh',
        ];
        $vdmComplete = true;
        foreach ($vdmKeys as $key) {
            if (!isset($vdmPages[$key])) {
                $vdmComplete = false;
                break;
            }
        }
        if ($vdmComplete) {
            $result['vdm'] = self::decodeVdm($vdmPages);
        }

        return $result;
    }

    /**
     * Build the `vdm` block from the complete caller-supplied VDM page set
     * (Pages 20h-2Fh, CMIS § 8.27), mirroring bindings/go/cmis_vdm.go and
     * bindings/python/openxcvr/_cmis.py. Pure: no I/O; a short page of the
     * supplied set throws (fail closed).
     *
     * @param array<string, string> $pages keyed '20h'..'2fh'
     */
    private static function decodeVdm(array $pages): array
    {
        $page2fh = $pages['2fh'];
        if (strlen($page2fh) < 128) {
            throw new \InvalidArgumentException(
                'VDM page 2Fh missing or short: ' . strlen($page2fh) . ' bytes'
            );
        }
        $ad = CmisPage2FhVdmAdvertisement::decode($page2fh);

        // CmisVdmSupport code = advertised group count - 1; only advertised
        // groups' descriptor/sample pages are decoded.
        $groups = $ad->vdmSupport + 1;

        $descriptorClasses = [
            CmisPage20hVdmDescriptors1To64::class,
            CmisPage21hVdmDescriptors65To128::class,
            CmisPage22hVdmDescriptors129To192::class,
            CmisPage23hVdmDescriptors193To256::class,
        ];
        $sampleClasses = [
            CmisPage24hVdmSamples1To64::class,
            CmisPage25hVdmSamples65To128::class,
            CmisPage26hVdmSamples129To192::class,
            CmisPage27hVdmSamples193To256::class,
        ];

        $observables = [];
        foreach (range(0, $groups - 1) as $group) {
            $descTag = sprintf('%02xh', 0x20 + $group);
            $descBuf = $pages[$descTag];
            if (strlen($descBuf) < 128) {
                throw new \InvalidArgumentException(
                    sprintf('VDM descriptor page %02Xh missing or short: %d bytes', 0x20 + $group, strlen($descBuf))
                );
            }
            $sampleTag = sprintf('%02xh', 0x24 + $group);
            $sampleBuf = $pages[$sampleTag];
            if (strlen($sampleBuf) < 128) {
                throw new \InvalidArgumentException(
                    sprintf('VDM sample page %02Xh missing or short: %d bytes', 0x24 + $group, strlen($sampleBuf))
                );
            }
            /** @var CmisPage20hVdmDescriptors1To64 $desc */
            $desc = $descriptorClasses[$group]::decode($descBuf);
            /** @var CmisPage24hVdmSamples1To64 $samples */
            $samples = $sampleClasses[$group]::decode($sampleBuf);
            foreach (range(0, 63) as $local) {
                // An unused descriptor slot is 00h in both bytes (CMIS Table
                // 8-178): observable type 0 means the instance is not active
                // and its sample is ignored.
                if ($desc->observableType[$local] === 0) {
                    continue;
                }
                $raw = $samples->vdmSample[$local];
                $observables[] = [
                    'instance'                => $group * 64 + $local + 1,
                    'group'                   => $group + 1,
                    'threshold_set_id'        => $desc->localThresholdSetId[$local],
                    'global_threshold_set_id' => $group * 16 + $desc->localThresholdSetId[$local] + 1,
                    'monitored_resource'      => $desc->monitoredResource[$local],
                    'observable_type'         => $desc->observableType[$local],
                    'active'                  => true,
                    'sample_raw'              => sprintf('0x%04X', $raw),
                    'sample'                  => self::decodeVdmSample($desc->observableType[$local], $raw),
                ];
            }
        }

        // Supervision threshold quads (Pages 28h-2Bh, global sets 1-64).
        $thresholdClasses = [
            '28h' => CmisPage28hVdmThresholds1To16::class,
            '29h' => CmisPage29hVdmThresholds17To32::class,
            '2Ah' => CmisPage2AhVdmThresholds33To48::class,
            '2Bh' => CmisPage2BhVdmThresholds49To64::class,
        ];
        $thresholdQuads = [];
        foreach ($thresholdClasses as $pageTag => $class) {
            $buf = $pages[strtolower($pageTag)];
            if (strlen($buf) < 128) {
                throw new \InvalidArgumentException(
                    "VDM threshold page $pageTag missing or short: " . strlen($buf) . ' bytes'
                );
            }
            $quads = $class::decode($buf);
            foreach (range(0, 15) as $set) {
                $hiA = $quads->highAlarmThreshold[$set];
                $loA = $quads->lowAlarmThreshold[$set];
                $hiW = $quads->highWarningThreshold[$set];
                $loW = $quads->lowWarningThreshold[$set];
                // An all-zero quad is an undefined threshold set: emitted
                // only when at least one carrier is non-zero.
                if ($hiA === 0 && $loA === 0 && $hiW === 0 && $loW === 0) {
                    continue;
                }
                $thresholdQuads[] = [
                    'page'         => $pageTag,
                    'set'          => $set + 1,
                    'high_alarm'   => sprintf('0x%04X', $hiA),
                    'low_alarm'    => sprintf('0x%04X', $loA),
                    'high_warning' => sprintf('0x%04X', $hiW),
                    'low_warning'  => sprintf('0x%04X', $loW),
                ];
            }
        }

        // TC flags and masks (Pages 2Ch / 2Dh, 256 instances).
        $flagsBuf = $pages['2ch'];
        if (strlen($flagsBuf) < 128) {
            throw new \InvalidArgumentException(
                'VDM page 2Ch missing or short: ' . strlen($flagsBuf) . ' bytes'
            );
        }
        $masksBuf = $pages['2dh'];
        if (strlen($masksBuf) < 128) {
            throw new \InvalidArgumentException(
                'VDM page 2Dh missing or short: ' . strlen($masksBuf) . ' bytes'
            );
        }
        $tc = CmisPage2ChVdmFlags::decode($flagsBuf);
        $masks = CmisPage2DhVdmMasks::decode($masksBuf);

        $nibbleEntries = static function (array $hiABits, array $loABits, array $hiWBits, array $loWBits): array {
            $out = [];
            foreach (range(0, 255) as $i) {
                $hiA = $hiABits[$i];
                $loA = $loABits[$i];
                $hiW = $hiWBits[$i];
                $loW = $loWBits[$i];
                // An all-zero nibble carries no threshold-crossing state.
                if ($hiA || $loA || $hiW || $loW) {
                    $out[] = [
                        'instance'     => $i + 1,
                        'high_alarm'   => $hiA,
                        'low_alarm'    => $loA,
                        'high_warning' => $hiW,
                        'low_warning'  => $loW,
                    ];
                }
            }
            return $out;
        };

        $supportName = CmisVdmSupport::tryFrom($ad->vdmSupport)?->label() ?? 'unknown';

        return [
            'support' => [
                'vdm_support'                => $ad->vdmSupport,
                'vdm_support_name'           => $supportName,
                'groups_advertised'          => $groups,
                'duty_cycle_support'         => $ad->dutyCycleSupport,
                'power_saving_support'       => $ad->powerSavingSupport,
                'fine_interval_length_0p1ms' => $ad->fineIntervalLength,
                'freeze_request'             => $ad->freezeRequest,
                'power_saving_mode'          => $ad->powerSavingMode,
                'monitoring_duty_cycle_k'    => $ad->monitoringDutyCycle,
                'freeze_done'                => $ad->freezeDone,
                'unfreeze_done'              => $ad->unfreezeDone,
            ],
            'observables'     => $observables,
            'threshold_quads' => $thresholdQuads,
            'tc_flags'        => $nibbleEntries(
                $tc->highAlarmTcFlag,
                $tc->lowAlarmTcFlag,
                $tc->highWarningTcFlag,
                $tc->lowWarningTcFlag
            ),
            'tc_masks'        => $nibbleEntries(
                $masks->highAlarmTcMask,
                $masks->lowAlarmTcMask,
                $masks->highWarningTcMask,
                $masks->lowWarningTcMask
            ),
        ];
    }

    /**
     * Decode one X16 sample register per the descriptor's observable type
     *: S16 for TEC
     * current / laser frequency error / laser temperature (0x02-0x04) and ELS
     * input power (0x54); F16 (m * 10^(s-24)) for the Pre-FEC BER and FERC
     * families (0x09-0x1A); raw U16 otherwise.
     *
     * The F16 product is converted via correctly-rounded decimal parsing
     * ((float) sprintf('%de%d', m, s-24) goes through strtod — the oracle the
     * Rust reference and the C binding's openxcvr_cmis_f16_to_double are
     * held to), because PHP's `10.0 ** n` carries no correctly-rounded
     * guarantee.
     */
    private static function decodeVdmSample(int $observableType, int $raw): array
    {
        if (($observableType >= 0x02 && $observableType <= 0x04) || $observableType === 0x54) {
            return ['storage' => 'i16', 'value' => $raw & 0x8000 ? $raw - 0x10000 : $raw];
        }
        if ($observableType >= 0x09 && $observableType <= 0x1A) {
            $m = $raw & 0x7FF;
            $s = ($raw >> 11) & 0x1F;
            return ['storage' => 'f16', 'value' => (float) sprintf('%de%d', $m, $s - 24)];
        }
        return ['storage' => 'u16', 'value' => $raw];
    }

    /**
     * Build the per-lane `lanes` array and the module/per-lane `flags` tree
     * from CMIS Page 11h. Mirrors bindings/go/cmis.go::decodeLanesFlags — the
     * generated Page 11h decoder does all offset/bit math; this applies only
     * scaling, dp_state naming via the schema-owned projection, and flag-key
     * remapping (OpticalPower*Tx→tx_power, LaserBias*Tx→tx_bias,
     * OpticalPower*Rx→rx_power). `$projection` is the generated name
     * projection bound once by the caller for the module's revision byte: a
     * null closure (refused — unsupported present — context, selected before
     * any call) or a raw the projection does not name renders Unknown<n>,
     * matching the C/Go/Python aggregators.
     *
     * @param \Closure|null $projection
     * @return array{0: list<array<string,mixed>>, 1: array<string,mixed>}
     */
    private static function decodeCmisLanesFlags(
        string $lower,
        string $page01h,
        string $page11h,
        ?\Closure $projection
    ): array {
        $p11 = CmisPage11hLaneStatus::decode($page11h);

        // LaserBiasTx multiplier from Page 01h:160 bits 4-3 (CMIS 5.3 Table 8-52).
        if (strlen($page01h) < 128) {
            throw new \RuntimeException(
                'page 11h present but page 01h missing (Tx-bias multiplier)'
            );
        }
        // Reserved 0b11 (CMIS 5.3 Table 8-52): the multiplier is undefined, so
        // tx_bias_ma is unrepresentable. $mult is null → emit null for it on
        // every lane and keep the rest of the lane block intact.
        $mult = match ((ord($page01h[160 - 128]) >> 3) & 0x03) {
            0 => 1.0,
            1 => 2.0,
            2 => 4.0,
            default => null,
        };

        $lanes     = [];
        $laneFlags = [];
        for ($i = 0; $i < 8; $i++) {
            $nibble  = $p11->dpStateHostLane[$i];
            $dpState = $projection !== null ? $projection($nibble) : null;
            if ($dpState === null) {
                $dpState = sprintf('Unknown%d', $nibble);
            }
            $lanes[] = [
                'index'       => $i + 1,
                'dp_state'    => $dpState,
                // Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
                // (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87),
                // both from the generated Page 11h decoder.
                'rx_los'      => $p11->losFlagRx[$i],
                'tx_fault'    => $p11->failureFlagTx[$i],
                'rx_power_mw' => round($p11->opticalPowerRx[$i] * 0.0001, 4),
                'tx_bias_ma'  => $mult === null ? null : round($p11->laserBiasTx[$i] * 0.002 * $mult, 3),
                'tx_power_mw' => round($p11->opticalPowerTx[$i] * 0.0001, 4),
            ];
            $laneFlags[] = [
                'rx_power_high_alarm'   => $p11->opticalPowerHighAlarmFlagRx[$i],
                'rx_power_low_alarm'    => $p11->opticalPowerLowAlarmFlagRx[$i],
                'rx_power_high_warning' => $p11->opticalPowerHighWarningFlagRx[$i],
                'rx_power_low_warning'  => $p11->opticalPowerLowWarningFlagRx[$i],
                'tx_bias_high_alarm'    => $p11->laserBiasHighAlarmFlagTx[$i],
                'tx_bias_low_alarm'     => $p11->laserBiasLowAlarmFlagTx[$i],
                'tx_bias_high_warning'  => $p11->laserBiasHighWarningFlagTx[$i],
                'tx_bias_low_warning'   => $p11->laserBiasLowWarningFlagTx[$i],
                'tx_power_high_alarm'   => $p11->opticalPowerHighAlarmFlagTx[$i],
                'tx_power_low_alarm'    => $p11->opticalPowerLowAlarmFlagTx[$i],
                'tx_power_high_warning' => $p11->opticalPowerHighWarningFlagTx[$i],
                'tx_power_low_warning'  => $p11->opticalPowerLowWarningFlagTx[$i],
            ];
        }

        $flags = [
            'data_not_ready'     => null,
            'interrupt_asserted' => (ord($lower[3]) & (1 << 0)) === 0,
            'temp_high_alarm'    => (ord($lower[9]) & (1 << 0)) !== 0,
            'temp_low_alarm'     => (ord($lower[9]) & (1 << 1)) !== 0,
            'supply_high_alarm'  => (ord($lower[9]) & (1 << 4)) !== 0,
            'supply_low_alarm'   => (ord($lower[9]) & (1 << 5)) !== 0,
            'lanes'              => $laneFlags,
        ];

        return [$lanes, $flags];
    }
}
