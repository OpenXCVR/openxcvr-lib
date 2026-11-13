<?php
declare(strict_types=1);
namespace Openxcvr;

class OpenxcvrDecoder
{
    use OpenxcvrDecoderLookups;
    use OpenxcvrSff8636Decoder;
    use OpenxcvrSff8472Decoder;
    use OpenxcvrCmisDecoder;
}
