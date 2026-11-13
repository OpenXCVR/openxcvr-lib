<?php
declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use Openxcvr\OpenxcvrDecoder;

class ParityTest extends TestCase
{
    public static function goldenProvider(): array
    {
        $repoRoot  = __DIR__ . '/../../..';
        $cases = [];

        foreach (['cmis', 'sff-8636', 'sff-8472'] as $spec) {
            $goldenDir = "$repoRoot/tests/golden/$spec";
            $fixtureDir = "$repoRoot/tests/fixtures/$spec";
            foreach (glob("$goldenDir/*.json") ?: [] as $path) {
                $stem = basename($path, '.json');
                if (str_starts_with($stem, '_')) {
                    continue;
                }
                $required = match ($spec) {
                    'cmis' => "$fixtureDir/$stem.lower.bin",
                    'sff-8636' => "$fixtureDir/$stem.lower.bin",
                    'sff-8472' => "$fixtureDir/$stem.a0h.bin",
                };
                if (!is_file($required)) {
                    throw new \RuntimeException("Golden $spec/$stem has no required capture: $required");
                }
                $cases["$spec/$stem"] = [$spec, $stem];
            }
        }

        if (count($cases) === 0) {
            throw new \RuntimeException("No parity fixtures found — check golden/fixture paths");
        }

        return $cases;
    }

    public function testSff8636InterruptSummaryUsesBooleanFlags(): void
    {
        $fixtureDir = __DIR__ . '/../../../tests/fixtures/sff-8636';
        $stem = 'finisar-ftl410qe3c-40g-sr4';
        $base = (string) file_get_contents("$fixtureDir/$stem.lower.bin");
        $page00h = (string) file_get_contents("$fixtureDir/$stem.page-00h.bin");
        self::assertSame(0x00, ord($base[1])); // Capture reports revision unspecified.

        // Preserve the facade's existing summary: reported IntL or a lane
        // LOS/power/bias flag. Cleared generated booleans are not integer zero.
        $flagBytes = [3, 9, 10, 11, 12, 13, 14];
        foreach ($flagBytes as $offset) {
            $base[$offset] = "\x00";
        }
        foreach ([false, true] as $deasserted) {
            $lower = $base;
            $lower[2] = chr((ord($base[2]) & ~0x02) | ($deasserted ? 0x02 : 0));
            $result = OpenxcvrDecoder::decodeSff8636($lower, $page00h);
            self::assertSame(!$deasserted, $result['flags']['interrupt_asserted']);
        }
        $base[2] = chr(ord($base[2]) | 0x02);
        foreach ($flagBytes as $offset) {
            foreach (range(0, 7) as $bit) {
                $lower = $base;
                $lower[$offset] = chr(1 << $bit);
                $result = OpenxcvrDecoder::decodeSff8636($lower, $page00h);
                self::assertTrue($result['flags']['interrupt_asserted'], "flag $offset.$bit");
            }
        }
    }

    public function testSff8472LaneStatusBitsAreIndependent(): void
    {
        $fixtureDir = __DIR__ . '/../../../tests/fixtures/sff-8472';
        $stem = 'finisar-ftlx8571d3bcl-mup0wb0';
        $a0h = (string) file_get_contents("$fixtureDir/$stem.a0h.bin");
        $base = (string) file_get_contents("$fixtureDir/$stem.a2h.bin");
        self::assertSame(0x03, ord($a0h[94])); // Capture reports SFF-8472 Rev 10.2.

        foreach (range(0, 7) as $bit) {
            $a2h = $base;
            $a2h[110] = chr(1 << $bit);
            $lane = OpenxcvrDecoder::decodeSff8472($a0h, $a2h)['lanes'][0];
            self::assertSame($bit === 1, $lane['rx_los'], "status bit $bit: rx_los");
            self::assertSame($bit === 2, $lane['tx_fault'], "status bit $bit: tx_fault");
            self::assertSame($bit === 7, $lane['tx_disable'], "status bit $bit: tx_disable");
        }
    }

    public function testCmisLaneStatusLatches(): void
    {
        // Pin the per-lane status latches (rx_los ← LOSFlagRx, Table 8-88;
        // tx_fault ← FailureFlagTx, Table 8-87) on the one fixture whose
        // Page 11h carries real latched values, independently of the deep
        // golden compare: fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched
        // on lanes 1/3 and tx_fault on lanes 2/4, everything else clear.
        $repoRoot   = __DIR__ . '/../../..';
        $fixtureDir = "$repoRoot/tests/fixtures/cmis";
        $stem = 'fboss-cmis-4-0-qsfp112-200g-fr4';
        $result = OpenxcvrDecoder::decodeCmis(
            (string) file_get_contents("$fixtureDir/$stem.lower.bin"),
            (string) file_get_contents("$fixtureDir/$stem.page-00h.bin"),
            (string) file_get_contents("$fixtureDir/$stem.page-01h.bin"),
            (string) file_get_contents("$fixtureDir/$stem.page-11h.bin"),
            self::loadVdmPages($fixtureDir, $stem),
        );
        $lanes = $result['lanes'];
        self::assertIsArray($lanes);
        self::assertCount(8, $lanes);
        foreach ($lanes as $i => $lane) {
            $idx = $i + 1;
            self::assertSame($idx === 1 || $idx === 3, $lane['rx_los'], "lane $idx rx_los");
            self::assertSame($idx === 2 || $idx === 4, $lane['tx_fault'], "lane $idx tx_fault");
        }
    }

    /** @dataProvider goldenProvider */
    public function testParity(string $spec, string $stem): void
    {
        $repoRoot   = __DIR__ . '/../../..';
        $fixtureDir = "$repoRoot/tests/fixtures/$spec";
        $goldenDir  = "$repoRoot/tests/golden/$spec";

        $golden = json_decode(
            (string) file_get_contents("$goldenDir/$stem.json"),
            true,
            512,
            JSON_THROW_ON_ERROR
        );

        $result = match ($spec) {
            'cmis' => OpenxcvrDecoder::decodeCmis(
                (string) file_get_contents("$fixtureDir/$stem.lower.bin"),
                file_exists("$fixtureDir/$stem.page-00h.bin")
                    ? (string) file_get_contents("$fixtureDir/$stem.page-00h.bin")
                    : str_repeat("\x00", 128),
                file_exists("$fixtureDir/$stem.page-01h.bin")
                    ? (string) file_get_contents("$fixtureDir/$stem.page-01h.bin")
                    : '',
                file_exists("$fixtureDir/$stem.page-11h.bin")
                    ? (string) file_get_contents("$fixtureDir/$stem.page-11h.bin")
                    : '',
                self::loadVdmPages($fixtureDir, $stem),
            ),
            'sff-8636' => OpenxcvrDecoder::decodeSff8636(
                (string) file_get_contents("$fixtureDir/$stem.lower.bin"),
                (string) file_get_contents("$fixtureDir/$stem.page-00h.bin"),
            ),
            'sff-8472' => OpenxcvrDecoder::decodeSff8472(
                (string) file_get_contents("$fixtureDir/$stem.a0h.bin"),
                (string) file_get_contents("$fixtureDir/$stem.a2h.bin"),
            ),
            default => throw new \RuntimeException("Unknown spec $spec"),
        };
        $resultNorm = json_decode((string) json_encode($result), true, 512, JSON_THROW_ON_ERROR);

        $skipKeys = $spec === 'cmis' ? ['_metadata', 'module_type'] : ['_metadata'];
        $diffs    = $this->diffRecursive($golden, $resultNorm, '', $skipKeys);

        $this->assertEmpty(
            $diffs,
            "Parity failure for $stem:\n" . implode("\n", $diffs)
        );
    }

    /**
     * Load a fixture's VDM page slices (CMIS § 8.27), keyed by page tag
     * '20h'..'2fh' for decodeCmis. 2Eh is not part of the VDM feature and has
     * no slice; the complete set gates the `vdm` block.
     *
     * @return array<string, string>
     */
    private static function loadVdmPages(string $fixtureDir, string $stem): array
    {
        $pages = [];
        foreach (
            ['20h', '21h', '22h', '23h', '24h', '25h', '26h', '27h',
             '28h', '29h', '2ah', '2bh', '2ch', '2dh', '2fh'] as $tag
        ) {
            $path = "$fixtureDir/$stem.page-$tag.bin";
            if (file_exists($path)) {
                $pages[$tag] = (string) file_get_contents($path);
            }
        }
        return $pages;
    }

    /**
     * @param array<string, mixed>|mixed $golden
     * @param array<string, mixed>|mixed $result
     * @param list<string> $skipKeys
     * @return list<string>
     */
    private function diffRecursive(mixed $golden, mixed $result, string $path, array $skipKeys): array
    {
        $diffs = [];

        if (is_array($golden) && array_is_list($golden)) {
            if (!is_array($result)) {
                return ["$path: type mismatch (golden=list, result=" . gettype($result) . ')'];
            }
            if (count($golden) !== count($result)) {
                $diffs[] = "$path: list length " . count($result) . ' != ' . count($golden);
            }
            foreach ($golden as $i => $gv) {
                if (isset($result[$i])) {
                    $diffs = [...$diffs, ...$this->diffRecursive($gv, $result[$i], "{$path}[$i]", $skipKeys)];
                }
            }
        } elseif (is_array($golden)) {
            if (!is_array($result)) {
                return ["$path: type mismatch (golden=map, result=" . gettype($result) . ')'];
            }
            $allKeys = array_unique([...array_keys($golden), ...array_keys($result)]);
            sort($allKeys);
            foreach ($allKeys as $k) {
                if (in_array($k, $skipKeys, true)) {
                    continue;
                }
                $child = $path !== '' ? "$path.$k" : $k;
                if (!array_key_exists($k, $result)) {
                    $diffs[] = "$child: missing from result (golden=" . json_encode($golden[$k]) . ')';
                } elseif (!array_key_exists($k, $golden)) {
                    $diffs[] = "$child: extra in result (result=" . json_encode($result[$k]) . ')';
                } else {
                    $diffs = [...$diffs, ...$this->diffRecursive($golden[$k], $result[$k], $child, $skipKeys)];
                }
            }
        } else {
            if (json_encode($golden) !== json_encode($result)) {
                $diffs[] = "$path: " . json_encode($result) . ' != ' . json_encode($golden);
            }
        }

        return $diffs;
    }
}
