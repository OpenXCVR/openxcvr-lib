package com.openxcvr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.openxcvr.generated.CmisPage14hDiagnosticsResults;
import com.openxcvr.generated.U64BeRoundTrip;
import com.openxcvr.generated.U64LeRoundTrip;
import java.math.BigInteger;
import org.junit.jupiter.api.Test;

/** u64le/u64be decode and write as exact unsigned values, including >= 2^63. */
class U64UnsignedTest {

    private static final BigInteger TWO_63_MINUS_ONE = new BigInteger("9223372036854775807");
    private static final BigInteger TWO_63 = new BigInteger("9223372036854775808");
    private static final BigInteger HIGH_BIT = new BigInteger("9223372036854775809");
    private static final BigInteger MAX = new BigInteger("18446744073709551615");

    @Test
    void page14hDecodesUnsignedLittleEndian() {
        byte[] high = new byte[128];
        // HostSideErrorCountLane1 is flat byte 192, file offset 64, little-endian.
        high[64] = 0x01;
        high[71] = (byte) 0x80;
        assertEquals(
                HIGH_BIT,
                CmisPage14hDiagnosticsResults.decode(high).hostSideErrorCountLane14[0]);

        byte[] max = new byte[128];
        for (int i = 64; i < 72; i++) {
            max[i] = (byte) 0xFF;
        }
        assertEquals(
                MAX, CmisPage14hDiagnosticsResults.decode(max).hostSideErrorCountLane14[0]);
    }

    @Test
    void littleEndianRoundTripAndRejection() {
        roundTrip(true);
        byte[] buf = new byte[8];
        U64LeRoundTrip.setCounter(buf, TWO_63);
        assertEquals((byte) 0x00, buf[0]);
        assertEquals((byte) 0x80, buf[7]);
        assertThrows(
                IllegalArgumentException.class,
                () -> U64LeRoundTrip.setCounter(buf, new BigInteger("-1")));
        assertThrows(
                IllegalArgumentException.class,
                () -> U64LeRoundTrip.setCounter(buf, new BigInteger("18446744073709551616")));
        assertThrows(IllegalArgumentException.class, () -> U64LeRoundTrip.setCounter(buf, null));
    }

    @Test
    void bigEndianRoundTripAndRejection() {
        roundTrip(false);
        byte[] buf = new byte[8];
        U64BeRoundTrip.setCounter(buf, TWO_63);
        assertEquals((byte) 0x80, buf[0]);
        assertEquals((byte) 0x00, buf[7]);
        assertThrows(
                IllegalArgumentException.class,
                () -> U64BeRoundTrip.setCounter(buf, new BigInteger("-1")));
        assertThrows(
                IllegalArgumentException.class,
                () -> U64BeRoundTrip.setCounter(buf, MAX.add(BigInteger.ONE)));
    }

    private static void roundTrip(boolean little) {
        BigInteger[] values = {BigInteger.ZERO, TWO_63_MINUS_ONE, TWO_63, MAX};
        for (BigInteger value : values) {
            byte[] buf = new byte[8];
            if (little) {
                U64LeRoundTrip.setCounter(buf, value);
                assertEquals(0, value.compareTo(U64LeRoundTrip.decode(buf).counter));
            } else {
                U64BeRoundTrip.setCounter(buf, value);
                assertEquals(0, value.compareTo(U64BeRoundTrip.decode(buf).counter));
            }
        }
    }
}
