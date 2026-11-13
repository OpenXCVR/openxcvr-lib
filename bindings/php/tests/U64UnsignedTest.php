<?php
declare(strict_types=1);

require_once __DIR__ . '/support/U64LeRoundTrip.php';
require_once __DIR__ . '/support/U64BeRoundTrip.php';

use PHPUnit\Framework\TestCase;
use Openxcvr\Generated\CmisPage14hDiagnosticsResults;
use Openxcvr\Generated\U64BeRoundTrip;
use Openxcvr\Generated\U64LeRoundTrip;

/** u64le/u64be decode and write as exact unsigned decimal strings, including >= 2^63. */
class U64UnsignedTest extends TestCase
{
    public function testPage14hDecodesUnsignedLittleEndian(): void
    {
        $high = str_repeat("\0", 128);
        // HostSideErrorCountLane1 is flat byte 192, file offset 64, little-endian.
        $high[64] = "\x01";
        $high[71] = "\x80";
        $page = CmisPage14hDiagnosticsResults::decode($high);
        $this->assertSame('9223372036854775809', $page->hostSideErrorCountLane14[0]);

        $max = str_repeat("\0", 128);
        for ($i = 64; $i < 72; $i++) {
            $max[$i] = "\xff";
        }
        $page = CmisPage14hDiagnosticsResults::decode($max);
        $this->assertSame('18446744073709551615', $page->hostSideErrorCountLane14[0]);
    }

    public function testLittleEndianRoundTrip(): void
    {
        $this->roundTrip(true);
        $buf = str_repeat("\0", 8);
        U64LeRoundTrip::setCounter($buf, '9223372036854775808');
        $this->assertSame("\x00", $buf[0]);
        $this->assertSame("\x80", $buf[7]);
    }

    public function testBigEndianRoundTrip(): void
    {
        $this->roundTrip(false);
        $buf = str_repeat("\0", 8);
        U64BeRoundTrip::setCounter($buf, '9223372036854775808');
        $this->assertSame("\x80", $buf[0]);
        $this->assertSame("\x00", $buf[7]);
    }

    public function testWriterRejectsOutOfRangeInput(): void
    {
        foreach (['', '-1', '18446744073709551616', '12a', ' 1', '+1', '1.0'] as $bad) {
            foreach ([U64LeRoundTrip::class, U64BeRoundTrip::class] as $class) {
                $buf = str_repeat("\0", 8);
                try {
                    $class::setCounter($buf, $bad);
                    $this->fail("$class accepted $bad");
                } catch (\InvalidArgumentException $e) {
                    $this->assertNotSame('', $e->getMessage());
                }
            }
        }
    }

    private function roundTrip(bool $little): void
    {
        foreach (['0', '9223372036854775807', '9223372036854775808', '18446744073709551615'] as $value) {
            $buf = str_repeat("\0", 8);
            if ($little) {
                U64LeRoundTrip::setCounter($buf, $value);
                $this->assertSame($value, U64LeRoundTrip::decode($buf)->counter);
            } else {
                U64BeRoundTrip::setCounter($buf, $value);
                $this->assertSame($value, U64BeRoundTrip::decode($buf)->counter);
            }
        }
    }
}
