<?php
declare(strict_types=1);
namespace Openxcvr;

use Openxcvr\Generated\page_a0h_base_memory;
use Openxcvr\Generated\page_a2h_diagnostics_base;

trait OpenxcvrSff8472Decoder
{
    public static function decodeSff8472(string $a0h, string $a2h): array
    {
        $a0h = self::padPage($a0h, 256);
        $a2h = self::padPage($a2h, 256);
        $a0hDecoded = page_a0h_base_memory::decode($a0h);
        $a2hDecoded = page_a2h_diagnostics_base::decode($a2h);
        $moduleType = $a0hDecoded->transceiverCompliance[3] ? '10GBASE-SR SFP+' : null;
        $flags = $a2hDecoded->alarmWarningFlag;

        // SFF-8472 v12.5a § 5.6: nominal signaling rate byte 12 == 0xFF is a
        // sentinel for rates > 25.4 GBd; the high-resolution rate is in byte 66
        // (250 MBd units → divide by 4 to express in GBd).
        $nominalBitRate = $a0hDecoded->signalingRateNominal === 0xFF
            ? $a0hDecoded->signalingRateMax / 4
            : $a0hDecoded->signalingRateNominal / 10;

        // SFF-8472 § 8.8: DDM (A2h real-time monitoring) is optional. When byte
        // 92 bit 6 (digitalDiagnosticsImplemented) is clear, A2h holds no
        // meaningful monitoring/flag/lane data — emit null rather than
        // zero-filled structures.
        $ddm = $a0hDecoded->digitalDiagnosticsImplemented;

        $flagsOut = null;
        $monitoringOut = null;
        $lanesOut = null;
        if ($ddm) {
            $flagsOut = [
                'alarm_summary'   => [
                    'rx_power_high_alarm' => $flags[8],
                    'rx_power_low_alarm'  => $flags[9],
                    'temp_high_alarm'     => $flags[0],
                    'temp_low_alarm'      => $flags[1],
                    'tx_bias_high_alarm'  => $flags[4],
                    'tx_bias_low_alarm'   => $flags[5],
                    'tx_power_high_alarm' => $flags[6],
                    'tx_power_low_alarm'  => $flags[7],
                    'vcc_high_alarm'      => $flags[2],
                    'vcc_low_alarm'       => $flags[3],
                ],
                'warning_summary' => [
                    'rx_power_high_warning' => $flags[24],
                    'rx_power_low_warning'  => $flags[25],
                    'temp_high_warning'     => $flags[16],
                    'temp_low_warning'      => $flags[17],
                    'tx_bias_high_warning'  => $flags[20],
                    'tx_bias_low_warning'   => $flags[21],
                    'tx_power_high_warning' => $flags[22],
                    'tx_power_low_warning'  => $flags[23],
                    'vcc_high_warning'      => $flags[18],
                    'vcc_low_warning'       => $flags[19],
                ],
            ];
            $monitoringOut = [
                'module_temperature_c' => $a2hDecoded->temperature / 256.0,
                'module_voltage_v'     => round($a2hDecoded->supplyVoltage * 0.0001, 4),
            ];
            $lanesOut = [[
                'index'       => 1,
                'rx_los'      => $a2hDecoded->rxLosState,
                'rx_power_mw' => round($a2hDecoded->rxPower * 0.0001, 4),
                'tx_bias_ma'  => round($a2hDecoded->txBias * 0.002, 3),
                'tx_disable'  => $a2hDecoded->txDisableState,
                'tx_fault'    => $a2hDecoded->txFaultState,
                'tx_power_mw' => round($a2hDecoded->txPower * 0.0001, 4),
            ]];
        }

        return [
            'decode_status' => 'ok',
            'identity'      => [
                'date_code'   => substr($a0hDecoded->dateCode, 0, 6),
                'vendor_name' => $a0hDecoded->vendorName,
                'vendor_oui'  => self::oui($a0hDecoded->vendorOui),
                'vendor_pn'   => $a0hDecoded->vendorPn,
                'vendor_sn'   => $a0hDecoded->vendorSn,
            ],
            'module'        => [
                'calibration'           => [
                    'ddm_implemented'       => $a0hDecoded->digitalDiagnosticsImplemented,
                    'externally_calibrated' => $a0hDecoded->externallyCalibrated,
                    'internally_calibrated' => $a0hDecoded->internallyCalibrated,
                    'rx_power_kind'         => $a0hDecoded->rxPowerAverage ? 'average' : 'oma',
                ],
                'cmis_revision'        => null,
                'connector'            => ['name' => self::lookupConnector($a0hDecoded->connectorType), 'raw' => self::hexByte($a0hDecoded->connectorType)],
                'encoding'             => ['name' => self::lookupEncoding($a0hDecoded->encoding), 'raw' => self::hexByte($a0hDecoded->encoding)],
                'identifier'           => ['name' => self::lookupIdentifier($a0hDecoded->sff8024Identifier), 'raw' => self::hexByte($a0hDecoded->sff8024Identifier)],
                'memory_model'         => null,
                'module_state'         => null,
                'module_type'          => $moduleType,
                'nominal_bit_rate_gbps' => $nominalBitRate,
            ],
            'monitoring'    => $monitoringOut,
            'compliance'    => [
                'raw_bytes_a0h_3_10' => self::hexBytes(substr($a0h, 3, 8)),
                'reach'              => [
                    'om1_m' => $a0hDecoded->lengthOm110m * 10,
                    'om2_m' => $a0hDecoded->lengthOm210m * 10,
                    // SFF-8472 byte 19 is OM3 length in units of 10 m (classic flat byte;
                    // base|multiplier<<6 == the raw byte). ×10 yields meters. Deliberate:
                    // the schema's base(u6)+multiplier(u2 enum) split is not the wire
                    // semantics — finisar (byte19=30, golden=300) falsifies the ×0.1
                    // reading. Do not "fix" to a fractional multiplier.
                    'om3_m' => ($a0hDecoded->lengthOm3Base | ($a0hDecoded->lengthOm3Multiplier << 6)) * 10,
                    'om4_m' => $a0hDecoded->lengthOm4OrCopper1m,
                    'smf_km' => $a0hDecoded->lengthSmfKm,
                ],
                'wavelength_nm'      => $a0hDecoded->wavelengthOrCableCompliance,
            ],
            'flags'         => $flagsOut,
            'lanes'         => $lanesOut,
            'checksums'     => [
                'cc_base' => self::checksum($a0h, 0, 63, 63),
                'cc_dmi'  => self::checksum($a2h, 0, 95, 95),
                'cc_ext'  => self::checksum($a0h, 64, 95, 95),
            ],
        ];
    }
}
