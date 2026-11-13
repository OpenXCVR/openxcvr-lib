<?php
declare(strict_types=1);
namespace Openxcvr;

use Openxcvr\Generated\Enums\CmisMediaInterface;
use Openxcvr\Generated\Enums\Sff8024ConnectorType;
use Openxcvr\Generated\Enums\Sff8024Encoding;
use Openxcvr\Generated\Enums\Sff8024HostElectricalInterface;
use Openxcvr\Generated\Enums\Sff8024Identifier;
use Openxcvr\Generated\Enums\Sff8636DeviceTechnology;

trait OpenxcvrDecoderLookups
{
    // ─── lookup helpers ───────────────────────────────────────────────────────

    private static function lookupIdentifier(int $id): string
    {
        return Sff8024Identifier::tryFrom($id)?->label() ?? 'unknown';
    }

    private static function lookupConnector(int $id): string
    {
        return Sff8024ConnectorType::tryFrom($id)?->label() ?? 'unknown';
    }

    private static function lookupEncoding(int $id): string
    {
        return Sff8024Encoding::tryFrom($id)?->label() ?? 'unknown';
    }

    private static function lookupHostInterface(int $id): string
    {
        return Sff8024HostElectricalInterface::tryFrom($id)?->label() ?? 'unknown';
    }

    private static function lookupDeviceTechnology(int $id): string
    {
        return Sff8636DeviceTechnology::tryFrom($id)?->label() ?? 'unknown';
    }

    private static function hexByte(int $value): string
    {
        return sprintf('0x%02X', $value);
    }

    private static function checksum(string $buf, int $start, int $end, int $storedOffset): array
    {
        $computed = 0;
        for ($i = $start; $i < $end; $i++) {
            $computed = ($computed + ord($buf[$i])) & 0xFF;
        }
        $stored = ord($buf[$storedOffset]);
        return [
            'computed' => self::hexByte($computed),
            'stored'   => self::hexByte($stored),
            'valid'    => $computed === $stored,
        ];
    }

    private static function hexBytes(string $buf): array
    {
        $values = [];
        for ($i = 0; $i < strlen($buf); $i++) {
            $values[] = self::hexByte(ord($buf[$i]));
        }
        return $values;
    }

    private static function oui(string $buf): string
    {
        return sprintf('%02X:%02X:%02X', ord($buf[0]), ord($buf[1]), ord($buf[2]));
    }

    private static function padPage(string $buf, int $size): string
    {
        return strlen($buf) >= $size ? $buf : $buf . str_repeat("\x00", $size - strlen($buf));
    }

    // ─── app descriptor parsing ───────────────────────────────────────────────

    private static function parseAppDescriptors(
        string $buf,
        int $startOffset,
        int $count,
        int $baseCode,
        int $mediaType
    ): array {
        $entries = [];
        for ($i = 0; $i < $count; $i++) {
            $off = $startOffset + $i * 4;
            if ($off + 4 > strlen($buf)) {
                break;
            }
            $hif  = ord($buf[$off]);
            $mif  = ord($buf[$off + 1]);
            $hlc  = (ord($buf[$off + 2]) >> 4) & 0xF;
            $mlc  = ord($buf[$off + 2]) & 0xF;
            $hlao = ord($buf[$off + 3]);
            if ($hif === 0xFF) {
                break;
            }
            if ($mif === 0) {
                continue;
            }
            $code = $baseCode + $i + 1;
            $entries[] = [
                'app_code'                     => $code,
                'host_interface_id'            => $hif,
                'host_interface_name'          => self::lookupHostInterface($hif),
                'host_lane_count'              => $hlc,
                'host_lane_assignment_options' => sprintf('0x%02X', $hlao),
                'media_interface_id'           => $mif,
                'media_interface_name'         => CmisMediaInterface::nameOf($mediaType, $mif) ?? 'unknown',
                'media_lane_count'             => $mlc,
            ];
        }
        return $entries;
    }

    // ─── public API ───────────────────────────────────────────────────────────
}
