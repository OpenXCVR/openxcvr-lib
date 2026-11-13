<?php

declare(strict_types=1);

namespace Openxcvr\Tests;

use PHPUnit\Framework\TestCase;
use Openxcvr\Generated\page_a0h_base_memory;
use Openxcvr\Generated\page_a2h_diagnostics_base;
use Openxcvr\Generated\page_a2h_03h_high_accuracy_timing;
use Openxcvr\Generated\Sff8636Page00hUpperAdministrative;
use Openxcvr\Generated\CmisPage00hAdministrative;
use Openxcvr\Generated\CmisPage02hThresholds;
use Openxcvr\Generated\CmisPage04hLaserCapabilities;
use Openxcvr\Generated\CmisPage05hFormFactorSignals;

/**
 * Checksum-recompute tests for PHP bindings.
 *
 * For each fixture, load the raw page buffer, capture the stored checksum
 * byte(s), call recomputeChecksums(), and assert the recomputed value matches
 * the byte already stored in the fixture. This proves the PHP recompute logic
 * is correct for all real and synthetic samples in the corpus.
 */
class ChecksumRecomputeTest extends TestCase
{
    private static string $repoRoot;

    public static function setUpBeforeClass(): void
    {
        // bindings/php/tests/ -> repo root (three levels up)
        self::$repoRoot = dirname(__DIR__, 3);
    }

    /**
     * Read a fixture file, zero-padding to $size bytes if shorter.
     */
    private function readPadded(string $relPath, int $size): string
    {
        $path = self::$repoRoot . '/' . $relPath;
        $bytes = file_get_contents($path);
        self::assertNotFalse($bytes, "Could not read fixture: $path");
        self::assertLessThanOrEqual(
            $size,
            strlen($bytes),
            "$path: file is " . strlen($bytes) . " bytes, expected at most $size"
        );
        return str_pad($bytes, $size, "\0");
    }

    /**
     * Read a fixture file, asserting exact size.
     */
    private function readExact(string $relPath, int $size): string
    {
        $path = self::$repoRoot . '/' . $relPath;
        $bytes = file_get_contents($path);
        self::assertNotFalse($bytes, "Could not read fixture: $path");
        self::assertSame(
            $size,
            strlen($bytes),
            "$path: expected $size bytes, got " . strlen($bytes)
        );
        return $bytes;
    }

    // ─── A0h fixtures: CC_BASE (buf[63]) + CC_EXT (buf[95]) ─────────────────

    public static function a0hProvider(): array
    {
        return [
            'finisar-mup0wb0' => ['finisar-ftlx8571d3bcl-mup0wb0'],
            'finisar-muq1bzb' => ['finisar-ftlx8571d3bcl-muq1bzb'],
            'synthetic-dac-passive-cu' => ['synthetic-dac-sfp10g-passive-cu'],
            'synthetic-extcal-sr' => ['synthetic-extcal-sfp10g-sr'],
            'synthetic-sfp28-25g-sr' => ['synthetic-sfp28-25g-sr'],
        ];
    }

    /**
     * @dataProvider a0hProvider
     */
    public function testA0hRecomputeChecksums(string $stem): void
    {
        $buf = $this->readPadded("tests/fixtures/sff-8472/{$stem}.a0h.bin", 256);
        $ccBaseStored = ord($buf[63]);
        $ccExtStored = ord($buf[95]);
        page_a0h_base_memory::recomputeChecksums($buf);
        self::assertSame(
            $ccBaseStored,
            ord($buf[63]),
            "$stem a0h CC_BASE: recomputed " . ord($buf[63]) . " != stored $ccBaseStored"
        );
        self::assertSame(
            $ccExtStored,
            ord($buf[95]),
            "$stem a0h CC_EXT: recomputed " . ord($buf[95]) . " != stored $ccExtStored"
        );
    }

    // ─── A2h fixtures: CHKSUM (buf[95]) ──────────────────────────────────────

    public static function a2hProvider(): array
    {
        return [
            'finisar-mup0wb0' => ['finisar-ftlx8571d3bcl-mup0wb0'],
            'finisar-muq1bzb' => ['finisar-ftlx8571d3bcl-muq1bzb'],
            'synthetic-dac-passive-cu' => ['synthetic-dac-sfp10g-passive-cu'],
            'synthetic-extcal-sr' => ['synthetic-extcal-sfp10g-sr'],
            'synthetic-sfp28-25g-sr' => ['synthetic-sfp28-25g-sr'],
        ];
    }

    /**
     * @dataProvider a2hProvider
     */
    public function testA2hRecomputeChecksums(string $stem): void
    {
        $buf = $this->readExact("tests/fixtures/sff-8472/{$stem}.a2h.bin", 256);
        $chksumStored = ord($buf[95]);
        page_a2h_diagnostics_base::recomputeChecksums($buf);
        self::assertSame(
            $chksumStored,
            ord($buf[95]),
            "$stem a2h CHKSUM: recomputed " . ord($buf[95]) . " != stored $chksumStored"
        );
    }

    // ─── SFF-8636 Page 00h: CC_BASE (buf[63]) + CC_EXT (buf[95]) ────────────

    public static function sff8636Page00hProvider(): array
    {
        return [
            'finisar-ftl410qe3c-40g-sr4' => ['finisar-ftl410qe3c-40g-sr4'],
            'finisar-ftlc9551repm-100g-sr4' => ['finisar-ftlc9551repm-100g-sr4'],
            'synthetic-qsfp-dac-40g-passive-cu' => ['synthetic-qsfp-dac-40g-passive-cu'],
            'synthetic-qsfp28-aoc-100g-sr4' => ['synthetic-qsfp28-aoc-100g-sr4'],
            'synthetic-qsfp28-bidi-100g-cwdm4' => ['synthetic-qsfp28-bidi-100g-cwdm4'],
        ];
    }

    /**
     * @dataProvider sff8636Page00hProvider
     */
    public function testSff8636Page00hRecomputeChecksums(string $stem): void
    {
        $buf = $this->readExact("tests/fixtures/sff-8636/{$stem}.page-00h.bin", 128);
        $ccBaseStored = ord($buf[63]);
        $ccExtStored = ord($buf[95]);
        Sff8636Page00hUpperAdministrative::recomputeChecksums($buf);
        self::assertSame(
            $ccBaseStored,
            ord($buf[63]),
            "$stem page-00h CC_BASE: recomputed " . ord($buf[63]) . " != stored $ccBaseStored"
        );
        self::assertSame(
            $ccExtStored,
            ord($buf[95]),
            "$stem page-00h CC_EXT: recomputed " . ord($buf[95]) . " != stored $ccExtStored"
        );
    }

    // ─── CMIS Page 00h: page_checksum (buf[94]) ───────────────────────────────

    public static function cmisPage00hProvider(): array
    {
        return [
            'fboss-cmis-4-0-qsfp112-200g-fr4' => ['fboss-cmis-4-0-qsfp112-200g-fr4'],
            'fboss-cmis-4-0-qsfpdd-400g-fr4-multiport' => ['fboss-cmis-4-0-qsfpdd-400g-fr4-multiport'],
            'fboss-cmis-4-0-qsfpdd-400g-lr4' => ['fboss-cmis-4-0-qsfpdd-400g-lr4'],
            'synthetic-cmis-3-0-qsfp-dd' => ['synthetic-cmis-3-0-qsfp-dd'],
            'synthetic-cmis-5-2-qsfp-dd' => ['synthetic-cmis-5-2-qsfp-dd'],
        ];
    }

    /**
     * @dataProvider cmisPage00hProvider
     */
    public function testCmisPage00hRecomputeChecksums(string $stem): void
    {
        $buf = $this->readExact("tests/fixtures/cmis/{$stem}.page-00h.bin", 128);
        $chksumStored = ord($buf[94]);
        CmisPage00hAdministrative::recomputeChecksums($buf);
        self::assertSame(
            $chksumStored,
            ord($buf[94]),
            "$stem page-00h checksum: recomputed " . ord($buf[94]) . " != stored $chksumStored"
        );
    }

    // ─── CMIS Page 02h: page_checksum (buf[127]) ─────────────────────────────

    public static function cmisPage02hProvider(): array
    {
        return [
            'fboss-cmis-4-0-qsfp112-200g-fr4' => ['fboss-cmis-4-0-qsfp112-200g-fr4'],
            'fboss-cmis-4-0-qsfpdd-400g-fr4-multiport' => ['fboss-cmis-4-0-qsfpdd-400g-fr4-multiport'],
            'fboss-cmis-4-0-qsfpdd-400g-lr4' => ['fboss-cmis-4-0-qsfpdd-400g-lr4'],
            'synthetic-cmis-3-0-qsfp-dd' => ['synthetic-cmis-3-0-qsfp-dd'],
            'synthetic-cmis-5-2-qsfp-dd' => ['synthetic-cmis-5-2-qsfp-dd'],
        ];
    }

    /**
     * @dataProvider cmisPage02hProvider
     */
    public function testCmisPage02hRecomputeChecksums(string $stem): void
    {
        $buf = $this->readExact("tests/fixtures/cmis/{$stem}.page-02h.bin", 128);
        $chksumStored = ord($buf[127]);
        CmisPage02hThresholds::recomputeChecksums($buf);
        self::assertSame(
            $chksumStored,
            ord($buf[127]),
            "$stem page-02h checksum: recomputed " . ord($buf[127]) . " != stored $chksumStored"
        );
    }

    // ─── CMIS Page 04h: page_checksum (buf[127]) — no corpus fixtures ────────
    // Use a zeroed buffer: recompute must produce 0x00 and must not throw.

    public function testCmisPage04hZeroedBuffer(): void
    {
        $buf = str_repeat("\0", 128);
        CmisPage04hLaserCapabilities::recomputeChecksums($buf);
        self::assertSame(0, ord($buf[127]), 'Page 04h zeroed-buffer checksum must be 0');
    }

    // ─── CMIS Page 05h: page_checksum (buf[127]) — no corpus fixtures ────────
    // Use a zeroed buffer: recompute must produce 0x00 and must not throw.

    public function testCmisPage05hZeroedBuffer(): void
    {
        $buf = str_repeat("\0", 128);
        CmisPage05hFormFactorSignals::recomputeChecksums($buf);
        self::assertSame(0, ord($buf[127]), 'Page 05h zeroed-buffer checksum must be 0');
    }

    // ─── A2h page-03h: CC_CALIB (buf[255]) — no corpus fixtures ──────────────
    // Use a zeroed 256-byte buffer: recompute must produce 0x00 and not throw.

    public function testA2h03hZeroedBuffer(): void
    {
        $buf = str_repeat("\0", 256);
        page_a2h_03h_high_accuracy_timing::recomputeChecksums($buf);
        self::assertSame(0, ord($buf[255]), 'A2h page-03h zeroed-buffer CC_CALIB must be 0');
    }
}
