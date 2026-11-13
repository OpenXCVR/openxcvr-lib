# openxcvr Java bindings

Java binding: generated page records emitted from the same schemas as the Rust crates, with a hand-written `com.openxcvr.Openxcvr` facade on top.

## Overview

Java facade for CMIS, SFF-8636, and SFF-8472. `com.openxcvr.Openxcvr`
(`src/main/java/com/openxcvr/Openxcvr.java`) is the handwritten aggregate
decoder. It does **not** implement the canonical page map, lane-flag
functions, or threshold evaluators. Those remain Python, Go, and TypeScript
only; see [../README.md](../README.md#consumer-facade-contract).

`src/main/java/com/openxcvr/generated/` is codegen (package
`com.openxcvr.generated`). Do not edit it.

## Installation

Build from `bindings/java`. The library compiles as release 11
(`options.release = 11` in `build.gradle`,
`maven.compiler.release` 11 in `pom.xml`) and has no runtime dependencies.
JUnit 5 and Jackson are test scope only.

```sh
cd bindings/java
gradle jar          # build/libs/java-0.2.0.jar  (directory name, not the Maven id)
# or
mvn -B package
```

Maven coordinates in `pom.xml` are `com.openxcvr:transceivers:0.2.0`. Gradle
`group` / `version` match (`com.openxcvr` / `0.2.0`); the jar file name follows
the project directory. Consume the jar on the classpath:

```sh
javac --release 11 -cp build/libs/java-0.2.0.jar Quickstart.java
java -cp build/libs/java-0.2.0.jar:. Quickstart
```

`bindings/java/consumer-check/run-consumer-check.sh` is the installed-artifact
check: it builds that jar and compiles a consumer against only the jar.
`.github/workflows/parity.yml` runs `gradle build` on CI. The quickstart below
was checked against `Openxcvr.java` on a host without `gradle` or `mvn`; it
was not executed here.

## Quickstart

Synthetic bytes only. Lower byte 1 `0x50` is CMIS 5.0. Page `00h` bytes 1–16
are the vendor name. Pass `null` for optional pages that were not read.

```java
import com.openxcvr.Openxcvr;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Map;

public final class Quickstart {
    public static void main(String[] args) {
        byte[] lower = new byte[128];
        lower[1] = 0x50;
        byte[] page00h = new byte[128];
        java.util.Arrays.fill(page00h, 1, 17, (byte) ' ');
        byte[] name = "SYNTHETIC".getBytes(StandardCharsets.US_ASCII);
        System.arraycopy(name, 0, page00h, 1, name.length);

        Map<String, Object> result = Openxcvr.decodeCmis(lower, page00h, null, null);
        if (!"ok".equals(result.get("decode_status"))) {
            throw new IllegalStateException(String.valueOf(result.get("rejection")));
        }
        @SuppressWarnings("unchecked")
        Map<String, Object> identity = (Map<String, Object>) result.get("identity");
        @SuppressWarnings("unchecked")
        Map<String, Object> module = (Map<String, Object>) result.get("module");
        if (!"SYNTHETIC".equals(identity.get("vendor_name"))
                || !"5.0".equals(module.get("cmis_revision"))) {
            throw new IllegalStateException(Arrays.toString(new Object[] {identity, module}));
        }
        System.out.println(identity.get("vendor_name") + " " + module.get("cmis_revision"));
    }
}
```

Verified by inspection of `Openxcvr.decodeCmis` (vendor name is
`CmisPage00hAdministrative.vendorName`, which is `buf[1:17]` with trailing
spaces stripped). CI runs the Gradle build.

## Usage

```java
Map<String, Object> Openxcvr.decodeCmis(byte[] lower, byte[] page00h,
                                        byte[] page01h, byte[] page11h);
Map<String, Object> Openxcvr.decodeCmis(byte[] lower, byte[] page00h,
                                        byte[] page01h, byte[] page11h,
                                        Map<String, byte[]> vdmPages);
Map<String, Object> Openxcvr.decodeSff8636(byte[] lower, byte[] page00h);
Map<String, Object> Openxcvr.decodeSff8472(byte[] a0h, byte[] a2h);
```

Results are `LinkedHashMap` / `ArrayList` trees. Null values are preserved
(flat-memory CMIS emits null `applications` / `flags` / `lanes`; SFF-8472
without DDM emits null `flags` / `monitoring` / `lanes`).

`page01h` and `page11h` may be null. `vdmPages` keys are `"20h"`..`"2fh"`
excluding `"2eh"`; the `vdm` entry is produced only when every page of that
set is present. A partial set does not zero-fill the missing pages. An unknown
CMIS major (not 3, 4, or 5) returns `decode_status` `"rejected"` and
`rejection.reason` `unknown_major_revision: 0xNN` and does not throw.

`decodeSff8636` always returns `decode_status` `"ok"` for pages of at least
128 bytes. `decodeSff8472` zero-pads each page to 256 bytes. Its telemetry is
uncalibrated; check `calibration.externally_calibrated`. There is no Java
lane-flag or threshold-evaluation method. Lane maps inside the SFF documents
use 1-based `index`.

These decoders admit CMIS by major revision and decode flat-union page fields.
They are not a revision-qualified facade.

## Error handling

- Null `lower` or `page00h` on CMIS or SFF-8636: `IllegalArgumentException`
  `<name> is required`.
- A non-null CMIS `lower`, `page00h`, `page01h`, or `page11h` (or an
  SFF-8636 `lower`/`page00h`) shorter than 128: `IllegalArgumentException`
  `<name> must be at least 128 bytes, got N`. These are validated up
  front, before the CMIS revision check, so a short `lower` does not
  return `rejected`.
- A supplied VDM page is different: its length is checked inside VDM
  decode, which runs only after the revision check passes. So an unknown
  CMIS major plus a short supplied VDM page returns `decode_status`
  `"rejected"` — the VDM length error never fires. On a known revision,
  a short supplied VDM page throws `IllegalArgumentException`
  (`VDM page 2Fh missing or short: N bytes`, and the analogous message for
  descriptor/sample pages) once the `vdm` entry is being built.
- Page `11h` non-null while `page01h` is null: `IllegalArgumentException`
  (Tx-bias multiplier lives on page `01h`).
- `decodeSff8472`: null `a0h` or `a2h` throws `NullPointerException`
  (`Objects.requireNonNull`). A short array is zero-padded, not rejected.
- Unknown CMIS major on full pages: not an exception.

## Regeneration

From the repository root. Do not edit `src/main/java/com/openxcvr/generated/`.

```sh
tools/regen-java.sh                # cmis, sff-8636, sff-8472
tools/regen-java.sh cmis
tools/regen-all.sh
```

`tools/regen-java.sh` runs
`cargo run -p openxcvr-codegen -- generate --spec <spec> --lang java --out bindings/java/src/main/java/com/openxcvr/generated`.
`Openxcvr.java` is handwritten.

```sh
gradle build      # or: mvn -B package — both run the golden parity tests
```

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
