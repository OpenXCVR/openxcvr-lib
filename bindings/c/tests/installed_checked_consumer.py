#!/usr/bin/env python3
"""Install the C binding into a scratch prefix and call BOTH new
revision-checked page APIs through the installed headers and libraries:
statically against the installed archive named by path (with the flags
`pkg-config --static --libs` carries beyond the library itself), and
dynamically with `pkg-config --cflags --libs` plus an rpath into the prefix,
run with the loader search paths unset. Mirrors the installed-consumer
pattern of installed_application_records.py; never installs outside the
build directory's scratch prefix.

usage: installed_checked_consumer.py <build_dir> <source_dir> <cc> <shared_suffix>
"""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

build, source, compiler, suffix = sys.argv[1:5]
build = Path(build)
prefix = build / "prefix-checked"
if prefix.exists():
    shutil.rmtree(prefix)
subprocess.run(["cmake", "--install", build, "--prefix", str(prefix)], check=True)
# The installed files the checked consumer actually consumes.
for rel in (
    "lib/libopenxcvr_c.a",
    f"lib/libopenxcvr_c{suffix}",
    "lib/pkgconfig/openxcvr-c.pc",
    "include/openxcvr/openxcvr.h",
    "include/openxcvr_generated.h",
    "include/pages/cmis_page01h_advertising_revision.h",
    "include/pages/cmis_page11h_lane_status_revision.h",
):
    if not (prefix / rel).is_file():
        sys.exit(f"cmake --install did not install {rel}")
# Relocate only the configured prefix inside the installed .pc, like the
# other installed-consumer tests.
pc = prefix / "lib/pkgconfig/openxcvr-c.pc"
text = pc.read_text()
text = "\n".join(
    f"prefix={prefix}" if line.startswith("prefix=") else line
    for line in text.splitlines()
) + "\n"
pc.write_text(text)
env = dict(os.environ, PKG_CONFIG_LIBDIR=str(pc.parent))
for key in ("PKG_CONFIG_PATH", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"):
    env.pop(key, None)

for shared in (False, True):
    cmd = ["pkg-config", "--cflags", "--libs", "openxcvr-c"]
    if not shared:
        cmd.insert(1, "--static")
    flags = shlex.split(subprocess.check_output(cmd, text=True, env=env))
    if shared:
        # The installed shared library must be self-contained: the consumer's
        # link flags must not name -lm, and the static archive is removed so
        # only the shared library can satisfy the references.
        assert "-lm" not in flags, flags
        (prefix / "lib/libopenxcvr_c.a").unlink()
        flags.append(f"-Wl,-rpath,{prefix / 'lib'}")
    else:
        # Static: the archive named by path, plus the flags --static carries
        # beyond the library itself (-l and -L removed so the shared library
        # cannot satisfy the reference).
        flags.remove("-lopenxcvr_c")
        flags.insert(0, str(prefix / "lib/libopenxcvr_c.a"))
        flags = [f for f in flags if not f.startswith("-L")]
    executable = build / ("installed-checked-dynamic" if shared
                          else "installed-checked-static")
    subprocess.run(
        [compiler, "-std=c99", "-Wall", "-Wextra", "-Werror", "-UNDEBUG",
         str(Path(source) / "tests/installed_checked_consumer.c"),
         "-o", str(executable), *flags],
        env=env, check=True,
    )
    # Loader search paths stay unset; the rpath locates the shared library.
    subprocess.run([str(executable)], env=env, check=True)

print("Installed checked API: static and dynamic consumers of both pages passed")
