# Issue 186 Source Flag

`schemas/sff-8024/smf-media-ids.yaml` now exposes `0xF0` through `0xFE`
as explicit entries so fixture references such as `0xF7` can resolve.

The checked local source, SFF-8024 Rev 4.13.2 (Table 4-7), does not
contain named `100ZR`, `400ZR`, `OpenZR+`, `OTUC1`,
`FOIC4`, `800ZR`, or `FOIC8` rows in the `0xF0` through `0xFE` range.
(The local PDF cache has since been refreshed to Rev 4.14; the
`0xF0`–`0xFE` block remains vendor-specific in 4.14 too.)
The extracted row covering these values is:

```text
192-255 C0-FF Vendor Specific/Custom
```

The public SNIA `SFF-8024 Rev 4.13` PDF checked during this pass also has no
`0xF0` through `0xFE` coherent block. The new entries therefore preserve the
vendor-specific semantics confirmed by SFF-8024 4.13.2 rather than assigning
unsourced coherent names.
