<?php
declare(strict_types=1);

// Round-trip tests (encode-round-trip Wave 3, PHP): set<Field> is the exact
// inverse of decode; writing decoded values back is byte-identical.

use PHPUnit\Framework\TestCase;
use Openxcvr\Generated\CmisPage00hAdministrative as Admin;

class RoundTripTest extends TestCase
{
    private function fixture(): string
    {
        $p = __DIR__ . '/../../../tests/fixtures/cmis/fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin';
        return substr(file_get_contents($p), 0, 128);
    }

    public function testU8SetterIsDecodeInverse(): void
    {
        $buf = str_repeat("\0", 128);
        Admin::setMaxPower($buf, 42);
        $this->assertSame(42, Admin::decode($buf)->maxPower);
    }

    public function testWritingDecodedValuesBackIsIdentity(): void
    {
        $orig = $this->fixture();
        $d = Admin::decode($orig);
        $copy = $orig;
        Admin::setMaxPower($copy, $d->maxPower);
        Admin::setVendorName($copy, $d->vendorName);
        Admin::setVendorOui($copy, $d->vendorOui);
        Admin::setVendorPn($copy, $d->vendorPn);
        $this->assertSame($orig, $copy);
    }

    public function testBitFieldSetterPreservesNeighbors(): void
    {
        $buf = str_repeat("\xFF", 128);
        Admin::setMediaLaneUnsupportedLane1($buf, false);
        $this->assertSame(0xFE, ord($buf[82]));
        Admin::setMediaLaneUnsupportedLane2($buf, false);
        $this->assertSame(0xFC, ord($buf[82]));
    }
}
