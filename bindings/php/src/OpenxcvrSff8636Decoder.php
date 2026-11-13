<?php
declare(strict_types=1);
namespace Openxcvr;

use Openxcvr\Generated\Sff8636LowerMemory;
use Openxcvr\Generated\Sff8636Page00hUpperAdministrative;

trait OpenxcvrSff8636Decoder
{
    private static array $SFF8636_EXT_SPEC = [
        0x02 => 'base_100g_sr4_or_25g_sr',
    ];

    public static function decodeSff8636(string $lower, string $page00h): array
    {
        $lowerDecoded = Sff8636LowerMemory::decode($lower);
        $page00hDecoded = Sff8636Page00hUpperAdministrative::decode($page00h);

        $extSpec = null;
        if ($page00hDecoded->extSpecCompliance !== 0) {
            $extSpec = [
                'name' => self::$SFF8636_EXT_SPEC[$page00hDecoded->extSpecCompliance] ?? 'unknown',
                'raw'  => self::hexByte($page00hDecoded->extSpecCompliance),
            ];
        }

        $bitsSet = [];
        if ($page00hDecoded->complianceExtended) {
            $bitsSet[] = 'compliance_extended';
        }
        if ($page00hDecoded->compliance40gbaseSr4) {
            $bitsSet[] = 'compliance_40gbase_sr4';
        }

        $moduleType = null;
        if ($page00hDecoded->extSpecCompliance === 0x02) {
            $moduleType = '100GBASE-SR4 QSFP28';
        } elseif ($page00hDecoded->compliance40gbaseSr4) {
            $moduleType = '40GBASE-SR4 QSFP+';
        }

        $nominalRate = $page00hDecoded->nominalSignalingRate === 0xFF
            ? $page00hDecoded->extBaudRate / 4
            : $page00hDecoded->nominalSignalingRate / 10;

        $anyInterruptFlag = in_array(true, [
            ...$lowerDecoded->txLosFlag,
            ...$lowerDecoded->rxLosFlag,
            ...$lowerDecoded->rxPowerAlarmWarningFlag,
            ...$lowerDecoded->txBiasAlarmWarningFlag,
            ...$lowerDecoded->txPowerAlarmWarningFlag,
        ], true);

        $lanes = [];
        for ($i = 0; $i < 4; $i++) {
            $lanes[] = [
                'index'       => $i + 1,
                'rx_power_mw' => round($lowerDecoded->rxPowerLane[$i] * 0.0001, 4),
                'tx_bias_ma'  => round($lowerDecoded->txBiasLane[$i] * 0.002, 3),
                'tx_power_mw' => round($lowerDecoded->txPowerLane[$i] * 0.0001, 4),
            ];
        }

        $extendedIdentifier =
            $page00hDecoded->powerClassLow
            | ($page00hDecoded->powerClassHigh << 6)
            | ($page00hDecoded->rxCdrPresent ? (1 << 2) : 0)
            | ($page00hDecoded->txCdrPresent ? (1 << 3) : 0)
            | ($page00hDecoded->cleiPresent ? (1 << 4) : 0)
            | ($page00hDecoded->powerClass8Implemented ? (1 << 5) : 0);

        return [
            'decode_status' => 'ok',
            'identity'      => [
                'date_code'   => $page00hDecoded->dateCodeYear . $page00hDecoded->dateCodeMonth . $page00hDecoded->dateCodeDay,
                'vendor_name' => $page00hDecoded->vendorName,
                'vendor_oui'  => self::oui($page00hDecoded->vendorOui),
                'vendor_pn'   => $page00hDecoded->vendorPn,
                'vendor_rev'  => $page00hDecoded->vendorRev,
                'vendor_sn'   => $page00hDecoded->vendorSn,
            ],
            'module'        => [
                'cmis_revision'          => null,
                'connector'              => ['name' => self::lookupConnector($page00hDecoded->connectorType), 'raw' => self::hexByte($page00hDecoded->connectorType)],
                'encoding'               => ['name' => self::lookupEncoding($page00hDecoded->encoding), 'raw' => self::hexByte($page00hDecoded->encoding)],
                'extended_identifier'    => self::hexByte($extendedIdentifier),
                'identifier'             => ['name' => self::lookupIdentifier($lowerDecoded->sff8024Identifier), 'raw' => self::hexByte($lowerDecoded->sff8024Identifier)],
                'memory_model'           => $lowerDecoded->flatMemory ? 'flat' : 'paged',
                'module_state'           => null,
                'module_type'            => $moduleType,
                'nominal_bit_rate_gbps'  => $nominalRate,
            ],
            'monitoring'    => [
                'module_temperature_c' => $lowerDecoded->temperature / 256.0,
                'module_voltage_v'     => round($lowerDecoded->supplyVoltage * 0.0001, 4),
            ],
            'compliance'    => [
                'bits_set'               => $bitsSet,
                'device_technology'      => ['name' => self::lookupDeviceTechnology($page00hDecoded->deviceTechnology), 'raw' => sprintf('0x%X', $page00hDecoded->deviceTechnology)],
                'ext_spec_byte_192'      => $extSpec,
                'raw_bytes_page_00h_3_10' => self::hexBytes(substr($page00h, 3, 8)),
                'reach'                  => [
                    'om1_m'          => $page00hDecoded->lengthOm1OrCopperAttenuation25p78ghz,
                    'om2_m'          => $page00hDecoded->lengthOm21m,
                    'om3_2m_units'   => $page00hDecoded->lengthOm32m * 2,
                    'om4_or_copper_m' => $page00hDecoded->lengthCableOrOm4,
                    'smf_km'         => $page00hDecoded->lengthSmfKm,
                ],
            ],
            'flags'         => [
                'data_not_ready'     => $lowerDecoded->dataNotReady,
                'interrupt_asserted' => !$lowerDecoded->interruptDeasserted || $anyInterruptFlag,
            ],
            'lanes'         => $lanes,
            'checksums'     => [
                'cc_base' => self::checksum($page00h, 0, 63, 63),
                'cc_ext'  => self::checksum($page00h, 64, 95, 95),
            ],
        ];
    }
}
