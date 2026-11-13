//! Regression: the prelude alone must resolve every consumer-facing return
//! type of the re-exported `Module` trait surface. Before the fix,
//! `use openxcvr::prelude::*;` omitted `ChecksumStatus`, `Application` and
//! `ModuleState`, so a consumer writing
//! `fn f(m: &UnifiedModule) -> Result<ChecksumStatus, ModuleError>` failed
//! name resolution.

use openxcvr::prelude::*;

#[test]
fn prelude_resolves_module_consumer_result_types() {
    fn checksums(m: &UnifiedModule) -> Result<ChecksumStatus, ModuleError> {
        m.checksums()
    }
    fn applications(m: &UnifiedModule) -> Result<Vec<Application>, ModuleError> {
        m.applications()
    }
    fn module_state(m: &UnifiedModule) -> Result<ModuleState, ModuleError> {
        m.module_state()
    }
    // `Rev::family` is public on the re-exported `Rev`, so `Family` is
    // consumer-facing for the revision APIs.
    fn family_of(rev: Rev) -> Family {
        rev.family
    }
    let _ = (checksums, applications, module_state, family_of);
}
