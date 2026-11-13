# Test fixture corpus

Real EEPROM dumps, synthetic fixtures, and provenance docs for end-to-end decode verification.

## Layout

```
tests/fixtures/
├── README.md                     (this file)
├── parity/                        kernel/userland parity fixture cases (synthetic, standard-cited)
│   ├── README.md                 parity format README + shape/adapter/donor contract
│   ├── _shape.yaml                machine-readable closed-world contract (metadata, never a case file)
│   ├── donor_inventory.yaml       pinned donor inventory (metadata, never a case file)
│   └── <kind>.yaml                case files, one per kind (module_state, readiness, classifier, thermal,
│                                  selector, dynamic_data, lifecycle, scheduling, operations)
├── cmis/                         openxcvr.page.v1 against CMIS schemas (fboss-derived + synthetic, keyed by CMIS rev)
│   ├── README.md                 corpus README + fixture table
│   ├── ATTRIBUTIONS.md           fboss BSD-3-Clause notice
│   ├── <fixture>.md              per-fixture provenance (synthetic + derived fixtures)
│   ├── <fixture>.lower.bin       Lower memory (128 bytes)
│   └── <fixture>.page-NNh.bin    Upper Page NNh (128 bytes each)
├── sff-8472/                     openxcvr.page.v1 against SFF-8472 v12.5a schemas
│   ├── README.md
│   ├── <fixture>.md              per-fixture provenance (one per module)
│   ├── <fixture>.a0h.bin         A0h base ID memory (128 or 256 bytes)
│   └── <fixture>.a2h.bin         A2h diagnostics (256 bytes)
└── sff-8636/                     openxcvr.page.v1 against SFF-8636 v2.11 schemas
    ├── README.md
    ├── <fixture>.md              per-fixture provenance
    ├── <fixture>.lower.bin       Lower memory (128 bytes, always present)
    └── <fixture>.page-NNh.bin    Upper Page NNh (128 bytes each)
```

Per-page `.bin` files mirror the `openxcvr.page.v1` schema topology — each binary is exactly the slice the matching schema describes, so a decoder reads `<fixture>.page-00h.bin` and walks `schemas/<spec>/page-00h-*.yaml` against it.

## License posture

This repository is MIT-licensed (see `LICENSE`). Inbound test fixtures must come from sources with **clearly compatible** licenses (Apache-2.0, MIT, BSD, CC0, public domain). Each fixture's `.md` records the source, license, and any anonymization done. Anything ambiguous goes back out — better to drop a fixture than to taint the corpus.

Synthetic fixtures (`synthetic-*` stems under each spec directory) are original work, in-tree, no external license attribution needed. They are emitted by `tools/gen-synthetic-fixtures.py` (SFF-8472 and SFF-8636: `.bin`, `.md` provenance, and golden JSON) and by `tools/build-cmis-3-0-synthetic.py` / `tools/build-cmis-5-2-synthetic.py` (CMIS: `.bin` slices with per-byte spec citations; see `cmis/README.md` § Synthetic fixtures).

## How to add a real fixture

1. Confirm the source is license-compatible (see § License posture). Note the URL, license text, and copyright holder.
2. Run an extractor to produce the canonical per-page `.bin` files:
   - `tools/oom-to-bin.py <input.txt> <stem>` — OCP OOM `module_data/*.txt` dumps (auto-detects FCC SETUP for SFP+ pairs vs. EEPROM Setup for QSFP).
   - `tools/fboss-to-bin.py <FakeTransceiverImpl.cpp> <fixture_id> <stem>` — facebook/fboss C++ static byte-array fixtures (auto-detects LowerA0 ↔ UpperPagesA0 pairing by proximity for fboss's inconsistent OSFP naming).
   - `tools/optoe-to-bin.py --interface <iface> --stem <stem>` — **on-switch capture from real transceivers** (SONiC / OPTOE-based Linux). Reads via ethtool / direct sysfs / sfputil with stdlib-only deps. Generates a draft `<stem>.md` provenance template that requires manual review before submission.
3. Author `<fixture>.md` next to the `.bin`s (template: any existing `.md` in the corpus). Cover: module identity (vendor / PN / S/N / date), source URL + license, pages included, notable schema interactions exercised, anonymization status, regeneration command. (For `optoe-to-bin.py` captures, the tool emits a draft template — edit the placeholder fields and the **Anonymization** section before opening the PR.)
4. Add a row to the matching spec README's index table.

## Distilling a real capture into a publishable synthetic fixture

When the raw bytes are **not** authorized for publication — vendor permission not
obtained, license unclear, or the capture carries customer data — grow the corpus
through distillation instead: `tools/distill-capture.py` scrubs a private raw capture
into synthetic-fixture builder inputs so the decode evidence lands here while the
raw capture never does. Vendor-derived captures do not belong in this
repository. The license-cleared extraction paths above (OOM, fboss, optoe with
redistribution rights) remain the route for publishing raw bytes.

Workflow (the raw capture stays private at every step):

```sh
# capture privately, one file per slice, named after the fixture convention:
#   <stem>.a0h.bin / <stem>.a2h.bin        (SFF-8472)
#   <stem>.lower.bin / <stem>.page-NNh.bin (SFF-8636, CMIS)

python3 tools/distill-capture.py \
    --spec sff-8472 --input /private/captures/port0.a0h.bin \
    --name synthetic-sff8472-distilled-<model> \
    --outdir tests/fixtures/sff-8472 \
    --provenance tests/fixtures/sff-8472/synthetic-sff8472-distilled-<model>.md
```

What the tool does, all schema-driven (never guessed):

- **Vendor-identity fields** — schema fields with `template: vendor_info` or the
  `Information::Vendor` tag (VendorName, VendorOUI, VendorPN, VendorRev, VendorSN,
  DateCode; plus CMIS `clei_code` as a documented privacy exception) — are replaced
  with a synthetic marker (`SYNTHETIC` by default, `--marker` to override), ASCII
  space-padded per the specs.
- **Opaque private regions** — `vendor_specific` / `storage` templates
  (vendor-specific bytes, user EEPROM) — are zeroed. For an SFF-8472 A2h capture,
  declare the selected page with `--a2h-page` or the whole 128–255 region is zeroed
  conservatively.
- **Checksums** whose schema-declared `covers_range` intersects a scrubbed range
  are recomputed (`sum8`), so the scrubbed fixture still decodes with valid
  checksums; untouched checksums are verified and any pre-existing mismatch is
  recorded, never silently patched.
- **Layout is preserved**: the output `.bin` is byte-for-byte the same length and
  identical everywhere outside the scrubbed ranges, so real-module evidence
  (compliance codes, monitor values, page layouts) survives into the golden corpus.
- It **fails closed** on an unreadable input, a capture larger than the schema's
  page or a vendor range cut in half by a partial capture, and it never writes raw
  bytes anywhere.

Each run emits `<name>.<slice>.bin` plus a machine-readable
`<name>.<slice>.bin.distill.json` scrub report (fields scrubbed at which offsets,
checksums recomputed, output digest — no original bytes, no input path). Review
the report, complete the provenance stub, and verify by hex dump that no original
vendor identity string remains before committing. Tests for the tool live in
`tools/test_distill_capture.py`.

## Submitting a switch-captured fixture

If you have access to a switch with optical transceivers and are willing to contribute fixtures from your hardware:

```sh
# On the switch (typically requires root for /sys/bus/i2c access):
sudo tools/optoe-to-bin.py --interface Ethernet0 --stem /tmp/myswitch-port-0
sudo tools/optoe-to-bin.py --sysfs /sys/bus/i2c/devices/2-0050/eeprom --stem /tmp/myswitch-port-1
sudo tools/optoe-to-bin.py --sfputil-port Ethernet8 --stem /tmp/myswitch-port-8

# Off the switch:
scp myswitch:/tmp/myswitch-port-*.{bin,md} ./tests/fixtures/<spec>/
```

Edit the generated `.md` to fill in: capture date, switch model + SONiC version, firmware revision, your right-to-redistribute justification, and anonymization decisions. Open a PR.

## How to add a synthetic fixture

1. Author the bytes in a generator script, not by hand: add an entry to `tools/gen-synthetic-fixtures.py` (SFF-8472/SFF-8636) or a dedicated `tools/build-<spec>-synthetic.py` (CMIS pattern), citing the spec section for every non-zero byte.
2. Use explicit synthetic vendor identity strings (e.g. `OPENXCVR` / `SYN-*` / `SYNTH*` / `*_SYNTH`) so a hex-dump audit classifies the fixture as non-real.
3. Emit the `<fixture>.md` provenance next to the `.bin`s, stating that the fixture is synthetic, the spec revision it targets, and the regeneration command.
4. Add a row to the matching spec README's index table.

## Provenance

The real SFF-8472/SFF-8636 dumps come from [opencomputeproject/oom](https://github.com/opencomputeproject/oom) (`oom/module_data/`, MIT license, Copyright (c) 2016 ocpnetworking-wip). OCP redistributed the data with explicit MIT permission, so vendor S/N and date codes appear in their original form — no further anonymization required for downstream redistribution. The CMIS fixtures are extracted from [facebook/fboss](https://github.com/facebook/fboss) test data (BSD-3-Clause; see `cmis/ATTRIBUTIONS.md` and `cmis/README.md`). Synthetic fixtures are project-authored (see § License posture).
