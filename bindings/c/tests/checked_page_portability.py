#!/usr/bin/env python3
"""Kernel-style portability gate for the generated revision-checked pages.

Compiles exactly the two new generated sources
(``cmis_page01h_advertising_revision.c`` and
``cmis_page11h_lane_status_revision.c``) two ways:

1. Host freestanding gate (always runs): the plain host compiler with
   ``-std=c99 -ffreestanding -Wall -Wextra -Werror
   -Wdeclaration-after-statement`` plus ``-fstack-usage`` when the compiler
   accepts it (GCC emits the frame report; Apple Clang rejects the flag
   outright, so it is probed for first and the report it enables is skipped
   where unsupported). The declaration flag is the kernel recipe
   constraint the supported LTS kernels build with; this test fails on it
   instead of suppressing the warning and claiming cleanliness.

2. Kernel-recipe gate (runs only when the sibling kmod plugin directory is
   supplied and a matching kernel build tree exists): stages the UNCHANGED
   kmod ``openxcvr_decoders.c`` wrapper and its ``stdint.h``/``stdbool.h``/
   ``stddef.h`` shims under a scratch tree, copies THIS worktree's generated C
   into the relative ``vendor/openxcvr/...`` path the wrapper includes (so the
   old vendored implementation is never what gets compiled), and builds the
   staged objects with the real ``make -C /lib/modules/$(uname -r)/build``
   recipe. The staged Kbuild mirrors the plugin's include flags and adds
   ``-Wdeclaration-after-statement -Werror -fstack-usage`` so the kernel
   recipe is a gate, not a silent pass. A test-only second object includes
   the two new revision sources; the kmod wrapper itself stays unchanged.

Stack frames measured with ``-fstack-usage`` are reported, not budgeted: no
universal kernel frame budget is claimed here.

This is host-qualified evidence: it proves the generated sources compile
under this host's kernel recipe, NOT that the kmod has migrated to them or
that any hardware was exercised. The sibling repository is never written.

usage: checked_page_portability.py <cc> <bindings_dir> <kmod_plugins_cmis_or_-> <scratch_dir>
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys

cc, bindings, kmod_plugins, scratch = (Path(a) for a in sys.argv[1:5])
bindings = bindings.resolve()
scratch = scratch.resolve()
kmod_plugins = None if kmod_plugins.as_posix() == "-" else kmod_plugins.resolve()

REVISION_SOURCES = [
    "cmis_page01h_advertising_revision.c",
    "cmis_page11h_lane_status_revision.c",
]
SHIMS = ["stdint.h", "stdbool.h", "stddef.h"]
KERNEL_BUILD = Path(f"/lib/modules/{os.uname().release}/build")

if scratch.exists():
    shutil.rmtree(scratch)
scratch.mkdir(parents=True)

generated = bindings / "generated"
missing = [n for n in REVISION_SOURCES if not (generated / "pages" / n).is_file()]
if missing:
    sys.exit(f"missing generated sources: {missing}")

# -fstack-usage is a GCC flag: Apple Clang (the macOS runner compiler)
# rejects it, which made this gate fail the macOS CI legs. Probe for it
# instead of assuming it; the .su frame report it produces is optional
# evidence (reported, never budgeted), while the compile gate above must
# run on every host.
stack_usage_probe = subprocess.run(
    [os.fspath(cc), "-fstack-usage", "-fsyntax-only", "-x", "c", "-"],
    input="int main(void) { return 0; }\n", capture_output=True, text=True,
)
has_stack_usage = stack_usage_probe.returncode == 0

# --- Gate 1: host freestanding compile -------------------------------------
free_dir = scratch / "freestanding"
free_dir.mkdir()
flags = [
    "-std=c99",
    "-ffreestanding",
    "-Wall",
    "-Wextra",
    "-Werror",
    "-Wdeclaration-after-statement",
]
if has_stack_usage:
    flags.append("-fstack-usage")
flags.extend([f"-I{generated}", f"-I{bindings / 'include'}"])
frames = {}
for name in REVISION_SOURCES:
    obj = free_dir / (name[:-2] + ".o")
    proc = subprocess.run(
        [os.fspath(cc), *flags, "-c", os.fspath(generated / "pages" / name),
         "-o", os.fspath(obj)],
        capture_output=True, text=True,
    )
    print(f"[freestanding] {cc} {' '.join(flags[:6])} -c generated/pages/{name}")
    if proc.returncode != 0:
        print(proc.stdout + proc.stderr, end="")
        sys.exit(f"freestanding gate FAILED for {name} (exit {proc.returncode})")
    su = free_dir / (name[:-2] + ".su")
    if not su.is_file():
        # The compiler took -fstack-usage but emitted no report: say so
        # instead of crashing on the missing optional evidence.
        print(f"[freestanding] no {su.name} emitted for {name}: "
              "stack-usage report unavailable")
        continue
    for line in su.read_text().splitlines():
        where, size, _kind = line.split("\t")
        # gcc .su rows are "file:line:col:function\tbytes\ttype".
        func = where.rsplit(":", 1)[1] if ":" in where else where
        frames[(name, func)] = int(size)
if not has_stack_usage:
    print("[freestanding] -fstack-usage unsupported by this compiler "
          "(e.g. Apple Clang): stack-usage reporting skipped; the compile "
          "gate above still ran in full")
for (name, func), size in sorted(frames.items()):
    print(f"[freestanding] stack usage {name}: {func} = {size} bytes")
print(f"[freestanding] gate passed: both revision sources clean under "
      f"-std=c99 -ffreestanding -Wall -Wextra -Werror -Wdeclaration-after-statement")

# --- Gate 2: real kernel build recipe on staged sources ---------------------
if kmod_plugins is None or not (kmod_plugins / "openxcvr_decoders.c").is_file():
    print("[kernel-recipe] SKIPPED: sibling kmod plugin directory not supplied "
          "(CI-runnable without the sibling); host-only kernel evidence is "
          "reported separately")
    sys.exit(0)
if not KERNEL_BUILD.is_dir():
    print(f"[kernel-recipe] SKIPPED: no kernel build tree at {KERNEL_BUILD}")
    sys.exit(0)

stage = scratch / "kernel" / "plugins" / "cmis"
vendor_cmis = scratch / "kernel" / "vendor" / "openxcvr" / "bindings" / "c"
stage.mkdir(parents=True)
vendor_cmis.mkdir(parents=True)
# This worktree's generated C and public headers occupy the exact relative
# vendor path the unchanged kmod wrapper includes, so nothing from the
# sibling's vendored tree is ever compiled.
shutil.copytree(bindings / "include", vendor_cmis / "include")
shutil.copytree(generated, vendor_cmis / "generated")
for shim in SHIMS:
    shutil.copy2(kmod_plugins / shim, stage / shim)
shutil.copy2(kmod_plugins / "openxcvr_decoders.c", stage / "openxcvr_decoders.c")

# Test-only companions (never written to the sibling repo): the module glue
# modpost requires, and a second wrapper that pulls in the two NEW generated
# revision sources the unchanged kmod wrapper does not reference yet.
(stage / "staged_module_glue.c").write_text(
    "// SPDX-License-Identifier: GPL-2.0-or-later\n"
    "// Test-only staged module glue; never part of the kmod repository.\n"
    "#include <linux/module.h>\n"
    "MODULE_LICENSE(\"GPL\");\n"
)
(stage / "checked_pages_revision.c").write_text(
    "// SPDX-License-Identifier: GPL-2.0-or-later\n"
    "// Test-only staged inclusion of the NEW generated revision-checked\n"
    "// decoders. The kmod's own openxcvr_decoders.c stays unchanged; the\n"
    "// actual kmod migration is a separate, separately verified step.\n"
    '#include "../../vendor/openxcvr/bindings/c/generated/pages/'
    'cmis_page01h_advertising_revision.c"\n'
    '#include "../../vendor/openxcvr/bindings/c/generated/pages/'
    'cmis_page11h_lane_status_revision.c"\n'
)
(stage / "Kbuild").write_text(
    "obj-m := staged_decoders.o checked_pages.o\n"
    "staged_decoders-y := openxcvr_decoders.o staged_module_glue.o\n"
    "checked_pages-y := checked_pages_revision.o staged_module_glue.o\n"
    "ccflags-y += -I$(src)/../../vendor/openxcvr/bindings/c/include\n"
    "ccflags-y += -I$(src)\n"
    "ccflags-y += -I$(src)/../../vendor/openxcvr/bindings/c/generated\n"
    "ccflags-y += -I$(src)/../../vendor/openxcvr/bindings/c/generated/pages\n"
    "ccflags-y += -I$(src)/../../vendor/openxcvr/bindings/c/generated/enums\n"
    "# The supported LTS kernels build with -Wdeclaration-after-statement\n"
    "# (warnings, not errors): mirror it for every staged object. Only the\n"
    "# NEW revision-checked sources get -Werror; the unchanged kmod wrapper\n"
    "# compiles the pre-existing raw pages, whose historical warnings stay\n"
    "# visible without turning this test into a raw-page repair.\n"
    "ccflags-y += -Wdeclaration-after-statement\n"
    "ccflags-y += -fstack-usage\n"
    "CFLAGS_checked_pages_revision.o += -Werror\n"
)
proc = subprocess.run(
    ["make", "-C", os.fspath(KERNEL_BUILD),
     f"M={scratch / 'kernel' / 'plugins' / 'cmis'}", "modules"],
    capture_output=True, text=True,
)
print(f"[kernel-recipe] make -C {KERNEL_BUILD} M=.../plugins/cmis modules "
      f"(kernel {os.uname().release}, compiler:")
proc_cc = subprocess.run([os.fspath(cc), "--version"], capture_output=True, text=True)
print(f"    {proc_cc.stdout.splitlines()[0]})")
output = proc.stdout + proc.stderr
if proc.returncode != 0:
    print(output, end="")
    sys.exit(f"kernel-recipe gate FAILED (exit {proc.returncode})")
errors = [l for l in output.splitlines() if "error:" in l]
if errors:
    print(output, end="")
    sys.exit("kernel-recipe gate FAILED: compiler errors under the kernel recipe")
for line in output.splitlines():
    if "error:" in line or "warning:" in line:
        if "revision" in line:
            print(output, end="")
            sys.exit(
                "kernel-recipe gate FAILED: the new revision-checked sources "
                "are not clean under the kernel recipe")
        # Pre-existing raw vendored pages and the staged glue report their
        # historical warnings verbatim; they are observations here, not
        # failures of this task's new sources.
        print(f"[kernel-recipe] observation: {line.strip()}")
kernel_frames = {}
for su in stage.glob("*.su"):
    for line in su.read_text().splitlines():
        where, size, _kind = line.split("\t")
        func = where.rsplit(":", 1)[1] if ":" in where else where
        kernel_frames[(su.name, func)] = int(size)
for (name, func), size in sorted(kernel_frames.items()):
    print(f"[kernel-recipe] stack usage {name}: {func} = {size} bytes")
print("[kernel-recipe] gate passed: the unchanged kmod wrapper compiled this "
      "worktree's raw pages and the new revision sources built with "
      "-Wdeclaration-after-statement -Werror under the running kernel's "
      "recipe (staged under scratch; no claim of kmod migration or hardware "
      "proof)")
print("checked page portability: all gates that ran passed")
