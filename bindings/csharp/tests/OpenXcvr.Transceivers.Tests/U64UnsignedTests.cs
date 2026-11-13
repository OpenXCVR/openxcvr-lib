using System.Reflection;
using OpenXcvr.Generated;
using Xunit;

namespace OpenXcvr.Transceivers.Tests;

/// <summary>
/// u64le/u64be decode and write as ulong, including values &gt;= 2^63.
/// A ulong parameter cannot represent an out-of-range value; the writer
/// rejects one by refusing to bind it.
/// </summary>
public class U64UnsignedTests
{
    [Fact]
    public void Page14hDecodesUnsignedLittleEndian()
    {
        var high = new byte[128];
        // HostSideErrorCountLane1 is flat byte 192, file offset 64, little-endian.
        high[64] = 0x01;
        high[71] = 0x80;
        Assert.Equal(
            9223372036854775809UL,
            CmisPage14hDiagnosticsResults.Decode(high).HostSideErrorCountLane14[0]);

        var max = new byte[128];
        for (var i = 64; i < 72; i++)
        {
            max[i] = 0xFF;
        }
        Assert.Equal(
            ulong.MaxValue,
            CmisPage14hDiagnosticsResults.Decode(max).HostSideErrorCountLane14[0]);
    }

    [Fact]
    public void LittleEndianRoundTrip()
    {
        RoundTrip(little: true);
        var buf = new byte[8];
        U64LeRoundTrip.SetCounter(buf, 1UL << 63);
        Assert.Equal(0x00, buf[0]);
        Assert.Equal(0x80, buf[7]);
    }

    [Fact]
    public void BigEndianRoundTrip()
    {
        RoundTrip(little: false);
        var buf = new byte[8];
        U64BeRoundTrip.SetCounter(buf, 1UL << 63);
        Assert.Equal(0x80, buf[0]);
        Assert.Equal(0x00, buf[7]);
    }

    [Fact]
    public void WriterRejectsOutOfRangeInput()
    {
        foreach (var method in new[]
        {
            typeof(U64LeRoundTrip).GetMethod("SetCounter"),
            typeof(U64BeRoundTrip).GetMethod("SetCounter"),
        })
        {
            Assert.NotNull(method);
            Assert.Equal(typeof(ulong), method!.GetParameters()[1].ParameterType);
            Assert.Throws<ArgumentException>(() => method.Invoke(null, new object[] { new byte[8], -1L }));
            Assert.Throws<ArgumentException>(() => method.Invoke(null, new object[] { new byte[8], "18446744073709551616" }));
        }
    }

    private static void RoundTrip(bool little)
    {
        ulong[] values = { 0UL, long.MaxValue, 1UL << 63, ulong.MaxValue };
        foreach (var value in values)
        {
            var buf = new byte[8];
            if (little)
            {
                U64LeRoundTrip.SetCounter(buf, value);
                Assert.Equal(value, U64LeRoundTrip.Decode(buf).Counter);
            }
            else
            {
                U64BeRoundTrip.SetCounter(buf, value);
                Assert.Equal(value, U64BeRoundTrip.Decode(buf).Counter);
            }
        }
    }
}
