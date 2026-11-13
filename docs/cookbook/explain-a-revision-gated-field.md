# Explain a revision-gated field

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** When a read says a field is not exposed, ask the catalog whether
that name exists at another revision. The report does not touch the module.

```rust
use openxcvr::catalog::family_catalog;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = family_catalog(Family::Cmis).expect("CMIS has a generated catalog");
    let at_40 = field_availability(catalog, Rev::CMIS_4_0)?;
    let at_50 = field_availability(catalog, Rev::CMIS_5_0)?;
    println!("{}", at_40.field("mci_max_speed").expect("schema field"));
    println!("{}", at_40.field("twi_max_speed_cmis4").expect("schema field"));
    println!("mci at 5.0 gated {}", at_50.field("mci_max_speed").unwrap().is_gated());
    println!("unknown name is {:?}", at_50.field("not_a_schema_field"));
    Ok(())
}
```

Expected output:

```text
mci_max_speed is revision-gated; page lower address 0x50 offset 2 until CMIS 4.0; page lower address 0x50 offset 2 since CMIS 5.0 until CMIS 5.3; page lower address 0x50 offset 2 since CMIS 5.3
twi_max_speed_cmis4 is available; page lower address 0x50 offset 2 since CMIS 4.0 until CMIS 5.0
mci at 5.0 gated false
unknown name is None
```

**Notes.** At CMIS 4.0 the speed field that applies is
`twi_max_speed_cmis4`. `mci_max_speed` is a different name and is gated
there; it becomes available at 5.0. `field` returns `None` only when the
catalog has no span under that name. A gated field is `Some` and
`is_gated()`. `UnifiedModule::revision_report` builds the same report
from a handle that was opened with detection. A handle with no detected
revision returns `RevisionReportError::RevisionNotDetected` instead of
assuming one. No facade exposes this report.
