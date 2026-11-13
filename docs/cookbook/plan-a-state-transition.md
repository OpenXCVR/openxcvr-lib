# Plan a state transition

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Ask the generated CMIS module state machine for the edges from
LowPwr to Ready. The plan is data. Nothing here reads or writes a module.

```rust
use openxcvr::prelude::Rev;
use openxcvr_cmis::adapter::{ModuleState as CmisMsmState, Msm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Msm::plan_transition_for_rev(Rev::CMIS_5_2, CmisMsmState::LowPwr, CmisMsmState::Ready)?;
    for edge in &path {
        println!(
            "{}: {:?} -> {:?} guards {} effects {}",
            edge.id, edge.from, edge.to, edge.guards.len(), edge.side_effects.len()
        );
    }
    Ok(())
}
```

Expected output:

```text
module_low_pwr_to_module_pwr_up_sw: ModuleLowPwr -> ModulePwrUp guards 1 effects 3
module_pwr_up_to_module_ready: ModulePwrUp -> ModuleReady guards 0 effects 4
```

**Notes.** This `ModuleState` is `openxcvr_cmis::adapter::ModuleState`,
the register state. It is not `openxcvr::prelude::ModuleState`, which is
the synthesized flag summary. Re-check each edge with the behaviour table
when you execute it; guards are live facts the plan does not sample.
A revision with no table, or no path, is an `MsmError` and not a guessed
route. Python, Go, and TypeScript have no state-machine facade.
