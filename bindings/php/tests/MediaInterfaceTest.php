<?php
declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use Openxcvr\OpenxcvrDecoder;

class MediaInterfaceTest extends TestCase
{
    private const CASES = [
        [0x01, 0x01, 'base_10g_sw'],
        [0x01, 0x1C, 'fc_128g_mm'],
        [0x01, 0xC0, 'vendor_specific'],
        [0x02, 0x01, 'base_10g_lw'],
        [0x02, 0x1C, 'base_400g_dr4'],
        [0x02, 0x68, 'flexo_8e_dpo_16qam_foic8e_8_dpo'],
        [0x02, 0xC0, 'vendor_specific'],
        [0x02, 0xC1, 'vendor_specific'],
        [0x02, 0xF7, 'vendor_specific_f7'],
        [0x03, 0x01, 'copper_cable'],
        [0x03, 0x1C, 'reserved_02_be'],
        [0x03, 0xC0, 'linear_active_copper_loopback_module'],
        [0x03, 0xC1, 'vendor_specific'],
        [0x04, 0x01, 'active_cable_ber_1e_12'],
        [0x04, 0x1C, 'reserved_05_be'],
        [0x04, 0xF7, 'vendor_specific'],
        [0x05, 0x01, 'base_1000_t'],
        [0x05, 0xC1, 'vendor_specific'],
        [0x00, 0x1C, 'unknown'],
        [0x06, 0x1C, 'unknown'],
        [0x40, 0x1C, 'unknown'],
        [0x90, 0x1C, 'unknown'],
        [0xFF, 0x1C, 'unknown'],
    ];

    public static function casesProvider(): array
    {
        $out = [];
        foreach (self::CASES as $idx => [$mt, $mif, $exp]) {
            $name = sprintf('0x%02X_0x%02X_%s', $mt, $mif, $exp);
            $out[$name] = [$mt, $mif, $exp];
        }
        return $out;
    }

    /** @dataProvider casesProvider */
    public function testMediaInterfaceLowerCaller(int $mediaType, int $mif, string $expected): void
    {
        $repoRoot = __DIR__ . '/../../..';
        $fixtureDir = "$repoRoot/tests/fixtures/cmis";
        $stem = 'fboss-cmis-4-0-qsfpdd-400g-dr4';

        $lower = (string) file_get_contents("$fixtureDir/$stem.lower.bin");
        $page00 = file_exists("$fixtureDir/$stem.page-00h.bin")
            ? (string) file_get_contents("$fixtureDir/$stem.page-00h.bin")
            : str_repeat("\x00", 128);
        $page01 = file_exists("$fixtureDir/$stem.page-01h.bin")
            ? (string) file_get_contents("$fixtureDir/$stem.page-01h.bin")
            : '';
        $page11 = file_exists("$fixtureDir/$stem.page-11h.bin")
            ? (string) file_get_contents("$fixtureDir/$stem.page-11h.bin")
            : '';

        $lower[85] = chr($mediaType);
        $lower[87] = chr($mif);

        $result = OpenxcvrDecoder::decodeCmis($lower, $page00, $page01, $page11);
        $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

        $apps = $resultNorm['applications'] ?? [];
        $this->assertNotEmpty($apps, 'applications list should not be empty');
        $this->assertSame($expected, $apps[0]['media_interface_name']);
    }

    /** @dataProvider casesProvider */
    public function testMediaInterfacePage01Caller(int $mediaType, int $mif, string $expected): void
    {
        $repoRoot = __DIR__ . '/../../..';
        $fixtureDir = "$repoRoot/tests/fixtures/cmis";
        $stem = 'fboss-cmis-4-0-osfp-2x400g-dr4';

        $lower = (string) file_get_contents("$fixtureDir/$stem.lower.bin");
        $page00 = file_exists("$fixtureDir/$stem.page-00h.bin")
            ? (string) file_get_contents("$fixtureDir/$stem.page-00h.bin")
            : str_repeat("\x00", 128);
        $page01 = (string) file_get_contents("$fixtureDir/$stem.page-01h.bin");
        $page11 = file_exists("$fixtureDir/$stem.page-11h.bin")
            ? (string) file_get_contents("$fixtureDir/$stem.page-11h.bin")
            : '';

        $lower[85] = chr($mediaType);
        $page01[96] = chr($mif);

        $result = OpenxcvrDecoder::decodeCmis($lower, $page00, $page01, $page11);
        $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

        $apps = $resultNorm['applications'] ?? [];
        $entry = null;
        foreach ($apps as $a) {
            if (($a['app_code'] ?? null) === 9) {
                $entry = $a;
                break;
            }
        }

        $this->assertNotNull($entry, 'entry with app_code 9 should exist');
        $this->assertSame($expected, $entry['media_interface_name']);
    }
}
