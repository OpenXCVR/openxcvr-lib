// Generated-resolver defensive contract (review follow-ups ADV-5/ADV-6):
// the CMIS media-interface resolver returns null — never throws, never a
// name — for a media type or code outside 0..255. This is a C#-side
// defensive contract, not Python parity: real decodes always pass a
// media-interface code that came from a byte (0..255), so the out-of-range
// behavior is unreachable from decode paths and the Python resolver was
// not compared on these inputs.
using OpenXcvr.Generated;
using Xunit;

namespace OpenXcvr.Transceivers.Tests;

public class CmisMediaInterfaceTests
{
    [Theory]
    [InlineData(-1, 1)]
    [InlineData(256, 1)]
    [InlineData(1, -1)]
    [InlineData(1, 256)]
    public void OutOfRangeMediaTypeOrCodeResolvesToNull(int mediaType, int code)
    {
        Assert.Null(CmisMediaInterface.Name(mediaType, code));
    }
}
