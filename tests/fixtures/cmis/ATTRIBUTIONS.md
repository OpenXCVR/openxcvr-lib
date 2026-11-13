# Attributions — CMIS fixture corpus

The `fboss-cmis-*` fixtures in this directory are derived from byte arrays in [`facebook/fboss`](https://github.com/facebook/fboss) (`fboss/qsfp_service/module/tests/FakeTransceiverImpl.cpp`). Per BSD-3-Clause license terms, the original copyright notice + the three license conditions are reproduced below.

## Source

- Repository: https://github.com/facebook/fboss
- File: `fboss/qsfp_service/module/tests/FakeTransceiverImpl.cpp`
- Extraction tool: `tools/fboss-to-bin.py` (extracts each `kCmis<NAME>` fixture's lower memory + upper page byte arrays into per-page `.bin` files matching the `openxcvr.page.v1` schema topology)

## License (BSD-3-Clause)

```
BSD License

For fboss software

Copyright (c) 2013-2015, Facebook, Inc.
All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

 * Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

 * Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

 * Neither the name Facebook nor the names of its contributors may be used to
   endorse or promote products derived from this software without specific
   prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR
ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
(INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## Coverage of the BSD-3-Clause conditions

- **Condition 1** (source distribution must retain copyright notice + disclaimer): satisfied by this `ATTRIBUTIONS.md` file shipped alongside the `.bin` fixtures in source form.
- **Condition 2** (binary distribution must reproduce notice in documentation): satisfied — same file serves as documentation for any binary redistribution.
- **Condition 3** (no Facebook endorsement): the openxcvr project does not use the Facebook name or fboss contributor names to endorse or promote derived work. The `fboss-cmis-*` filename prefix is a factual provenance marker, not an endorsement claim.
