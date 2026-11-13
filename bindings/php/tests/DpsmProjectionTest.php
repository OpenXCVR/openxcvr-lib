<?php
declare(strict_types=1);

// Native tests for the generated revision-bound CMIS DPSM name projection
// (CmisDataPathState::forRevision, schema-owned).
//
// The expected tables below are an independent literal oracle transcribed
// from the spec tables (CMIS 3.0 § 1.7.7.1 Table 66; CMIS 4.0 Table 8-58;
// CMIS 5.3 § 8.10.1 / Table 8-84), not values derived from a decoder, the
// generated table, a raw adapter or runtime case conversion. null means "no
// name for this raw at this revision" — the caller renders its own
// Unknown<n> fallback.

use Openxcvr\Generated\Enums\CmisDataPathState;
use PHPUnit\Framework\TestCase;

class DpsmProjectionTest extends TestCase
{
    /** Expected name per raw nibble at CMIS 3.0 (Table 66: raws 5-7 reserved). */
    private const V3_0 = [
        'Reserved0', 'DpDeactivated', 'DpInit', 'DpDeinit', 'DpActivated',
        null, null, null, null, null, null, null, null, null, null, null,
    ];

    /** Expected name per raw nibble at every catalog revision from 4.0 on. */
    private const V4_0_PLUS = [
        'Reserved0', 'DpDeactivated', 'DpInit', 'DpDeinit', 'DpActivated',
        'DpTxTurnOn', 'DpTxTurnOff', 'DpInitialized',
        null, null, null, null, null, null, null, null,
    ];

    /** The seven known CMIS context bytes (packed-BCD major/minor). */
    private const CATALOG_BYTES = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54];

    private static function oracle(int $rev): array
    {
        return $rev === 0x30 ? self::V3_0 : self::V4_0_PLUS;
    }

    public function testFactoryDomainIsExactlyTheLiteralCatalog(): void
    {
        for ($byte = 0; $byte <= 255; $byte++) {
            $bound = CmisDataPathState::forRevision($byte) !== null;
            $this->assertSame(in_array($byte, self::CATALOG_BYTES, true), $bound, sprintf('byte 0x%02x', $byte));
        }
    }

    /** 112 exact-string cases: every catalog revision × every raw nibble. */
    public function testProjectionsMatchTheLiteralOracle(): void
    {
        $cases = 0;
        foreach (self::CATALOG_BYTES as $rev) {
            $project = CmisDataPathState::forRevision($rev);
            $this->assertNotNull($project, sprintf('catalog byte 0x%02x must be bound', $rev));
            foreach (self::oracle($rev) as $raw => $want) {
                $this->assertSame($want, $project($raw), sprintf('rev 0x%02x raw 0x%02x', $rev, $raw));
                $cases++;
            }
        }
        $this->assertSame(112, $cases);
    }

    public function testUnsupportedContextsRefuse(): void
    {
        foreach ([0x31, 0x41, 0x55, 0x00, 0xFF] as $byte) {
            $this->assertNull(CmisDataPathState::forRevision($byte), sprintf('byte 0x%02x is not a known context', $byte));
        }
    }

    /** -1, 256 and 0x130 must refuse; none may alias 0x30 or any entry. */
    public function testOutOfRangeRevisionArgumentsRefuseWithoutAliasing(): void
    {
        foreach ([-1, 256, 0x130] as $bad) {
            $this->assertNull(CmisDataPathState::forRevision($bad), var_export($bad, true));
        }
    }

    /** Raw is not masked or coerced: out-of-byte raws have no name. */
    public function testOutOfRangeRawArgumentsNeverAlias(): void
    {
        foreach (self::CATALOG_BYTES as $rev) {
            $project = CmisDataPathState::forRevision($rev);
            $this->assertNotNull($project);
            foreach ([-1, 256, 0x130] as $bad) {
                $this->assertNull($project($bad), sprintf('rev 0x%02x raw %d', $rev, $bad));
            }
        }
    }

    public function testUnnamedRawsHaveNoName(): void
    {
        foreach (self::CATALOG_BYTES as $rev) {
            $project = CmisDataPathState::forRevision($rev);
            $this->assertNotNull($project);
            foreach ([8, 9, 10, 11, 12, 13, 14, 15, 16, 255] as $raw) {
                $this->assertNull($project($raw), sprintf('raw 0x%02x must be unnamed at rev 0x%02x', $raw, $rev));
            }
        }
    }

    public function testInterleavedHandlesKeepIndependentSelections(): void
    {
        $v3 = CmisDataPathState::forRevision(0x30);
        $v5 = CmisDataPathState::forRevision(0x53);
        $this->assertNotNull($v3);
        $this->assertNotNull($v5);
        for ($raw = 0; $raw < 16; $raw++) {
            $this->assertSame(self::V3_0[$raw], $v3($raw), "3.0 handle at {$raw}");
            $this->assertSame(self::V4_0_PLUS[$raw], $v5($raw), "5.3 handle at {$raw}");
            // Interleave again to prove no cross-handle state.
            $this->assertSame(self::V3_0[$raw], $v3($raw), "3.0 handle repeat at {$raw}");
        }
        // A fresh binding of the same revision agrees with the held one.
        $again = CmisDataPathState::forRevision(0x30);
        $this->assertNotNull($again);
        $this->assertNull($again(0x05));
        $this->assertSame('DpTxTurnOn', $v5(0x05));
    }

    /**
     * Existing revision-blind raw labels/cases are unchanged: the eight
     * named cases keep their snake_case labels, the case list stays eight
     * long, and raws 8-15 stay unconstructible backed values.
     */
    public function testFlatEnumSemanticsRemainRevisionBlind(): void
    {
        $labels = [
            0 => 'reserved_0',
            1 => 'dp_deactivated',
            2 => 'dp_init',
            3 => 'dp_deinit',
            4 => 'dp_activated',
            5 => 'dp_tx_turn_on',
            6 => 'dp_tx_turn_off',
            7 => 'dp_initialized',
        ];
        $this->assertSame(8, count(CmisDataPathState::cases()));
        $this->assertSame(5, CmisDataPathState::DpTxTurnOn->value);
        foreach (CmisDataPathState::cases() as $case) {
            $this->assertSame($labels[$case->value], $case->label(), $case->name);
        }
        for ($raw = 8; $raw < 16; $raw++) {
            $this->assertNull(CmisDataPathState::tryFrom($raw), "raw {$raw} stays unconstructible");
        }
    }

    // ── Public normalizer behavior on the shared reserved fixture ────────
    //
    // These exercise the PUBLIC OpenxcvrDecoder::decodeCmis entrypoint (not
    // the projection factory) over the independently authored reserved
    // fixture's pages, built in memory — no corpus file is modified. Expected
    // non-label snapshot values come from the independently authored golden,
    // never from a decoder run.

    private const RESERVED_STEM = 'synthetic-cmis-3-0-qsfp-dd-dp-reserved';

    /** Every major-3/4/5 context byte outside the literal seven-byte catalog
     * (0x30, 0x40, 0x50..0x54): 0x31..0x3F, 0x41..0x4F, 0x55..0x5F. */
    public static function unsupportedContextBytesProvider(): array
    {
        $cases = [];
        foreach ([...range(0x31, 0x3F), ...range(0x41, 0x4F), ...range(0x55, 0x5F)] as $byte) {
            $cases[sprintf('ctx 0x%02x', $byte)] = [$byte];
        }
        return $cases;
    }

    private static function reservedPage(string $suffix): string
    {
        $repoRoot = __DIR__ . '/../../..';
        $path = $repoRoot . '/tests/fixtures/cmis/' . self::RESERVED_STEM . '.' . $suffix . '.bin';
        $data = @file_get_contents($path);
        if ($data === false || strlen($data) !== 128) {
            throw new RuntimeException("reserved fixture $suffix unavailable");
        }
        return $data;
    }

    private static function reservedGolden(): array
    {
        $repoRoot = __DIR__ . '/../../..';
        $raw = file_get_contents($repoRoot . '/tests/golden/cmis/' . self::RESERVED_STEM . '.json');
        $golden = json_decode($raw ?: '', true);
        if (!is_array($golden)) {
            throw new RuntimeException('reserved golden unavailable');
        }
        return $golden;
    }

    private static function contextPages(int $contextByte, int $raw): array
    {
        $lower = self::reservedPage('lower');
        $lower[1] = chr($contextByte);
        $p11 = self::reservedPage('page-11h');
        $state = chr($raw | ($raw << 4));
        for ($i = 0; $i < 4; $i++) {
            $p11[$i] = $state;
        }
        return [
            'lower' => $lower,
            'page00h' => self::reservedPage('page-00h'),
            'page01h' => self::reservedPage('page-01h'),
            'page11h' => $p11,
        ];
    }

    /** @dataProvider unsupportedContextBytesProvider */
    public function testUnsupportedPresentContextsKeepSnapshotAndUseUnknown(int $ctx): void
    {
        $golden = self::reservedGolden();
        foreach (range(0, 15) as $raw) {
            $pages = self::contextPages($ctx, $raw);
            $result = \Openxcvr\OpenxcvrDecoder::decodeCmis(
                $pages['lower'],
                $pages['page00h'],
                $pages['page01h'],
                $pages['page11h']
            );
            $this->assertSame('ok', $result['decode_status'], sprintf('ctx 0x%02x raw 0x%02x admission', $ctx, $raw));
            $this->assertIsArray($result['lanes']);
            $this->assertCount(8, $result['lanes']);
            $want = 'Unknown' . $raw;
            foreach ($result['lanes'] as $i => $lane) {
                $this->assertSame(
                    $want,
                    $lane['dp_state'],
                    sprintf('ctx 0x%02x raw 0x%02x lane %d', $ctx, $raw, $i + 1)
                );
                $this->assertSame($golden['lanes'][$i]['rx_power_mw'], $lane['rx_power_mw']);
                $this->assertSame($golden['lanes'][$i]['tx_bias_ma'], $lane['tx_bias_ma']);
                $this->assertSame($golden['lanes'][$i]['tx_power_mw'], $lane['tx_power_mw']);
            }
            // Non-label snapshot values: the exact values, types and lane
            // association of the independently authored golden are preserved,
            // apart from the device revision text; only associative-map
            // insertion order (the golden's JSON key order, which is not a
            // decoder output contract) is ignored below.
            $this->assertSame($golden['identity'], $result['identity']);
            $this->assertSame($golden['monitoring'], $result['monitoring']);
            $this->assertNull($result['applications']);
            $this->assertSame('paged', $result['module']['memory_model']);
            $this->assertSame($golden['module']['module_state'], $result['module']['module_state']);
            $this->assertNull($result['module']['module_type']);
            $this->assertSame(
                sprintf('%d.%d', $ctx >> 4, $ctx & 0xF),
                $result['module']['cmis_revision']
            );
            $this->assertSame($golden['flags']['interrupt_asserted'], $result['flags']['interrupt_asserted']);
            $goldenFlagsLanes = $golden['flags']['lanes'];
            $actualFlagsLanes = $result['flags']['lanes'];
            $this->assertSame(
                array_keys($goldenFlagsLanes),
                array_keys($actualFlagsLanes),
                sprintf('ctx 0x%02x raw 0x%02x flags lane shape/count/order', $ctx, $raw)
            );
            foreach ($goldenFlagsLanes as $i => $expectedMap) {
                $sortedExpected = $expectedMap;
                $sortedActual = $actualFlagsLanes[$i];
                ksort($sortedExpected);
                ksort($sortedActual);
                $this->assertSame(
                    $sortedExpected,
                    $sortedActual,
                    sprintf('ctx 0x%02x raw 0x%02x lane %d flags map', $ctx, $raw, $i + 1)
                );
            }
        }
    }

    public function testMissingPage11BehaviorIsUnchanged(): void
    {
        $lower = self::reservedPage('lower');
        $page00h = self::reservedPage('page-00h');
        $page01h = self::reservedPage('page-01h');
        $page11h = self::reservedPage('page-11h');

        // Paged module without Page 11h: existing null lane/flag blocks.
        $result = \Openxcvr\OpenxcvrDecoder::decodeCmis($lower, $page00h, $page01h);
        $this->assertSame('ok', $result['decode_status']);
        $this->assertNull($result['lanes']);
        $this->assertNull($result['flags']);

        // Flat-memory module: null blocks even with Page 11h bytes supplied.
        $flat = $lower;
        $flat[2] = chr(ord($flat[2]) | 0x80);
        $result = \Openxcvr\OpenxcvrDecoder::decodeCmis($flat, $page00h, $page01h, $page11h);
        $this->assertSame('ok', $result['decode_status']);
        $this->assertNull($result['lanes']);
        $this->assertNull($result['flags']);

        // Page 11h present without Page 01h: existing explicit prerequisite error.
        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessage('page 01h missing');
        \Openxcvr\OpenxcvrDecoder::decodeCmis($lower, $page00h, '', $page11h);
    }
}
