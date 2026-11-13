package com.openxcvr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
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
    private static final BigInteger ASYMMETRIC = new BigInteger("18364758544493064720");
    private static final BigInteger LANE0 = new BigInteger("578437695752307201");
    private static final byte[] LE_WIRE =
            new byte[] {
                0x10, 0x32, 0x54, 0x76, (byte) 0x98, (byte) 0xBA, (byte) 0xDC, (byte) 0xFE
            };
    private static final byte[] BE_WIRE =
            new byte[] {
                (byte) 0xFE, (byte) 0xDC, (byte) 0xBA, (byte) 0x98, 0x76, 0x54, 0x32, 0x10
            };

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

    @Test
    void asymmetricLittleEndianBytes() {
        assertEquals(0, ASYMMETRIC.compareTo(U64LeRoundTrip.decode(LE_WIRE).counter));
        byte[] buf = new byte[8];
        U64LeRoundTrip.setCounter(buf, ASYMMETRIC);
        assertArrayEquals(LE_WIRE, buf);
    }

    @Test
    void asymmetricBigEndianBytes() {
        assertEquals(0, ASYMMETRIC.compareTo(U64BeRoundTrip.decode(BE_WIRE).counter));
        byte[] buf = new byte[8];
        U64BeRoundTrip.setCounter(buf, ASYMMETRIC);
        assertArrayEquals(BE_WIRE, buf);
    }

    @Test
    void page14hLaneElementsAreIndependent() {
        byte[] page = new byte[128];
        page[64] = 0x01;
        page[65] = 0x02;
        page[66] = 0x03;
        page[67] = 0x04;
        page[68] = 0x05;
        page[69] = 0x06;
        page[70] = 0x07;
        page[71] = 0x08;
        page[80] = 0x10;
        page[81] = 0x32;
        page[82] = 0x54;
        page[83] = 0x76;
        page[84] = (byte) 0x98;
        page[85] = (byte) 0xBA;
        page[86] = (byte) 0xDC;
        page[87] = (byte) 0xFE;
        CmisPage14hDiagnosticsResults decoded = CmisPage14hDiagnosticsResults.decode(page);
        assertEquals(0, LANE0.compareTo(decoded.hostSideErrorCountLane14[0]));
        assertEquals(0, ASYMMETRIC.compareTo(decoded.hostSideErrorCountLane14[1]));
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
