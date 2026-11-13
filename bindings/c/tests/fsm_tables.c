/* Runtime checks over the generated CMIS state-machine behaviour tables
 * (fsm headers): fail-closed revision factories, schema parity facts and the
 * host-policy/standard-timer separation, asserted against the compiled
 * pure-data tables themselves. The tables are data only: nothing here
 * reads, writes or clocks a module.
 */
#include "fsm/cmis_module_state_machine.h"
#include "fsm/cmis_data_path_state_machine.h"
#include "fsm/cmis_network_path_state_machine.h"
#include <stdio.h>
#include <string.h>

static int failures;

#define CHECK(cond, msg)                                                    \
    do {                                                                    \
        if (!(cond)) {                                                      \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, (msg)); \
            failures++;                                                     \
        }                                                                   \
    } while (0)

/* Every state a table references (initial, transition endpoints, timeout
 * expiry destinations, terminal exits) must be one of its declared states:
 * a reference into a state absent at that revision would dangle. */
static void check_table_refs(const openxcvr_fsm_table *t, const char *machine)
{
    for (size_t i = 0; i < t->n_states; i++) {
        const openxcvr_fsm_state_desc *a = &t->states[i];
        for (size_t j = i + 1; j < t->n_states; j++) {
            CHECK(a->state != t->states[j].state, "duplicate state member");
        }
    }
    int initial_known = 0;
    for (size_t i = 0; i < t->n_states; i++) {
        if (t->initial == t->states[i].state) {
            initial_known = 1;
            break;
        }
    }
    CHECK(initial_known, "initial state not in the state set");
    for (size_t i = 0; i < t->n_transitions; i++) {
        const openxcvr_fsm_transition *tr = &t->transitions[i];
        int from_known = 0, to_known = 0;
        for (size_t j = 0; j < t->n_states; j++) {
            from_known |= tr->from == t->states[j].state;
            to_known |= tr->to == t->states[j].state;
        }
        CHECK(from_known, "transition from-state not in the state set");
        CHECK(to_known, "transition to-state not in the state set");
    }
    for (size_t i = 0; i < t->n_timeouts; i++) {
        const openxcvr_fsm_timeout *to = &t->timeouts[i];
        /* Host policy never masquerades as a standard timer: no duration,
         * no module-moving expiry. */
        if (to->source.kind == OPENXCVR_FSM_TIMEOUT_SOURCE_HOST_POLICY) {
            CHECK(!to->source.has_duration, "host-policy timeout carries a duration");
            CHECK(!to->has_on_expiry, "host-policy timeout moves the module on expiry");
            CHECK(to->source.timer == 0, "host-policy timeout names a standard timer");
        }
        if (to->has_on_expiry) {
            int known = 0;
            for (size_t j = 0; j < t->n_states; j++) {
                known |= to->on_expiry == t->states[j].state;
            }
            CHECK(known, "timeout expiry destination not in the state set");
        }
    }
    for (size_t i = 0; i < t->n_terminal_exits; i++) {
        int known = 0;
        for (size_t j = 0; j < t->n_states; j++) {
            known |= t->terminal_exits[i].state == t->states[j].state;
        }
        CHECK(known, "terminal exit state not in the state set");
    }
    (void)machine;
}

static const openxcvr_fsm_state_desc *find_state(
    const openxcvr_fsm_table *t, const char *schema_name)
{
    for (size_t i = 0; i < t->n_states; i++) {
        if (strcmp(t->states[i].schema_name, schema_name) == 0) {
            return &t->states[i];
        }
    }
    return NULL;
}

int main(void)
{
    /* Fail-closed factories: catalog revisions resolve, everything else is
     * NULL — no assume-latest, no nearest table. */
    CHECK(openxcvr_cmis_module_state_machine_for_rev_byte(0x53) != NULL, "5.3 must resolve");
    CHECK(openxcvr_cmis_module_state_machine_for_rev_byte(0x30) != NULL, "3.0 must resolve");
    CHECK(openxcvr_cmis_module_state_machine_for_revision(3, 0) != NULL, "(3,0) must resolve");
    CHECK(openxcvr_cmis_module_state_machine_for_revision(5, 3) != NULL, "(5,3) must resolve");
    CHECK(openxcvr_cmis_module_state_machine_for_rev_byte(0x00) == NULL, "0x00 must fail closed");
    CHECK(openxcvr_cmis_module_state_machine_for_rev_byte(0x55) == NULL, "5.5 must fail closed");
    CHECK(openxcvr_cmis_module_state_machine_for_rev_byte(0xFF) == NULL, "0xFF must fail closed");
    CHECK(openxcvr_cmis_module_state_machine_for_revision(5, 5) == NULL, "(5,5) must fail closed");
    CHECK(openxcvr_cmis_module_state_machine_for_revision(0, 0) == NULL, "(0,0) must fail closed");
    CHECK(openxcvr_cmis_module_state_machine_for_revision(9, 9) == NULL, "(9,9) must fail closed");

    const openxcvr_fsm_table *msm =
        openxcvr_cmis_module_state_machine_for_rev_byte(0x53);
    CHECK(msm != NULL, "module table at 5.3");
    if (msm) {
        check_table_refs(msm, "module");
        CHECK(strcmp(msm->id, "CmisModuleStateMachine") == 0, "table id");
        CHECK(msm->n_states == 8, "8 states at 5.3");
        /* The three unreported CMIS 5.3 § 6.3.2 states are modelled. */
        const openxcvr_fsm_state_desc *resetting = find_state(msm, "Resetting");
        const openxcvr_fsm_state_desc *reset = find_state(msm, "Reset");
        const openxcvr_fsm_state_desc *mgmt_init = find_state(msm, "MgmtInit");
        CHECK(resetting && !resetting->reported, "Resetting is unreported");
        CHECK(reset && !reset->reported, "Reset is unreported");
        CHECK(mgmt_init && !mgmt_init->reported, "MgmtInit is unreported");
        CHECK(msm->initial == reset->state, "initial state is Reset");
        /* ModuleState 4 = ModulePwrDn, 5 = ModuleFault (the kmod
         * correction): reserved codes never borrow a state. */
        const openxcvr_fsm_state_desc *pwr_dn = find_state(msm, "ModulePwrDn");
        const openxcvr_fsm_state_desc *fault = find_state(msm, "ModuleFault");
        CHECK(pwr_dn && pwr_dn->reported && pwr_dn->reported_value == 0x4,
              "4 is ModulePwrDn");
        CHECK(fault && fault->reported && fault->reported_value == 0x5,
              "5 is ModuleFault");
        CHECK(fault->state_class == OPENXCVR_FSM_STATE_CLASS_FAULT,
              "ModuleFault has class fault");
        /* tMgmtInit is a standard timer with its CMIS 5.3 value. */
        int tmgmt = 0;
        for (size_t i = 0; i < msm->n_timeouts; i++) {
            const openxcvr_fsm_timeout *to = &msm->timeouts[i];
            if (to->source.kind == OPENXCVR_FSM_TIMEOUT_SOURCE_STANDARD &&
                strcmp(to->source.timer, "tMgmtInit") == 0) {
                tmgmt = 1;
                CHECK(to->source.has_duration, "tMgmtInit carries a duration");
                CHECK(to->source.duration.kind == OPENXCVR_FSM_TIMER_FIXED_MS &&
                          to->source.duration.as.fixed_ms == UINT64_C(2000),
                      "tMgmtInit is fixed 2000 ms");
            }
        }
        CHECK(tmgmt, "tMgmtInit timeout present");
    }

    /* Host policy: kmod's 50 ms cadence and 10 s backstop as named
     * constants, apart from the standard timers. */
    CHECK(OPENXCVR_CMIS_MODULE_STATE_MACHINE_HOST_POLICY_POLL_CADENCE_MS ==
              UINT64_C(50),
          "poll cadence is 50 ms");
    CHECK(OPENXCVR_CMIS_MODULE_STATE_MACHINE_HOST_POLICY_READINESS_BACKSTOP_MS ==
              UINT64_C(10000),
          "readiness backstop is 10 s");

    /* The all-reported machines resolve through the registry enum: their
     * reported values are the registry's raw register values. */
    const openxcvr_fsm_table *dpsm =
        openxcvr_cmis_data_path_state_machine_for_rev_byte(0x53);
    CHECK(dpsm != NULL, "data-path table at 5.3");
    if (dpsm) {
        check_table_refs(dpsm, "data-path");
        CHECK(strcmp(dpsm->id, "CmisDataPathStateMachine") == 0, "data-path id");
        const openxcvr_fsm_state_desc *deactivated = find_state(dpsm, "DPDeactivated");
        CHECK(deactivated && deactivated->reported &&
                  deactivated->state == (uint32_t)OPENXCVR_CMIS_DATA_PATH_STATE_DP_DEACTIVATED &&
                  deactivated->reported_value == 0x1,
              "DPDeactivated maps to registry member 0x1");
        CHECK(dpsm->n_transitions > 1, "data-path has edges at 5.3");
        /* Revision awareness, not assume-latest: the 3.0 table carries the
         * 3.0 edge set, which is strictly smaller than 5.3's. */
        const openxcvr_fsm_table *dpsm_3_0 =
            openxcvr_cmis_data_path_state_machine_for_rev_byte(0x30);
        CHECK(dpsm_3_0 != NULL, "data-path table at 3.0");
        CHECK(dpsm_3_0 && dpsm_3_0->n_transitions < dpsm->n_transitions,
              "3.0 edge set differs from 5.3");
        check_table_refs(dpsm_3_0, "data-path@3.0");
    }
    const openxcvr_fsm_table *npsm =
        openxcvr_cmis_network_path_state_machine_for_rev_byte(0x30);
    CHECK(npsm != NULL, "network-path table at 3.0");
    if (npsm) {
        check_table_refs(npsm, "network-path");
        /* 3.0 table references only 3.0 states: revision awareness, not
         * assume-latest. */
        CHECK(npsm->n_transitions == 9, "network-path edges at 3.0");
    }

    if (failures) {
        fprintf(stderr, "%d failure(s)\n", failures);
        return 1;
    }
    printf("fsm tables OK\n");
    return 0;
}
