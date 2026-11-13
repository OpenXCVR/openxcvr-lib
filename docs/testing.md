# Testing and fast feedback

Run commands from the repository root unless a command changes directory.
Use the focused loop for the component you are editing, then run its full suite.
These loops complement, rather than replace, the workspace and binding checks.

## Generator CLI contracts

```sh
cargo test -p openxcvr-codegen --test cli_orchestration
```

This invokes the real generator in isolated temporary directories. It checks
language dispatch, default Rust module wiring, unsupported-language diagnostics,
and preflight rejection across all supported output languages. Invalid pages
must all be reported; invalid input must not overwrite existing output files or
leave newly generated files behind. Directory creation itself is not atomic;
these checks do not promise transactional rollback for later write failures.

The emitter comparisons check CLI routing, not the correctness of the emitters.
For emitter semantics, run the codegen suite and relevant binding tests; verify
regeneration equality when generated output changes:

```sh
cargo test -p openxcvr-codegen
./tools/check-regen-equality.sh --require-clean
```

The equality command requires a clean checkout and checks committed outputs; it
is not an independent oracle for whether those outputs are correct. Fixtures,
goldens, and focused examples supply that evidence. Do not update canonical
goldens merely to make a changed decoder pass.

## PHP decoder contracts

Use PHP 8.2 or later with the extensions required by Composer and PHPUnit:

```sh
(cd bindings/php && composer install --no-interaction --prefer-dist)
(cd bindings/php && vendor/bin/phpunit tests/ParityTest.php)
(cd bindings/php && vendor/bin/phpunit)
```

Parity exercises the public `OpenxcvrDecoder` facade against independently
recorded goldens. Focused status tests also exercise individual input bits so
all-clear captures cannot hide incorrect flag extraction.

The shared `phpunit.xml` makes warnings and risky tests fail the command, even
when their assertions pass. A retired generated property can produce a warning
and silently become a plausible false flag; a green assertion is insufficient.
Existing PHPUnit deprecation notices remain visible rather than suppressed.
Every discovered golden must have its required base capture; a missing capture
is a discovery error, not a smaller successful test run. Optional pages retain
their existing decoding rules.

A vendored copy of this tree's C binding, refreshed into another component
by that component's sync script, is not documentation of this repository.
Do not treat its links as this tree's.

Keep documentation checks separate from installed dependencies. A checkout
with Composer packages present also contains third-party documentation, and
a link check of that tree is not a check of this repository. CI uses a fresh
checkout. Do not exclude every directory named `vendor`: a sync may keep
project files under that name.

## OpenAPI contracts

Install `jsonschema` and `pyyaml` in your Python environment, then run both:

```sh
python3 tools/validate-golden-schema.py
python3 -m unittest tools.test_validate_golden_schema
```

The first checks the current corpus against generated schemas. The second checks
schema constraints and the recorded CMIS historical verdicts. Adding a golden
requires evaluating its historical verdict independently, not copying the new
validator's result into the expected table.

## CI and defect sensitivity

[parity.yml](../.github/workflows/parity.yml) runs Rust, Go, Python, and OpenAPI in
independent jobs, alongside the other language jobs. A failed or slow Rust build
cannot skip their results. The Rust job runs the focused CLI contracts before
building and testing the full workspace. Existing schema, Go vet, Python
synthetic-builder, and full binding checks remain enabled.

When adding a regression test, demonstrate a specific failure in an isolated
worktree, restore the correct code, and rerun the focused and full affected
suites. Useful probes for these contracts are bypassing language admission,
reporting only the first invalid page, writing an enum before validation, or
reading a retired generated PHP property. Temporarily withholding a capture
must produce an explicit discovery error. Check diagnostics as well as exit
status: a compiler error or missing test dependency is not evidence that a
behavioral test detected its intended defect. Restore all probes before
committing; never mutate shared fixtures during a normal test run.

These offline loops do not qualify live module operations or replace transport
and installed-artifact checks required by the
[versioning notes](versioning.md).
