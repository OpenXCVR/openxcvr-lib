<?php
declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use Openxcvr\OpenxcvrDecoder;

class ApplicationNumberingTest extends TestCase
{
    private const EXPECTED_HIF_0x53 = [81, 82, 79, 80, 77, 78, 75, 76, 81, 82, 79, 80, 77, 78, 75];
    private const REVISIONS = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53];

    private function loadZRBuffers(): array
    {
        $repoRoot = __DIR__ . '/../../..';
        $fixtureDir = "$repoRoot/tests/fixtures/cmis";
        $stem = 'fboss-cmis-5-3-osfp-800g-zr';

        $lower = (string) file_get_contents("$fixtureDir/$stem.lower.bin");
        $page00 = (string) file_get_contents("$fixtureDir/$stem.page-00h.bin");
        $page01 = (string) file_get_contents("$fixtureDir/$stem.page-01h.bin");

        return [$lower, $page00, $page01];
    }

    private function getAppCodes(array $result): array
    {
        $apps = $result['applications'] ?? [];
        if (!is_array($apps)) {
            return [];
        }
        $codes = [];
        foreach ($apps as $app) {
            if (isset($app['app_code'])) {
                $codes[] = (int) $app['app_code'];
            }
        }
        return $codes;
    }

    private function getAppHIFs(array $result): array
    {
        $apps = $result['applications'] ?? [];
        if (!is_array($apps)) {
            return [];
        }
        $hifs = [];
        foreach ($apps as $app) {
            if (isset($app['host_interface_id'])) {
                $hifs[] = (int) $app['host_interface_id'];
            }
        }
        return $hifs;
    }

    public function testRevisionsAdditive(): void
    {
        // (a) For each Lower:1 revision byte 0x30..0x53: app codes [1..15], and for 0x53 HIF matches.
        [$lowerOrig, $page00, $page01] = $this->loadZRBuffers();
        $expectedCodes = range(1, 15);

        foreach (self::REVISIONS as $rev) {
            $lower = $lowerOrig;
            $lower[1] = chr($rev);

            $result = OpenxcvrDecoder::decodeCmis($lower, $page00, $page01);
            $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

            $codes = $this->getAppCodes($resultNorm);
            $this->assertSame($expectedCodes, $codes, sprintf('Revision 0x%02X app codes mismatch', $rev));

            if ($rev === 0x53) {
                $hifs = $this->getAppHIFs($resultNorm);
                $this->assertSame(self::EXPECTED_HIF_0x53, $hifs, 'Revision 0x53 host_interface_id mismatch');
            }
        }
    }

    public function testPage01hNotSupplied(): void
    {
        // (b) Page 01h not supplied: app codes exactly [1..8].
        [$lower, $page00] = $this->loadZRBuffers();

        $result = OpenxcvrDecoder::decodeCmis($lower, $page00, '');
        $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

        $codes = $this->getAppCodes($resultNorm);
        $this->assertSame(range(1, 8), $codes);
    }

    public function testLowerSlot3Terminator(): void
    {
        // (c) Lower slot 3 HIF set to 0xFF (terminator): Lower yields [1,2], Page 01h still yields [9..15].
        // Preservation of existing facade behaviour, not normative spec rules (design-decision A3).
        [$lower, $page00, $page01] = $this->loadZRBuffers();
        // Lower slot 3 descriptor is at bytes 86 + 2*4 = 94. Byte 94 is HIF.
        $lower[94] = "\xFF";

        $result = OpenxcvrDecoder::decodeCmis($lower, $page00, $page01);
        $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

        $codes = $this->getAppCodes($resultNorm);
        $expectedCodes = array_merge([1, 2], range(9, 15));
        $this->assertSame($expectedCodes, $codes);
    }

    public function testPage01hAllMIFZero(): void
    {
        // (d) Every Page 01h descriptor MIF set to 0: app codes [1..8] (MIF-0 skip preserved).
        // Preservation of existing facade behaviour, not normative spec rules (design-decision A3).
        [$lower, $page00, $page01] = $this->loadZRBuffers();
        // Page 01h has 7 descriptors at buffer offsets 95-122.
        // Each descriptor is 4 bytes: [HIF, MIF, ...]. MIF is offset + 1.
        for ($i = 0; $i < 7; $i++) {
            $mifOffset = 95 + $i * 4 + 1;
            $page01[$mifOffset] = "\x00";
        }

        $result = OpenxcvrDecoder::decodeCmis($lower, $page00, $page01);
        $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

        $codes = $this->getAppCodes($resultNorm);
        $this->assertSame(range(1, 8), $codes);
    }
}
