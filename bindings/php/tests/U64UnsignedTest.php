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

    public function testWriterRejectsNonDigitAndOutOfRangeAndLeavesBufferUnchanged(): void
    {
        foreach ([
            "5\n",
            "\n",
            "5\n5",
            " 5",
            "5 ",
            "+5",
            "-1",
            "0x10",
            "1e3",
            "",
            "18446744073709551616",
            "99999999999999999999",
        ] as $bad) {
            foreach ([U64LeRoundTrip::class, U64BeRoundTrip::class] as $class) {
                $buf = str_repeat("\0", 8);
                $before = $buf;
                try {
                    $class::setCounter($buf, $bad);
                    $this->fail($class . ' accepted ' . json_encode($bad));
                } catch (\InvalidArgumentException $e) {
                    $this->assertNotSame('', $e->getMessage());
                    $this->assertSame($before, $buf, $class . ' mutated the buffer for ' . json_encode($bad));
                }
            }
        }
    }

    public function testWriterAcceptsZeroAndLeadingZerosAtMaximum(): void
    {
        foreach ([U64LeRoundTrip::class, U64BeRoundTrip::class] as $class) {
            $buf = str_repeat("\x11", 8);
            $class::setCounter($buf, '0');
            $this->assertSame(str_repeat("\0", 8), $buf);

            $buf = str_repeat("\0", 8);
            $class::setCounter($buf, '00018446744073709551615');
            $this->assertSame(str_repeat("\xff", 8), $buf);
        }
    }

    public function testAsymmetricWireBytes(): void
    {
        $le = "\x10\x32\x54\x76\x98\xBA\xDC\xFE";
        $this->assertSame('18364758544493064720', U64LeRoundTrip::decode($le)->counter);
        $buf = str_repeat("\0", 8);
        U64LeRoundTrip::setCounter($buf, '18364758544493064720');
        $this->assertSame($le, $buf);

        $be = "\xFE\xDC\xBA\x98\x76\x54\x32\x10";
        $this->assertSame('18364758544493064720', U64BeRoundTrip::decode($be)->counter);
        $buf = str_repeat("\0", 8);
        U64BeRoundTrip::setCounter($buf, '18364758544493064720');
        $this->assertSame($be, $buf);
    }

    public function testPage14hLaneElementsAreIndependent(): void
    {
        $page = str_repeat("\0", 128);
        $page[64] = "\x01";
        $page[65] = "\x02";
        $page[66] = "\x03";
        $page[67] = "\x04";
        $page[68] = "\x05";
        $page[69] = "\x06";
        $page[70] = "\x07";
        $page[71] = "\x08";
        $page[80] = "\x10";
        $page[81] = "\x32";
        $page[82] = "\x54";
        $page[83] = "\x76";
        $page[84] = "\x98";
        $page[85] = "\xBA";
        $page[86] = "\xDC";
        $page[87] = "\xFE";
        $decoded = CmisPage14hDiagnosticsResults::decode($page);
        $this->assertSame('578437695752307201', $decoded->hostSideErrorCountLane14[0]);
        $this->assertSame('18364758544493064720', $decoded->hostSideErrorCountLane14[1]);
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
