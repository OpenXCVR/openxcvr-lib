// SFF-8472 page-padding tests (review follow-ups A1/A2): A0h/A2h pages
// shorter than 256 bytes are zero-padded like the Python facade's
// `_pad_page` (no 128-byte floor), a 256-byte page keeps every caller byte
// including 128-255, and a null page still throws. Every padded decode is
// pinned to the committed Python-reference output for the exact same bytes:
// the golden fixtures carry zero flag bytes, so these tests are the only
// place the alarm/warning flag mapping is discriminated (re-review B1).
using System.Text.Json;
using OpenXcvr.Generated;
using Xunit;

namespace OpenXcvr.Transceivers.Tests;

public class Sff8472PaddingTests
{
    // A0h truncated to its first 100 bytes (below the old 128-byte floor):
    // identifier/connector/compliance, vendor fields, the checksum bytes and
    // the DDM byte survive; bytes 100-255 must come back as zero padding.
    // Byte comments name the SFF-8472 field each write lands in.
    internal static byte[] ShortA0h()
    {
        var buf = new byte[100];
        buf[0] = 0x03; // identifier (byte 0): sfp_sff8472
        buf[1] = 0x04; // extended identifier (byte 1)
        buf[2] = 0x21; // connector (byte 2): copper_pigtail (SFF-8024 code 0x21)
        buf[3] = 0x03; // transceiver compliance (bytes 3-10; feeds compliance.raw_bytes_a0h_3_10)
        buf[4] = 0x04;
        buf[5] = 0x05;
        buf[6] = 0x06;
        buf[7] = 0x07;
        buf[8] = 0x08;
        buf[9] = 0x09;
        buf[10] = 0x0A;
        buf[12] = 0x64; // nominal signaling rate (byte 12): 0x64 / 10 = 10.0 Gbps
        WriteAscii(buf, 20, "ACMEWIDGETSP"); // vendor name (bytes 20-35; 12 chars + NUL pad)
        WriteAscii(buf, 36, "ABC-123 "); // ext spec compliance (byte 36) + vendor OUI (bytes 37-39 = 'B','C','-') + vendor PN start (bytes 40-43)
        WriteAscii(buf, 44, "A1      "); // vendor PN (bytes 40-55), middle 8 bytes
        WriteAscii(buf, 52, "SN000001        "); // vendor PN end (52-55) + vendor rev (56-59) + wavelength (60-61) + reserved (62) + cc_base (63, overwritten below)
        WriteAscii(buf, 68, "250113  "); // vendor SN (bytes 68-83; 8 chars + zero pad)
        buf[92] = 0x60; // monitor type (byte 92): bit 6 = DDM implemented, bit 5 = internally calibrated
        buf[63] = Sum(buf, 0, 63); // cc_base (byte 63): sum of bytes 0-62
        buf[95] = Sum(buf, 64, 95); // cc_ext (byte 95): sum of bytes 64-94
        return buf;
    }

    // Full 256-byte A2h: telemetry, one bit set in each flag-area byte, and a
    // valid cc_dmi checksum, so the padded-A0h decode has real lanes.
    internal static byte[] FullA2h()
    {
        var buf = new byte[256];
        buf[96] = 0x20; // temperature (bytes 96-97): 0x2000 / 256 = 32.0 °C
        buf[97] = 0x00;
        buf[98] = 0x81; // supply voltage (bytes 98-99): 0x8100 × 0.0001 = 3.3024 V
        buf[99] = 0x00;
        buf[100] = 0x27; // tx bias (bytes 100-101): 0x2710 × 0.002 = 20.0 mA
        buf[101] = 0x10;
        buf[102] = 0x04; // tx power (bytes 102-103): 0x0400 × 0.0001 = 0.1024 mW
        buf[103] = 0x00;
        buf[104] = 0x0C; // rx power (bytes 104-105): 0x0C35 × 0.0001 = 0.3125 mW
        buf[105] = 0x35;
        buf[110] = 0x00; // status/control (byte 110): all lane status bits clear
        // Alarm/warning flags per SFF-8472 v12.5a § 9.9 (Table 9-17): alarms in
        // bytes 112-113, warnings in bytes 116-117 — bytes 114/115 are the
        // § 9.10 TX input equalization / RX output emphasis controls, NOT
        // flags. One bit per byte, chosen so the committed Python reference
        // discriminates the mapping: a contiguous-bytes-112-117 misreading
        // would report tx_bias_low_warning where the reference has
        // vcc_low_warning (re-review B1's hypothesis).
        buf[112] = 0x01; // alarm flags, byte 112: bit 0 → tx_power_low_alarm
        buf[113] = 0x02; // alarm flags, byte 113: bit 1 is reserved (rx power alarms are bits 7/6)
        buf[114] = 0x04; // TX input equalization control (§ 9.10, Table 9-18): not a flag, not surfaced
        buf[115] = 0x08; // RX output emphasis control (§ 9.10, Table 9-19): not a flag, not surfaced
        buf[116] = 0x10; // warning flags, byte 116: bit 4 → vcc_low_warning
        buf[117] = 0x20; // warning flags, byte 117: bit 5 (optional laser temperature) not in the summary; rx power warnings are bits 7/6
        buf[95] = Sum(buf, 0, 95); // cc_dmi (byte 95): sum of bytes 0-94
        return buf;
    }

    private static void WriteAscii(byte[] buf, int offset, string value)
    {
        for (var i = 0; i < value.Length; i++)
        {
            buf[offset + i] = (byte)value[i];
        }
    }

    private static byte Sum(byte[] buf, int start, int end)
    {
        var sum = 0;
        for (var i = start; i < end; i++)
        {
            sum += buf[i];
        }
        return (byte)(sum & 0xFF);
    }

    // Expected trees are committed outputs of the Python facade
    // `decode_sff8472` on exactly these bytes. Regenerate (from the repo
    // root) with the command recorded in each file's header comment:
    //   python3 bindings/csharp/tests/OpenXcvr.Transceivers.Tests/gen-python-ref-sff8472.py
    // The reference also pins NUL-preserving strings (real goldens are
    // space-padded), reviewer advisory ADV-5.
    [Fact]
    public void ShortA0hDecodesOkAndMatchesPython()
    {
        var tree = Decoder.DecodeSff8472(ShortA0h(), FullA2h());
        Assert.Equal("ok", tree["decode_status"]);
        AssertMatchesPythonReference(tree, "python-ref-sff8472-shorta0h-fulla2h.json");
    }

    // Review follow-up ADV-2: A0h lengths on either side of the 256-byte page
    // — zero (zero-padded to a full page of zeros) and 300 (passed through
    // verbatim, never truncated) — each compared against the Python facade on
    // the same bytes.
    [Theory]
    [InlineData(0, "python-ref-sff8472-emptya0h.json")]
    [InlineData(300, "python-ref-sff8472-a0h300.json")]
    public void A0hLengthZeroAnd300MatchPython(int length, string referenceFile)
    {
        var a0h = new byte[length];
        if (length > 100)
        {
            Array.Copy(ShortA0h(), a0h, 100);
            for (var i = 100; i < length; i++)
            {
                a0h[i] = (byte)(i & 0xFF);
            }
        }

        var tree = Decoder.DecodeSff8472(a0h, FullA2h());
        Assert.Equal("ok", tree["decode_status"]);
        AssertMatchesPythonReference(tree, referenceFile);
    }

    [Fact]
    public void NullPagesStillThrow()
    {
        Assert.Throws<ArgumentException>(() => Decoder.DecodeSff8472(null!, FullA2h()));
        Assert.Throws<ArgumentException>(() => Decoder.DecodeSff8472(ShortA0h(), null!));
    }

    // Review follow-up A2: a 256-byte A0h keeps its caller bytes 128-255 and
    // the generated page decode sees them. The aggregate output exposes no
    // field above byte 127, so this pins the padding helper the decoder
    // composes (DecodeSff8472 pads the caller's buffer directly) and feeds
    // its result through PageA0hBaseMemory.Decode.
    [Fact]
    public void PadPageKeepsCallerBytesAbove128ThroughGeneratedDecode()
    {
        var a0h = new byte[256];
        Array.Copy(ShortA0h(), a0h, 100);
        var expectedUpper = new byte[128];
        for (var i = 0; i < 128; i++)
        {
            expectedUpper[i] = (byte)(0x80 + i);
            a0h[128 + i] = expectedUpper[i];
        }

        var padded = Decoder.PadPage(a0h, 256);
        Assert.Same(a0h, padded); // a full page is never copied or truncated

        var rec = PageA0hBaseMemory.Decode(padded);
        Assert.Equal(expectedUpper, rec.VendorSpecificA0h128255);

        var shortPadded = Decoder.PadPage(ShortA0h(), 256);
        Assert.Equal(256, shortPadded.Length);
        for (var i = 100; i < 256; i++)
        {
            Assert.Equal((byte)0, shortPadded[i]);
        }
    }

    // Serializes the C# tree and deep-compares it to the committed Python
    // reference with the ParityTests comparator (numeric-aware, key order
    // ignored). The reference files carry `//` header comments, hence the
    // comment-skipping parse options.
    private static void AssertMatchesPythonReference(Dictionary<string, object?> tree, string referenceFile)
    {
        var actualJson = JsonSerializer.Serialize(tree);
        using var actual = JsonDocument.Parse(actualJson);
        var path = Path.Combine(
            ParityTests.RepoRoot, "bindings", "csharp", "tests", "OpenXcvr.Transceivers.Tests", referenceFile);
        using var expected = JsonDocument.Parse(
            File.ReadAllText(path), new JsonDocumentOptions { CommentHandling = JsonCommentHandling.Skip });
        var diffs = ParityTests.DeepCompare(actual.RootElement, expected.RootElement);
        Assert.True(
            diffs.Count == 0,
            $"Python reference mismatch ({referenceFile}):\n{string.Join("\n", diffs)}");
    }
}
