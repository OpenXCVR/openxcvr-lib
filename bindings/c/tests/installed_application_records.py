#!/usr/bin/env python3
"""Install and call the new records API through pkg-config, static and shared."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

build, source, compiler, suffix = sys.argv[1:]
prefix = Path(build) / "prefix-rich"
if prefix.exists():
    shutil.rmtree(prefix)
subprocess.run(["cmake", "--install", build, "--prefix", str(prefix)], check=True)
pc = prefix / "lib/pkgconfig/openxcvr-c-rich.pc"
# Same pattern as the existing installed-consumer tests: relocate only the
# configured prefix; use the installed libraries/headers and dependency flags.
text = pc.read_text()
text = "\n".join(f"prefix={prefix}" if line.startswith("prefix=") else line for line in text.splitlines()) + "\n"
pc.write_text(text)
env = dict(os.environ, PKG_CONFIG_LIBDIR=str(pc.parent))
for key in ("PKG_CONFIG_PATH", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"):
    env.pop(key, None)
for shared in (False, True):
    cmd = ["pkg-config", "--cflags", "--libs", "openxcvr-c-rich"]
    if not shared:
        cmd.insert(1, "--static")
    flags = shlex.split(subprocess.check_output(cmd, text=True, env=env))
    if shared:
        # The installed shared component must be self-contained: neither static
        # archive may be consulted by its consumer's linker or loader.
        (prefix / "lib/libopenxcvr_rich_ffi.a").unlink()
        (prefix / "lib/libopenxcvr_c_rich.a").unlink()
        assert "-lm" not in flags
        assert not any("rich_ffi" in flag for flag in flags)
        flags.append(f"-Wl,-rpath,{prefix / 'lib'}")
    else:
        flags.remove("-lopenxcvr_c_rich")
        flags.insert(0, str(prefix / "lib/libopenxcvr_c_rich.a"))
    executable = Path(build) / f"installed-rich-{'dynamic' if shared else 'static'}"
    subprocess.run([compiler, "-std=c99", "-Wall", "-Wextra", "-Werror", "-UNDEBUG",
                    str(Path(source) / "tests/application_records.c"), "-o", str(executable), *flags],
                   env=env, check=True)
    subprocess.run([str(executable)], env=env, check=True)
assert (prefix / f"lib/libopenxcvr_c_rich{suffix}").is_file()
print("Installed rich API: static and dynamic consumers passed")
