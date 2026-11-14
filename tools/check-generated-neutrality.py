#!/usr/bin/env python3
"""Generated-neutrality scope, context admission and replay (stages 1–3b).

This is deliberately not a shell/Rust interpreter. Only the audited regeneration
recipe grammar and output-relative emitter path grammar are understood. A new
construct must be qualified here before it can supply ownership evidence.
Task mode consumes explicit parent evidence and creates separate immutable receipts.
Finish validates historical bindings then independently regenerates current schemas.
The exact nine-file exercise retains removal/restoration evidence in both modes.
Manifest digests hash UTF-8 canonical JSON (sorted keys, compact separators),
without a trailing newline. File digests hash the actual bytes, not git blob IDs.

Full replay needs caller-supplied qualification context (--context).
Synthetic qualification tests run independently of that context.
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import ctypes
import errno
import fcntl
from functools import wraps
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import secrets
import signal
import stat
import subprocess
import sys
import tempfile
import tomllib


class NeutralityError(ValueError):
    """An input cannot establish complete, unambiguous generated ownership."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(value) -> str:
    return sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                             ensure_ascii=True).encode("utf-8"))


def relative_path(value: str) -> str:
    path = PurePosixPath(value)
    if (not value or path.is_absolute() or str(path) != value
            or any(part in (".", "..", ".git") for part in path.parts)
            or "\\" in value or "\x00" in value):
        raise NeutralityError(f"unsafe or noncanonical relative path: {value!r}")
    return value


def checked_path(repo: Path, relative: str) -> Path:
    """Reject symlinks in every component, including dangling root symlinks."""
    relative_path(relative)
    path = repo
    for part in PurePosixPath(relative).parts:
        path = path / part
        try:
            mode = path.lstat().st_mode
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(mode):
            raise NeutralityError(f"symlink in generated scope/source: {path}")
    return path


# C13: every git child reads the recorded objects themselves. With replacement
# objects enabled, rev-parse names the requested commit while ls-tree, show,
# cat-file and ancestry queries read its refs/replace/ substitute.
_GIT = ('git', '--no-replace-objects')


def _git(repo: Path, *args: str) -> bytes:
    result = subprocess.run([*_GIT, "-C", str(repo), *args], capture_output=True)
    if result.returncode:
        raise NeutralityError(f"git {' '.join(args)} failed: "
                              + result.stderr.decode("utf-8", errors="replace").strip())
    return result.stdout


class Sources:
    """Read the same rule sources from the filesystem or a pinned git tree."""

    def __init__(self, repo: Path, revision: str | None):
        self.repo = Path(repo).resolve(strict=True)
        self.revision = revision
        self.hashes = {}
        self.tree = None
        if revision is not None:
            # Resolve once, avoiding mutable refs during subsequent reads.
            self.revision = _git(self.repo, "rev-parse", "--verify",
                                 f"{revision}^{{tree}}").decode().strip()
            self.tree = _git_entries(self.repo, self.revision)

    def paths(self, pattern: str) -> list[str]:
        if '/**/' in pattern:
            prefix, suffix = pattern.split('/**/', 1)
            if self.tree is not None:
                for path, (mode, _, _) in self.tree.items():
                    if (path == prefix or prefix.startswith(path + '/')
                            or path.startswith(prefix + '/')) and mode in ('120000', '160000'):
                        raise NeutralityError(f"non-file emitter source tree: {path}")
                return sorted(p for p in self.tree if p.startswith(prefix + '/')
                              and PurePosixPath(p).match(suffix))
            base = checked_path(self.repo, prefix)
            paths = []
            for directory, children, files in os.walk(base):
                for child in children:
                    checked_path(self.repo, (Path(directory) / child).relative_to(self.repo).as_posix())
                for name in files:
                    path = (Path(directory) / name).relative_to(self.repo).as_posix()
                    checked_path(self.repo, path)
                    if PurePosixPath(path).match(suffix):
                        paths.append(path)
            return sorted(paths)
        if self.tree is not None:
            return sorted(p for p in self.tree if PurePosixPath(p).match(pattern))
        return sorted(p.relative_to(self.repo).as_posix()
                      for p in self.repo.glob(pattern))

    def read(self, path: str) -> str:
        relative_path(path)
        if self.tree is not None:
            entry = self.tree.get(path)
            if entry is None or entry[0] not in ("100644", "100755"):
                raise NeutralityError(f"missing/non-file rule source: {path}")
            data = _git(self.repo, "cat-file", "blob", entry[2])
        else:
            source = checked_path(self.repo, path)
            if not source.is_file():
                raise NeutralityError(f"missing/non-file rule source: {path}")
            data = source.read_bytes()
        self.hashes[path] = sha256(data)
        return data.decode("utf-8")


# Every non-output logical line of the existing recipes is accounted for. In
# particular, arbitrary commands, redirects, eval/source, and assignments are
# not accepted as harmless boilerplate. Comments do not declare output domains.
_SHELL_BOILERPLATE = [
    r'set -euo pipefail',
    re.escape('REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"'),
    re.escape('SPECS=("${@}")'),
    re.escape('if [[ ${#SPECS[@]} -eq 0 ]]; then'),
    r'SPECS=\([a-z0-9 -]+\)', r'fi', r'done', r'esac', r'case "\$(?:spec|lang)" in',
    re.escape('cd "$REPO_ROOT"'), re.escape('for spec in "${SPECS[@]}"; do'),
    r'for lang in [a-z ]+; do',
    r'cmis\|sff-8636\|sff-8472\)(?: ;;)?', r'\*\)',
    r'\*\) echo "Unknown spec: \$spec" >&2; exit 1 ;;',
    r'exit 1', r';;',
    r'regen_spec\(\) \{', r'\}', re.escape('local spec="$1" crate="$2"'),
    re.escape('cargo fmt -p "$crate"'),
]


def _inert_echo_text(text: str) -> bool:
    """True only for literal echo content and simple variable references.

    The recipe grammar describes an intentionally inert progress message, not a
    shell string. Command substitution, backticks, array/parameter expansions
    beyond a plain named reference, escapes and process substitution are never
    inert output, so they fail closed instead of being trusted to an `echo`
    prefix. A recognizer that cannot establish inertness refuses.
    """
    index = 0
    while index < len(text):
        character = text[index]
        if character == '$':
            match = re.match(r'\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)',
                             text[index:])
            if match is None:
                return False
            index += match.end()
            continue
        if character in '\\`"\n' or (character in '<>' and index + 1 < len(text)
                                      and text[index + 1] == '('):
            return False
        index += 1
    return True


def _inert_echo(line: str) -> bool:
    """Recognize `echo "<inert text>"` with an optional stderr redirection."""
    prefix = 'echo "'
    if not line.startswith(prefix):
        return False
    rest = line[len(prefix):]
    close = rest.find('"')
    if close < 0:
        return False
    return rest[close + 1:] in ('', ' >&2') and _inert_echo_text(rest[:close])


def _shell_recipe(text: str, source: str) -> tuple[list[tuple[str, str]], list[str]]:
    """Return (language, dedicated root) declarations and child recipe names."""
    lines = re.sub(r"\\\n[ \t]*", " ", text).splitlines()
    declarations, children, loops, invocations = [], [], [], []
    rust_template = None
    current_lang = PurePosixPath(source).stem.removeprefix("regen-")
    blocks, loop_stack, dispatch_loops, output_loops = [], [], [], []
    first_command = True
    for raw in lines:
        line = re.sub(r'[ \t]+', ' ', raw.strip())
        if not line or line.startswith("#"):
            continue
        # C11: without fail-fast, a failing generator followed by a later
        # successful command exits 0 over unchanged generated bytes.
        if first_command and line != 'set -euo pipefail':
            raise NeutralityError(f"recipe must set -euo pipefail before its first command: {source}")
        first_command = False
        opener = None
        if line.startswith('for ') and line.endswith('; do'):
            opener = 'done'
        elif line.startswith('if ') and line.endswith('; then'):
            opener = 'fi'
        elif line.startswith('case ') and line.endswith(' in'):
            opener = 'esac'
        elif line == 'regen_spec() {':
            opener = '}'
        if opener:
            blocks.append(opener)
            match = re.fullmatch(r'for lang in ([a-z ]+); do', line)
            loop_stack.append(len(loops) if match else None)
            if match:
                languages = match[1].split()
                if len(languages) != len(set(languages)):
                    raise NeutralityError(f"ambiguous language loop: {source}")
                loops.append(languages)
        elif line in ('done', 'fi', 'esac', '}'):
            if not blocks or blocks.pop() != line:
                raise NeutralityError(f"unbalanced recipe structure: {source}: {line}")
            loop_stack.pop()
        elif re.match(r'(?:[a-z0-9|*-]+)\)', line) and (not blocks or blocks[-1] != 'esac'):
            raise NeutralityError(f"unresolved recipe case: {source}: {line}")
        match = re.fullmatch(r'(?:([a-z]+)\) )?OUT="\$REPO_ROOT/([^"\n]+)"(?: ;;)?', line)
        if match:
            declarations.append((match[1] or current_lang, match[2]))
            continue
        if line == 'local out="$REPO_ROOT/crates/$crate/src/generated"':
            if rust_template is not None:
                raise NeutralityError(f"duplicate Rust output template: {source}")
            rust_template = 'crates/{crate}/src/generated'
            continue
        match = re.fullmatch(r'([a-z0-9-]+)\)\s+regen_spec ([a-z0-9-]+) ([a-z0-9-]+) ;;', line)
        if match:
            if match[1] != match[2] or rust_template is None:
                raise NeutralityError(f"unresolved regen_spec invocation: {source}: {line}")
            declarations.append(("rust", rust_template.format(crate=match[3])))
            continue
        if line.startswith("cargo run "):
            # Account for the entire command, not just a --out substring.
            match = re.fullmatch(
                r'cargo run -q -p openxcvr-codegen -- generate '
                r'(?:--schemas "\$REPO_ROOT/schemas" )?--spec "\$spec" '
                r'(?:(?:--lang ("\$lang"|[a-z]+) --out "\$(OUT|out)")'
                r'|(?:--out "\$(OUT|out)" --lang ("\$lang"|[a-z]+)))', line)
            if not match:
                raise NeutralityError(f"unparsed generation invocation: {source}: {line}")
            invocations.append((match[1] or match[4], match[2] or match[3]))
            if source.endswith('regen-all.sh'):
                active = [i for i in loop_stack if i is not None]
                if len(active) != 1:
                    raise NeutralityError(f"unresolved output language loop: {source}")
                output_loops.append(active[0])
            continue
        if line == '"$REPO_ROOT/tools/regen-$lang.sh" "${SPECS[@]}"':
            active = [i for i in loop_stack if i is not None]
            if len(active) != 1:
                raise NeutralityError(f"unresolved child dispatch loop: {source}")
            dispatch_loops.append(active[0])
            children.append("$lang")
            continue
        if _inert_echo(line):
            continue
        if not any(re.fullmatch(pattern, line) for pattern in _SHELL_BOILERPLATE):
            raise NeutralityError(f"unparsed regeneration source: {source}: {line}")
    if blocks:
        raise NeutralityError(f"unclosed recipe structure: {source}")
    if not declarations or not invocations:
        raise NeutralityError(f"missing output declarations/invocation: {source}")
    if len(declarations) != len(set(declarations)):
        raise NeutralityError(f"duplicate output declarations: {source}")
    if source.endswith("regen-all.sh"):
        if (children != ["$lang"] or len(loops) != 2 or len(output_loops) != 1
                or dispatch_loops == output_loops):
            raise NeutralityError(f"unresolved child/language loops: {source}")
        children = [f"tools/regen-{lang}.sh" for lang in loops[dispatch_loops[0]]]
        if sorted(loops[output_loops[0]]) != sorted(lang for lang, _ in declarations):
            raise NeutralityError(f"unresolved output language cases: {source}")
        expected = [('"$lang"', 'OUT')]
    else:
        if loops or children:
            raise NeutralityError(f"unexpected child/language loop: {source}")
        expected = [(current_lang, 'out' if current_lang == 'rust' else 'OUT')]
        if any(lang != current_lang for lang, _ in declarations):
            raise NeutralityError(f"ambiguous language ownership: {source}")
    if invocations != expected:
        raise NeutralityError(f"output declarations do not bind invocation: {source}")
    return declarations, children


def _balanced(text: str, start: int) -> tuple[str, int]:
    """Read paired delimiters, respecting Rust strings; return body/end."""
    opening = text[start]
    closing = {'(': ')', '{': '}', '[': ']'}[opening]
    depth, quoted, escaped = 1, False, False
    for index in range(start + 1, len(text)):
        char = text[index]
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return text[start + 1:index], index + 1
    raise NeutralityError("unbalanced emitter declaration")


def _rust_argument(argument: str) -> None:
    argument = argument.strip()
    if argument in ("&header_rel", "sub"):
        return
    if argument.startswith('format!('):
        body, end = _balanced(argument, len('format!'))
        if end != len(argument):
            raise NeutralityError(f"unresolved emitter format: {argument}")
        match = re.fullmatch(r'"([^"\\]+)"(?:\s*,\s*(.+))?', body, re.S)
        if not match or (match[2] is None and match[1] != '{file_stem}.c') or (
                match[2] is not None and match[2].strip() not in (
                    'schema.id', 'page.id', 'rev.dirname()',
                    'camel_to_snake(&schema.id)', 'JavaNaming.type_name(&schema.id)')):
            raise NeutralityError(f"unresolved emitter format: {argument}")
        value = re.sub(r'\{[^{}]*\}', '*', match[1])
    else:
        match = re.fullmatch(r'"([^"\\]+)"', argument)
        if not match:
            raise NeutralityError(f"unresolved emitter path component: {argument}")
        value = match[1]
    relative_path(value)


def _rust_path(expression: str) -> None:
    expression = expression.strip().removeprefix('&')
    match = re.match(r'(out_dir|out|dir)\b', expression)
    if not match:
        raise NeutralityError(f"unresolved emitter destination: {expression}")
    rest = expression[match.end():].strip()
    while rest:
        match = re.match(r'\.join\s*\(', rest)
        if not match:
            raise NeutralityError(f"unresolved emitter destination: {expression}")
        argument, end = _balanced(rest, match.end() - 1)
        _rust_argument(argument)
        rest = rest[end:].strip()


def _audit_emitter(text: str, source: str) -> list[str]:
    """Validate every filesystem declaration and the variables feeding writes.

    Path expressions may only remain under the caller's output root. Unknown
    filesystem APIs or destination syntax are rejected, even if existing writes
    still parse. This includes newly introduced writers in other emitter files.
    """
    text = re.sub(r'"(?:\\.|[^"\\])*"|/\*.*?\*/|//[^\n]*',
                  lambda m: m[0] if m[0].startswith('"') else ' ', text, flags=re.S)
    # Mask literals before token checks: generated code strings are not APIs.
    tokens = re.sub(r'"(?:\\.|[^"\\])*"', lambda m: ' ' * len(m[0]), text, flags=re.S)
    if re.search(r'\b(?:Command|File|OpenOptions|TcpStream|UdpSocket|UnixStream|UnixDatagram)\b'
                 r'|\b(?:process|libc|unsafe)\b|\buse\s+std::(?:io|net|os)\b', tokens):
        raise NeutralityError(f"unresolved non-fs write channel: {source}")
    declarations = []
    calls = list(re.finditer(r'(?:std::)?fs::([A-Za-z_]+)\s*\(', text))
    if re.search(r'\b(?:File|OpenOptions)\s*::|\b(?:std::)?fs\s*::\s*\{'
                 r'|\buse\s+(?:std::)?fs\b|\buse\s+std::io\b', text):
        raise NeutralityError(f"unresolved filesystem API: {source}")
    if len(calls) != len(re.findall(r'\bfs\s*::', text)):
        raise NeutralityError(f"unresolved filesystem declaration: {source}")
    for call in calls:
        operation = call[1]
        if operation not in ('write', 'create_dir_all', 'read_dir'):
            raise NeutralityError(f"unresolved filesystem API {operation}: {source}")
        body, _ = _balanced(text, call.end() - 1)
        # First argument's separator is outside nested parentheses/strings.
        quoted, depth, separator = False, 0, len(body)
        for index, char in enumerate(body):
            if char == '"':
                quoted = not quoted
            elif not quoted:
                if char == '(':
                    depth += 1
                elif char == ')':
                    depth -= 1
                elif char == ',' and depth == 0:
                    separator = index
                    break
        destination = body[:separator].strip()
        if destination == '&out_file':
            assignments = re.findall(r'let out_file = (.*?);', text, flags=re.S)
            if not assignments:
                raise NeutralityError(f"unresolved out_file binding: {source}")
            for assignment in assignments:
                # Every branch must return a validated output-relative path.
                masked = re.sub(r'"(?:\\.|[^"\\])*"',
                                lambda m: ' ' * len(m[0]), assignment)
                branches = list(re.finditer(r'\{\s*(out\.join.*?)\s*\}', masked, re.S))
                if not branches:
                    raise NeutralityError(f"unresolved out_file branches: {source}")
                residue = assignment
                for branch in reversed(branches):
                    start, end = branch.span(1)
                    _rust_path(assignment[start:end])
                    residue = residue[:start] + 'DEST' + residue[end:]
                if not re.fullmatch(
                        r'\s*if \w+\s*\{\s*DEST\s*\}'
                        r'(?:\s*else if \w+\s*\{\s*DEST\s*\})*'
                        r'\s*else\s*\{\s*DEST\s*\}\s*', residue):
                    raise NeutralityError(f"unresolved out_file branches: {source}")
        else:
            _rust_path(destination)
        declarations.append(f'{operation}({destination})')
    # Audit aliases rather than assuming a variable called dir is inside out.
    if calls:
        # Only filesystem-owning functions use out/out_dir as paths. Other
        # emitters legitimately use a local String named out to build code.
        functions = []
        for fn in re.finditer(r'\bfn\s+\w+\s*\(', tokens):
            _, signature_end = _balanced(text, fn.end() - 1)
            body_start = tokens.find('{', signature_end)
            if body_start < 0:
                raise NeutralityError(f"unresolved emitter function: {source}")
            _, end = _balanced(text, body_start)
            functions.append((fn.start(), end, fn.end() - 1))
        regions = set()
        for call in calls:
            owners = [(start, end, params) for start, end, params in functions
                      if start < call.start() < end]
            if owners:
                start, end, params = min(owners)
                if any(start < other < end for other, _, _ in functions):
                    raise NeutralityError(f"nested filesystem-owning function: {source}")
                signature, _ = _balanced(text, params)
                names = re.findall(r'\b(out|out_dir|dir|sub|out_file|header_rel|file_stem)\b', signature)
                if names and (len(names) != 1 or names[0] not in ('out', 'out_dir') or
                              not re.search(r'\b' + names[0] + r'\s*:\s*&\s*'
                                            r'(?:std\s*::\s*path\s*::\s*)?Path\s*(?:,|$)', signature)):
                    raise NeutralityError(f"rebound emitter function parameter: {source}")
                regions.add((start, end))
            else:
                regions.add((0, len(text)))
        text = '\n'.join(text[start:end] for start, end in sorted(regions))
        protected = r'\b(?:out|out_dir|dir|sub|out_file|header_rel|file_stem)\b'
        region_tokens = re.sub(r'"(?:\\.|[^"\\])*"', lambda m: ' ' * len(m[0]), text, flags=re.S)
        if re.search(r'\bmut\s+' + protected + r'|&\s*mut\s+' + protected +
                     r'|' + protected + r'\s*\.\s*as_mut_\w*\s*\('
                     r'|\b(?:std\s*::\s*)?mem\s*::\s*(?:swap|replace|take)\s*\(', region_tokens):
            raise NeutralityError(f"mutable emitter output root: {source}")
        # Only these audited aliases and the literal sub loop may bind names.
        bindings = re.sub(r'for sub in \["enums", "pages"\]', 'SAFE_SUB_LOOP', text)
        bindings = re.sub(r'"(?:\\.|[^"\\])*"', lambda m: ' ' * len(m[0]), bindings, flags=re.S)
        bindings = re.sub(r'\blet (?:dir|out_file|header_rel|file_stem) =', 'ALIAS =', bindings)
        # The CLI destructuring is the original output root declaration.
        bindings = re.sub(r'Commands::Generate\s*\{\s*schemas,\s*spec,\s*out,\s*lang,\s*\}',
                          'CLI_OUTPUT', bindings)
        patterns = (r'\blet\s+([^=;]+)\s*=', r'\bfor\s+(.+?)\s+in\b',
                    r'\|([^|\n]*)\|', r'([^;=>]+)=>')
        for pattern in patterns:
            for binding in re.finditer(pattern, bindings, re.S):
                if re.search(protected, binding[1]):
                    raise NeutralityError(f"rebound emitter binding: {source}")
        # Inputs out/out_dir must not be rebound. No mutation/shadowing may
        # redirect a syntactically output-relative write outside its root.
        if re.search(r'\b(?:out|out_dir|sub)\s*=|\blet\s+(?:mut\s+)?(?:out|out_dir)\b'
                     r'|\b(?:out|out_dir|dir|sub|out_file)\.(?:push|push_str|pop|set_extension|set_file_name)\s*\(', text):
            raise NeutralityError(f"rebound emitter output root: {source}")
        if re.search(r'\b(?:dir|out_file|header_rel|file_stem)\s*=',
                     re.sub(r'let (?:dir|out_file|header_rel|file_stem) = .*?;', '', text, flags=re.S)):
            raise NeutralityError(f"rebound emitter destination: {source}")
        aliases = re.findall(r'let dir = (.*?);', text, flags=re.S)
        if any('dir' == re.sub(r'^&', '', d.split('(', 1)[1]).split('.', 1)[0]
               for d in declarations) and not aliases:
            raise NeutralityError(f"unresolved dir binding: {source}")
        if len(aliases) > 1:
            raise NeutralityError(f"ambiguous dir binding: {source}")
        for alias in aliases:
            if not re.match(r'out(?:_dir)?\.join', alias.strip()):
                raise NeutralityError(f"unresolved dir binding: {source}")
            _rust_path(alias)
        if '{file_stem}' in text and not re.search(
                r'let file_stem = camel_to_snake\(&schema.id\);', text):
            raise NeutralityError(f"unresolved file_stem binding: {source}")
        if 'sub' in text and 'out.join(sub)' in text:
            if 'for sub in ["enums", "pages"]' not in text:
                raise NeutralityError(f"unresolved sub-directory declaration: {source}")
        if '&header_rel' in text and not re.search(
                r'let header_rel = format!\("pages/\{file_stem\}\.h"\);', text):
            raise NeutralityError(f"unresolved header_rel binding: {source}")
    return declarations


def validate_rules(rules: list[dict]) -> None:
    roots = []
    for rule in rules:
        root = relative_path(rule['root'])
        if any(char in root for char in '$`*?{}[]'):
            raise NeutralityError(f"unresolved generated root: {root}")
        if rule['rule'] != 'recursive-files':
            raise NeutralityError(f"unknown ownership rule: {rule}")
        # These recipes explicitly select dedicated generated directories, not
        # broad bindings/crates roots. No handwritten exception is declared.
        if PurePosixPath(root).name not in ('generated', '_generated', 'Generated'):
            raise NeutralityError(f"not a dedicated generated root: {root}")
        if any(root == old or root.startswith(old + '/') or old.startswith(root + '/')
               for old in roots):
            raise NeutralityError(f"duplicate/overlapping generated ownership: {root}")
        roots.append(root)
    if not roots:
        raise NeutralityError("empty generated scope")


def _emitter_language_guard(text: str) -> set[str] | None:
    """Read the audited `!matches!(lang.as_str(), "a" | "b")` language guard.

    A deterministic bounded token scan replaces the previous repeated
    subexpression pattern, whose overlapping optional separator repetition
    backtracked superlinearly on malformed input (CodeQL alert 38). Every step
    consumes input, so a malformed or ambiguous language list is refused in
    linear time; complete language coverage is still checked by the caller.
    """
    prefix = 'if !matches!('
    start = text.find(prefix)
    if start < 0:
        return None
    position = start + len(prefix)

    def skip_space() -> None:
        nonlocal position
        while position < len(text) and text[position] in ' \t\n\r':
            position += 1

    def literal(expected: str) -> bool:
        nonlocal position
        skip_space()
        if not text.startswith(expected, position):
            return False
        position += len(expected)
        return True

    if not literal('lang.as_str()') or not literal(','):
        return None
    languages = set()
    while True:
        skip_space()
        if position >= len(text) or text[position] != '"':
            return None
        close = text.find('"', position + 1)
        if close < 0:
            return None
        token = text[position + 1:close]
        position = close + 1
        if re.fullmatch(r'[a-z]+', token) is None or token in languages:
            return None
        languages.add(token)
        if not literal('|'):
            break
    skip_space()
    if not languages or position >= len(text) or text[position] != ')':
        return None
    return languages


def derive_scope(repo: Path, revision: str | None = None) -> dict:
    """Derive complete domains and auditable source hashes without generating."""
    sources = Sources(repo, revision)
    scripts = sources.paths('tools/regen-*.sh')
    if 'tools/regen-all.sh' not in scripts:
        raise NeutralityError("missing tools/regen-all.sh; stage-1 requires current recipes")
    rules, children, languages = [], [], set()
    for source in scripts:
        declarations, child_scripts = _shell_recipe(sources.read(source), source)
        children.extend(child_scripts)
        for language, root in declarations:
            languages.add(language)
            rules.append({'root': root, 'rule': 'recursive-files', 'source': source,
                          'source_sha256': sources.hashes[source]})
    if sorted(children) != sorted(set(scripts) - {'tools/regen-all.sh'}):
        raise NeutralityError("regen-all child coverage differs from declared recipes")
    validate_rules(rules)
    main = 'crates/openxcvr-codegen/src/main.rs'
    text = sources.read(main)
    if _emitter_language_guard(text) != languages:
        raise NeutralityError("unresolved emitter language coverage")
    emitters = sources.paths('crates/openxcvr-codegen/src/emitters/**/*.rs')
    rust_pages = 'crates/openxcvr-codegen/src/emitters/rust/pages.rs'
    if rust_pages not in emitters:
        raise NeutralityError(f"missing emitter source: {rust_pages}")
    destinations = []
    for source in [main, *emitters]:
        declarations = _audit_emitter(text if source == main else sources.read(source), source)
        destinations.extend({'source': source, 'declaration': declaration}
                            for declaration in declarations)
        if source in (main, rust_pages) and not declarations:
            raise NeutralityError(f"missing emitter destinations: {source}")
    return {'scope_sources': [{'path': p, 'sha256': h}
                              for p, h in sorted(sources.hashes.items())],
            'output_rules': sorted(rules, key=lambda r: r['root']),
            'emitter_destinations': destinations}


def union_rules(*scopes: dict) -> list[dict]:
    """Union prelaunch/post-edit domains, preserving each source declaration.

    Scopes are passed oldest first. On a shared root the later (current) scope's
    rule wins, so the record names the script that actually runs (C9).
    """
    by_root = {}
    for scope in scopes:
        validate_rules(scope['output_rules'])
        for rule in scope['output_rules']:
            by_root[rule['root']] = rule
    rules = sorted(by_root.values(), key=lambda r: r['root'])
    validate_rules(rules)
    return rules


def _manifest(entries: list[dict]) -> dict:
    entries.sort(key=lambda entry: entry['path'])
    paths = [entry['path'] for entry in entries]
    if len(set(paths)) != len(paths):
        raise NeutralityError("duplicate manifest paths")
    return {'manifest': entries, 'manifest_sha256': canonical_digest(entries)}


def filesystem_manifest(repo: Path, rules: list[dict]) -> dict:
    """Enumerate actual files, regardless of git tracked/ignored status."""
    repo = Path(repo).resolve(strict=True)
    validate_rules(rules)
    entries = []

    def visit(path: Path) -> None:
        mode = path.lstat().st_mode
        relative = path.relative_to(repo).as_posix()
        relative_path(relative)
        if stat.S_ISREG(mode):
            entries.append({'path': relative, 'kind': 'file',
                            'sha256': sha256(path.read_bytes())})
        elif stat.S_ISDIR(mode):
            for child in sorted(path.iterdir()):
                visit(child)
        else:
            raise NeutralityError(f"symlink/non-regular generated output: {relative}")

    for rule in rules:
        path = checked_path(repo, rule['root'])
        if path.exists():
            if not path.is_dir():
                raise NeutralityError(f"generated root is not a directory: {rule['root']}")
            visit(path)
    return _manifest(entries)


def _git_entries(repo: Path, revision: str) -> dict:
    entries = {}
    for record in _git(repo, 'ls-tree', '-r', '-z', revision).split(b'\0'):
        if not record:
            continue
        metadata, raw_path = record.split(b'\t', 1)
        path = relative_path(raw_path.decode('utf-8'))
        if path in entries:
            raise NeutralityError(f"duplicate git path: {path}")
        entries[path] = tuple(metadata.decode('ascii').split())
    return entries


def git_manifest(repo: Path, revision: str, rules: list[dict]) -> dict:
    """Recover the clean baseline from git objects, not working-tree dirt."""
    validate_rules(rules)
    entries = []
    for path, (mode, kind, oid) in _git_entries(Path(repo), revision).items():
        if any(path == rule['root'] or path.startswith(rule['root'] + '/')
               or rule['root'].startswith(path + '/') for rule in rules):
            if mode not in ('100644', '100755') or kind != 'blob':
                raise NeutralityError(f"symlink/non-file generated git object: {path}")
            if any(rule['root'] == path or rule['root'].startswith(path + '/') for rule in rules):
                raise NeutralityError(f"generated root/ancestor is a git file: {path}")
            entries.append({'path': path, 'kind': 'file',
                            'sha256': sha256(_git(Path(repo), 'cat-file', 'blob', oid))})
    return _manifest(entries)


# Parent evidence adapters, schema v1. Qualification receipts use the caller's
# qualification fields (excluding receipt path/hash) plus run_id/repo/task_id,
# schema_version and commands. source_identity is {base_sha, helper_sha256,
# test_sha256, scope_sources}. Task19 additionally supplies a parent binding
# path/hash in its qualification reference; it is never a native record field.
# Task receipts are emitted beneath evidence_root/task-<id>/<request digest>/.
# Existing requests are readbacks, not regeneration or replacement of old pins.


def require(condition: bool, message: str) -> None:
    if not condition:
        raise NeutralityError(message)


def fields(value, names: str, label: str) -> dict:
    require(isinstance(value, dict), f"{label}: expected object")
    for name in names.split():
        require(name in value, f"{label}: missing {name}")
    return value


def hex_digest(value, length=64) -> str:
    require(isinstance(value, str) and re.fullmatch(f'[0-9a-f]{{{length}}}', value) is not None,
            f"invalid {length}-hex digest: {value!r}")
    return value


def absolute_path(value, *, exists=True) -> Path:
    require(isinstance(value, str) and Path(value).is_absolute(),
            f"required absolute path: {value!r}")
    path = Path(value)
    require(str(path.resolve(strict=exists)) == value, f"non-realpath/symlink path: {value}")
    if exists:
        require(path.exists(), f"missing path: {value}")
    return path


def _json_bytes(data: bytes, label: str) -> dict:
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, f"duplicate JSON key in {label}: {key}")
            result[key] = value
        return result
    value = json.loads(data, object_pairs_hook=pairs)
    return fields(value, '', label)


class Evidence:
    """Pin all consumed parent files; never rewrite a historical input."""

    def __init__(self, repo: Path, root: Path):
        self.repo, self.root, self.pins = repo, root, {}
        require(not root.is_relative_to(repo), 'evidence_root must be outside code worktree')

    def path(self, value) -> Path:
        path = absolute_path(value)
        require(path.is_relative_to(self.root) and not path.is_relative_to(self.repo),
                f"parent evidence outside evidence_root: {path}")
        require(path.is_file(), f"non-file parent evidence: {path}")
        return path

    def read(self, value, digest=None) -> bytes:
        path = self.path(value)
        data = path.read_bytes()
        actual = sha256(data)
        if digest is not None:
            require(actual == hex_digest(digest), f"digest mismatch: {path}")
        require(str(path) not in self.pins or self.pins[str(path)] == actual,
                f"historical evidence changed: {path}")
        self.pins[str(path)] = actual
        return data

    def json(self, value, digest=None) -> dict:
        return _json_bytes(self.read(value, digest), str(value))

    def unchanged(self) -> None:
        for path, digest in self.pins.items():
            require(sha256(self.path(path).read_bytes()) == digest,
                    f"historical pin changed: {path}")


def _schema(value: dict, label: str) -> None:
    require(type(value.get('schema_version')) is int and value['schema_version'] == 1,
            f"{label}: unsupported/missing schema_version")


def _commit(repo: Path, value) -> str:
    value = hex_digest(value, 40)
    actual = _git(repo, 'rev-parse', '--verify', value + '^{commit}').decode().strip()
    require(actual == value, 'not an actual commit identity')
    return value


def _dispatch(snapshot: dict, repo: Path, run_id: str, task: str) -> tuple[str, int, dict]:
    fields(snapshot, 'run_id wave review_context', 'dispatch snapshot')
    require(snapshot['run_id'] == run_id, 'wrong dispatch run_id')
    wave = snapshot['wave']
    require(type(wave) is int and wave >= 0, 'invalid dispatch wave')
    review = fields(snapshot['review_context'], 'base_sha tasks', 'review_context')
    base = _commit(repo, review['base_sha'])
    require(isinstance(review['tasks'], list), 'review_context.tasks must be a list')
    matches = [t for t in review['tasks'] if isinstance(t, dict) and str(t.get('task_id')) == task]
    require(len(matches) == 1, 'task absent/duplicate in review_context.tasks')
    require(matches[0].get('repo') == str(repo), 'wrong review_context task repo')
    return base, wave, matches[0]


def _same_identity(value: dict, expected: dict, label: str) -> None:
    fields(value, ' '.join(expected), label)
    for name, actual in expected.items():
        require(value[name] == actual, f"{label}: wrong {name}")


def _command_records(records, evidence: Evidence) -> None:
    require(isinstance(records, list) and bool(records), 'missing qualification command results')
    for command in records:
        fields(command, 'argv cwd exit stdout_path stdout_sha256 stderr_path stderr_sha256', 'command')
        require(isinstance(command['argv'], list) and bool(command['argv'])
                and all(isinstance(a, str) for a in command['argv']), 'invalid command argv')
        require(command['cwd'] == str(evidence.repo), 'wrong command cwd')
        require(type(command['exit']) is int and command['exit'] == 0, 'unqualified command exit')
        evidence.read(command['stdout_path'], command['stdout_sha256'])
        evidence.read(command['stderr_path'], command['stderr_sha256'])


def qualification(context: dict, task: str, base: str, current: dict,
                  evidence: Evidence, helper: Path, test: Path, *, revision=None,
                  reference=None, check_link=True) -> tuple[dict, dict]:
    reference = fields(reference if reference is not None else context['qualifications'].get(task),
                       'helper_path helper_sha256 test_path test_sha256 qualification_receipt_path '
                       'qualification_receipt_sha256 source_identity', 'qualification reference')
    for role, path in (('helper', helper), ('test', test)):
        require(absolute_path(reference[role + '_path']) == path, f"wrong {role} path")
        relative = path.relative_to(evidence.repo).as_posix()
        data = path.read_bytes() if revision is None else _git(evidence.repo, 'show', revision + ':' + relative)
        require(sha256(data) == hex_digest(reference[role + '_sha256']),
                f"changed/unqualified {role} bytes")
    source = fields(reference['source_identity'],
                    'base_sha helper_sha256 test_sha256 scope_sources', 'qualified source identity')
    _commit(evidence.repo, source['base_sha'])
    require(subprocess.run([*_GIT, '-C', str(evidence.repo), 'merge-base', '--is-ancestor',
                           source['base_sha'], revision or 'HEAD'], capture_output=True).returncode == 0,
            'qualification source base ancestry mismatch')
    _same_identity(source, {'helper_sha256': reference['helper_sha256'],
                           'test_sha256': reference['test_sha256'],
                           'scope_sources': current['scope_sources']}, 'qualified source identity')
    receipt = evidence.json(reference['qualification_receipt_path'],
                            reference['qualification_receipt_sha256'])
    _schema(receipt, 'qualification receipt')
    origin_task = '10'  # Task19 consumes Task10's accepted qualification, not itself.
    expected = {k: reference[k] for k in ('helper_path', 'helper_sha256', 'test_path',
                                         'test_sha256', 'source_identity')}
    expected.update(run_id=context['run_id'], repo=str(evidence.repo), task_id=origin_task)
    _same_identity(receipt, expected, 'qualification receipt')
    _command_records(receipt.get('commands'), evidence)
    require(receipt['commands'][0]['argv'] == ['python3', 'tools/test_check_generated_neutrality.py'],
            'qualification did not run allocated suite')
    if task == '10':
        require(source['base_sha'] == base, 'stale qualification base')
    elif check_link:
        fields(reference, 'binding_path binding_sha256', 'task19 accepted qualification')
        binding = evidence.json(reference['binding_path'], reference['binding_sha256'])
        validate_binding(binding, evidence, context['run_id'], '10')
        bound = binding['qualification_reference']
        qualification_link(reference, bound, evidence, context['run_id'])
    return reference, receipt


def qualification_link(reference: dict, prior: dict, evidence: Evidence, run_id: str) -> None:
    """Follow explicit parent requalifications, never infer a link from new hashes.

    A requalification receipt retains prior_qualification_reference (the complete
    previous reference). Each edge pins that prior receipt's actual bytes.
    """
    keys = ('helper_path', 'helper_sha256', 'test_path', 'test_sha256',
            'qualification_receipt_path', 'qualification_receipt_sha256', 'source_identity')
    seen = set()
    while any(reference.get(k) != prior.get(k) for k in keys):
        digest = hex_digest(reference['qualification_receipt_sha256'])
        require(digest not in seen, 'cyclic qualification link')
        seen.add(digest)
        receipt = evidence.json(reference['qualification_receipt_path'], digest)
        _schema(receipt, 'requalification receipt')
        _same_identity(receipt, dict(run_id=run_id, repo=str(evidence.repo), task_id='10',
                       **{k: reference[k] for k in (*keys[:4], 'source_identity')}), 'requalification receipt')
        _command_records(receipt.get('commands'), evidence)
        require(receipt['commands'][0]['argv'] == ['python3', 'tools/test_check_generated_neutrality.py'],
                'requalification did not run allocated suite')
        fields(receipt, 'prior_qualification_reference', 'missing linked requalification')
        reference = fields(receipt['prior_qualification_reference'], ' '.join(keys), 'prior qualification')
        previous = evidence.json(reference['qualification_receipt_path'], reference['qualification_receipt_sha256'])
        _same_identity(previous, {k: reference[k] for k in (*keys[:4], 'source_identity')}, 'prior qualification receipt')
    evidence.read(prior['qualification_receipt_path'], prior['qualification_receipt_sha256'])


def admit_finish(context: dict, task: str, evidence: Evidence, helper: Path, test: Path) -> dict:
    repo = evidence.repo
    bindings = {}
    for historical_task in ('10', '19'):
        binding = evidence.json(context['bindings'][historical_task])
        validate_binding(binding, evidence, context['run_id'], historical_task)
        dispatch = fields(context['dispatch_evidence'][historical_task],
            'native_dispatch_path dispatch_snapshot_path dispatch_snapshot_sha256', 'finish dispatch evidence')
        absolute_path(dispatch['native_dispatch_path'], exists=False)
        require(dispatch['native_dispatch_path'] != dispatch['dispatch_snapshot_path'],
                'snapshot aliases mutable native dispatch')
        _same_identity(binding, dispatch, 'finish binding dispatch')
        require(context['dispatches'][historical_task] == binding['native_dispatch_path'],
                'wrong finish native dispatch path')
        receipt = evidence.json(binding['receipt_path'], binding['receipt_sha256'])
        envelope = evidence.json(context['prelaunch'][historical_task], receipt['envelope_sha256'])
        _same_identity(envelope, {k: binding[k] for k in ('run_id', 'repo', 'task_id', 'wave',
                       'base_sha', 'native_dispatch_path', 'dispatch_snapshot_path',
                       'dispatch_snapshot_sha256')}, 'finish prelaunch envelope')
        _schema(envelope, 'finish prelaunch envelope')
        before_scope = derive_scope(repo, binding['base_sha'])
        baseline_before = git_manifest(repo, binding['base_sha'], receipt['output_rules'])
        _same_identity(envelope, dict(scope_sources=before_scope['scope_sources'],
            output_rules=before_scope['output_rules'], before_manifest=baseline_before['manifest'],
            before_manifest_sha256=baseline_before['manifest_sha256']), 'finish prelaunch envelope')
        require(context['qualifications'][historical_task] == binding['qualification_reference'],
                'unbound historical qualification reference')
        bindings[historical_task] = binding
    historical_reference = fields(bindings['19']['qualification_reference'],
                                  'binding_path binding_sha256', 'task19 accepted qualification')
    require(historical_reference['binding_path'] == context['bindings']['10'], 'wrong task19 accepted binding link')
    require(evidence.json(historical_reference['binding_path'], historical_reference['binding_sha256'])
            == bindings['10'], 'mismatched task19 accepted binding link')
    qualification_link(historical_reference, bindings['10']['qualification_reference'], evidence, context['run_id'])
    historical_nine = evidence.json(bindings['10']['receipt_path'], bindings['10']['receipt_sha256'])
    require('nine_exercise' in historical_nine, 'missing historical task10 nine restoration evidence')
    fields(context, 'current_qualification', 'finish current qualification')
    current_scope = derive_scope(repo)
    reference, qualified = qualification(context, '19', context['finish_head'], current_scope,
        evidence, helper, test, reference=context['current_qualification'], check_link=False)
    qualification_link(reference, bindings['19']['qualification_reference'], evidence, context['run_id'])
    baseline = git_manifest(repo, context['finish_head'], current_scope['output_rules'])
    require(filesystem_manifest(repo, current_scope['output_rules']) == baseline,
            'dirty/untracked/ignored generated manifest at finish_head')
    # Finish reproduces a committed revision exactly, so a pre-existing
    # executable-mode drift from finish_head refuses before any generation.
    _require_executable_modes(repo, context['finish_head'], current_scope['output_rules'],
                              'finish admission')
    evidence.unchanged()
    return dict(context=context, evidence=evidence, task=task, baseline=baseline,
                current_scope=current_scope, rules=current_scope['output_rules'],
                qualification=reference, qualified=qualified,
                head=context['finish_head'], baseline_revision=context['finish_head'])


def admit_context(context_path: str, task: str, repo: Path,
                  helper: Path, test: Path) -> dict:
    repo = repo.resolve(strict=True)
    require(_git(repo, 'rev-parse', '--show-toplevel').decode().strip() == str(repo),
            'verification cwd must be repository root')
    path = absolute_path(context_path)
    context = _json_bytes(path.read_bytes(), str(path))
    _schema(context, 'caller context')
    fields(context, 'mode run_id repo evidence_root dispatches prelaunch dispatch_evidence qualifications',
           'caller context')
    require(context['mode'] in ('task', 'finish'), 'unknown context mode')
    require(isinstance(context['run_id'], str) and bool(context['run_id'].strip()), 'invalid run_id')
    require(absolute_path(context['repo']) == repo, 'wrong caller repo')
    root = absolute_path(context['evidence_root'])
    require(root.is_dir(), 'evidence_root is not a directory')
    evidence = Evidence(repo, root)
    evidence.read(str(path))
    tasks = (task,) if context['mode'] == 'task' else ('10', '19')
    for name in ('dispatches', 'prelaunch', 'dispatch_evidence', 'qualifications'):
        fields(context[name], ' '.join(tasks), f'context {name}')
    if context['mode'] == 'finish':
        fields(context, 'bindings finish_head', 'finish context')
        fields(context['bindings'], '10 19', 'finish bindings')
        require(_commit(repo, context['finish_head']) == _git(repo, 'rev-parse', 'HEAD').decode().strip(),
                'stale finish_head')
        return admit_finish(context, task, evidence, helper, test)
    dispatch = fields(context['dispatch_evidence'][task],
                      'native_dispatch_path dispatch_snapshot_path dispatch_snapshot_sha256', 'dispatch evidence')
    native = absolute_path(dispatch['native_dispatch_path'], exists=False)
    require(context['dispatches'][task] == str(native), 'wrong native dispatch path')
    require(dispatch['dispatch_snapshot_path'] != str(native), 'snapshot aliases mutable native dispatch')
    snapshot = evidence.json(dispatch['dispatch_snapshot_path'], dispatch['dispatch_snapshot_sha256'])
    base, wave, subject = _dispatch(snapshot, repo, context['run_id'], task)
    require(subprocess.run([*_GIT, '-C', str(repo), 'merge-base', '--is-ancestor', base, 'HEAD'],
                           capture_output=True).returncode == 0, 'dispatch base is not ancestor of HEAD')
    # Captured independently of the historical dispatch base: the qualified
    # source identity, this HEAD and the helper/test bytes are revalidated at
    # every actual regeneration boundary and before success emission.
    head = _git(repo, 'rev-parse', 'HEAD').decode().strip()
    before_scope, current_scope = derive_scope(repo, base), derive_scope(repo)
    rules = union_rules(before_scope, current_scope)
    baseline = git_manifest(repo, base, rules)
    envelope = evidence.json(context['prelaunch'][task])
    _schema(envelope, 'prelaunch envelope')
    identity = dict(run_id=context['run_id'], repo=str(repo), task_id=task, wave=wave,
                    base_sha=base, **dispatch)
    _same_identity(envelope, identity, 'prelaunch envelope')
    _same_identity(envelope, {'scope_sources': before_scope['scope_sources'],
                              'output_rules': before_scope['output_rules'],
                              'before_manifest': baseline['manifest'],
                              'before_manifest_sha256': baseline['manifest_sha256']}, 'prelaunch envelope')
    require(filesystem_manifest(repo, rules) == baseline, 'stale/dirty generated manifest before task verify')
    # Task/prelaunch admission is a named R5 comparison point: the working
    # generated tree must reproduce the dispatch base's Git executable modes,
    # not only its bytes. Byte equality does not imply executable-mode equality:
    # the byte manifest above only fixes the path set, so a byte-identical tree
    # can still refuse here on Git executable-mode drift.
    _require_executable_modes(repo, base, rules, 'task admission')
    reference, qualified = qualification(context, task, base, current_scope, evidence, helper, test)
    return dict(context=context, evidence=evidence, envelope=envelope,
                envelope_sha256=evidence.pins[context['prelaunch'][task]], identity=identity,
                baseline=baseline, rules=rules, current_scope=current_scope,
                qualification=reference, qualified=qualified, subject=subject,
                head=head, baseline_revision=base)


def create_only(path: Path, data: bytes) -> None:
    """Never replace an existing receipt, log or sidecar; equal bytes read back."""
    if path.exists():
        require(not path.is_symlink() and path.read_bytes() == data,
                f"overwrite attempt/conflicting immutable evidence: {path}")
        return
    checked_path(path.parent, path.name)
    try:
        with path.open('xb') as output:
            output.write(data)
    except FileExistsError:
        require(not path.is_symlink() and path.read_bytes() == data,
                f"overwrite race/conflicting immutable evidence: {path}")


def _encoded(value) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True) + '\n').encode()



# Private recovery is deliberately separate from Evidence and receipt decoding.
# Files are opened relative to validated non-following directory descriptors.
_RECOVERY = 'openxcvr-neutrality-recovery'
_PENDING = 'pending.neutrality-recovery'
_SUFFIX = '.neutrality-recovery'


def _lexical_absolute(value: str) -> Path:
    require(isinstance(value, str) and value.startswith('/') and not value.startswith('//')
            and '\x00' not in value, 'invalid private absolute pathname')
    value.encode('utf-8', errors='strict')
    require(value == '/' or (not value.endswith('/') and
            all(p and p not in ('.', '..') for p in value[1:].split('/'))),
            'noncanonical private absolute pathname')
    return Path(value)


def _overlap(a: Path, b: Path) -> bool:
    return a == b or a.is_relative_to(b) or b.is_relative_to(a)


def _exact(value, names: str, label: str) -> dict:
    fields(value, names, label)
    require(set(value) == set(names.split()), f'{label}: unknown fields')
    return value


def _integer(value, minimum=0, maximum=None) -> int:
    require(type(value) is int and value >= minimum and
            (maximum is None or value <= maximum), 'invalid private integer')
    return value


def _private_json(raw: bytes, names: str, label: str) -> dict:
    value = _json_bytes(raw, label)
    _exact(value, names, label)
    require(_encoded(value) == raw, f'{label}: noncanonical private JSON')
    return value


@contextmanager
def _directory(path: Path, *, private=False):
    """Walk from / with O_NOFOLLOW; never repair existing ancestry."""
    _lexical_absolute(str(path))
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        parts = path.parts[1:]
        # / is established too.
        for index in range(len(parts) + 1):
            info = os.fstat(fd)
            mode = stat.S_IMODE(info.st_mode)
            strict = private and (index == len(parts) or _RECOVERY in parts[:index])
            require(stat.S_ISDIR(info.st_mode), f'not a directory: {path}')
            if strict:
                require(info.st_uid == os.getuid() and mode == 0o700,
                        f'unsafe private directory: {path}')
            else:
                require(info.st_uid in (0, os.getuid()) and
                        (not mode & 0o002 or mode & stat.S_ISVTX),
                        f'unsafe established ancestry: {path}')
            if index == len(parts):
                break
            child = os.open(parts[index], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                            dir_fd=fd)
            os.close(fd)
            fd = child
        yield fd
    finally:
        os.close(fd)


def _stable_stat(info):
    # Reading may update atime; identity, kind, modes, length and write times
    # must remain stable, not access time.
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _private_file(info, path):
    require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
            and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1,
            f'unsafe private file: {path}')


class _PrivateDirectory:
    """Bound directory and file access with identity/readback checks."""

    def __init__(self, path: Path):
        self.path = path

    def mkdir(self, name):
        with _directory(self.path, private=True) as fd:
            try:
                os.mkdir(name, 0o700, dir_fd=fd)
                child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                try:
                    os.fchmod(child, 0o700)
                    os.fsync(child)
                finally:
                    os.close(child)
                os.fsync(fd)
            except FileExistsError:
                pass
        result = _PrivateDirectory(self.path / name)
        with _directory(result.path, private=True):
            pass
        return result

    def names(self):
        with _directory(self.path, private=True) as fd:
            return set(os.listdir(fd))

    def read(self, name):
        with _directory(self.path, private=True) as fd:
            info = os.stat(name, dir_fd=fd, follow_symlinks=False)
            _private_file(info, self.path / name)
            require(info.st_dev == os.fstat(fd).st_dev, f'private mount crossing: {self.path}')
            file = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
            try:
                opened = os.fstat(file)
                _private_file(opened, self.path / name)
                require((info.st_dev, info.st_ino) == (opened.st_dev, opened.st_ino),
                        f'private identity changed: {self.path / name}')
                with os.fdopen(os.dup(file), 'rb') as stream:
                    raw = stream.read()
                require(_stable_stat(os.stat(name, dir_fd=fd, follow_symlinks=False)) == _stable_stat(os.fstat(file)),
                        f'private file changed: {self.path / name}')
                return raw
            finally:
                os.close(file)

    def publish(self, name, raw, *, replace=False):
        with _directory(self.path, private=True) as fd:
            temp = '.staging-' + secrets.token_hex(16)
            file = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                           0o600, dir_fd=fd)
            try:
                os.fchmod(file, 0o600)
                with os.fdopen(os.dup(file), 'wb') as stream:
                    stream.write(raw)
                    stream.flush()
                os.fsync(file)
            finally:
                os.close(file)
            # Interruption leaves staging preserved. Only our successfully
            # published staging is unlinked; no scanning/cleanup of old entries.
            if replace:
                self.read(name)  # replacement is only the validated sequence
                os.replace(temp, name, src_dir_fd=fd, dst_dir_fd=fd)
            else:
                os.link(temp, name, src_dir_fd=fd, dst_dir_fd=fd, follow_symlinks=False)
                os.unlink(temp, dir_fd=fd)
            os.fsync(fd)
        require(self.read(name) == raw, f'private publication readback failed: {self.path / name}')

    def sync(self):
        with _directory(self.path, private=True) as fd:
            os.fsync(fd)


def _rename_private(source: _PrivateDirectory, name: str, target: _PrivateDirectory, new: str):
    """Linux no-replace directory publication; never a copy/delete fallback."""
    with _directory(source.path, private=True) as oldfd, _directory(target.path, private=True) as newfd:
        require(os.fstat(oldfd).st_dev == os.fstat(newfd).st_dev, 'cross-device completion')
        libc = ctypes.CDLL(None, use_errno=True)
        rename = getattr(libc, 'renameat2', None)
        require(rename is not None, 'Linux renameat2 unavailable')
        if rename(oldfd, os.fsencode(name), newfd, os.fsencode(new), 1):
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error), str(target.path / new))
        try:
            os.fsync(oldfd)
            os.fsync(newfd)
        except OSError as error:
            raise NeutralityError(f'uncertain completion at {target.path / new}: {error}') from error


def _local_filesystem(path):
    require(sys.platform == 'linux', 'private lifecycle requires Linux/local filesystem')
    # Linux statfs magic: explicitly supported local filesystems, never NFS/CIFS.
    with _directory(path) as fd:
        buffer = ctypes.create_string_buffer(256)
        libc = ctypes.CDLL(None, use_errno=True)
        if libc.fstatfs(fd, buffer):
            raise OSError(ctypes.get_errno(), 'cannot determine private filesystem')
        magic = ctypes.c_long.from_buffer(buffer).value & 0xffffffff
        require(magic in (0xef53, 0x58465342, 0x9123683e, 0x01021994, 0x794c7630),
                'unsupported private filesystem')
        os.fsync(fd)


def _recovery_identity(repo):
    version = _git(repo, '--version').decode().strip()
    match = re.search(r'(\d+)\.(\d+)', version)
    require(match and tuple(map(int, match.groups())) >= (2, 31), 'private recovery requires Git >=2.31')
    root = Path(_git(repo, 'rev-parse', '--show-toplevel').decode().strip()).resolve(strict=True)
    common = Path(_git(repo, 'rev-parse', '--path-format=absolute', '--git-common-dir')
                  .decode().strip()).resolve(strict=True)
    _local_filesystem(common)
    with _directory(common) as fd:
        c = os.fstat(fd)
        require(c.st_uid == os.getuid(), 'common directory not invoking-uid-owned')
    with _directory(root) as fd:
        r = os.fstat(fd)
    return dict(origin_id=sha256(str(root).encode('utf-8')), source_root=str(root),
                source_dev=r.st_dev, source_ino=r.st_ino, common_dir=str(common),
                common_dev=c.st_dev, common_ino=c.st_ino)


class PrivateRecovery:
    """One origin slot, immutable originals, source-read-only accounting."""

    def __init__(self, repo, origin=None):
        self.repo = Path(repo)
        self.identity = _recovery_identity(self.repo)
        self.origin = hex_digest(origin) if origin is not None else self.identity['origin_id']
        self.common = Path(self.identity['common_dir'])
        with _directory(self.common) as fd:
            try:
                os.mkdir(_RECOVERY, 0o700, dir_fd=fd)
                child = os.open(_RECOVERY, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                try:
                    os.fchmod(child, 0o700)
                    os.fsync(child)
                finally:
                    os.close(child)
                os.fsync(fd)
            except FileExistsError:
                pass
        self.namespace = _PrivateDirectory(self.common / _RECOVERY)
        self.directory = self.namespace.mkdir(self.origin)
        self.fd = None

    def __enter__(self):
        with _directory(self.directory.path, private=True) as fd:
            try:
                lease = os.open('lease', os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                0o600, dir_fd=fd)
                os.fchmod(lease, 0o600)
                os.fsync(lease)
                os.fsync(fd)
            except FileExistsError:
                lease = os.open('lease', os.O_RDWR | os.O_NOFOLLOW, dir_fd=fd)
            try:
                info = os.fstat(lease)
                _private_file(info, self.directory.path / 'lease')
                require(_stable_stat(info) == _stable_stat(os.stat('lease', dir_fd=fd, follow_symlinks=False)), 'lease identity changed')
                try:
                    fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError as error:
                    raise NeutralityError(f'busy private lease: {self.directory.path}') from error
                require(_stable_stat(info) == _stable_stat(os.fstat(lease)) and
                        _stable_stat(info) == _stable_stat(os.stat('lease', dir_fd=fd, follow_symlinks=False)), 'lease identity changed')
                self.fd = lease
            except BaseException:
                os.close(lease)
                raise
        return self

    def __exit__(self, *args):
        # Never LOCK_UN: descendants retain this same open file description.
        os.close(self.fd)
        self.fd = None

    def inventory(self):
        names = self.directory.names()
        require(names <= {'lease', 'sequence.json', _PENDING, 'completed'},
                f'unknown private state: {self.directory.path}')
        return names

    def no_pending(self):
        require(_PENDING not in self.inventory(), f'unresolved pending originals: {self.directory.path / _PENDING}')

    def keep(self):
        result = subprocess.run([*_GIT, 'config', '--file', str(self.common / 'config'),
            '--no-includes', '--get-all', 'openxcvr.neutralityRecoveryKeepCompleted'], capture_output=True)
        if result.returncode == 1 and not result.stdout and not result.stderr:
            return 3
        if result.returncode:
            raise NeutralityError('private retention config lookup failed')
        values = result.stdout.decode('utf-8').splitlines()
        require(len(values) == 1 and re.fullmatch('[0-9]+', values[0]) is not None
                and int(values[0]) > 0, 'invalid private retention config')
        return int(values[0])

    def capabilities(self):
        # Probe the exact no-replace/sync primitive before saving or generation.
        name = '.probe-' + secrets.token_hex(16)
        other = name + '-renamed'
        probe = self.directory.mkdir(name)
        try:
            _rename_private(self.directory, name, self.directory, other)
            try:
                _rename_private(self.directory, other, self.directory, other)
            except OSError as error:
                require(error.errno == errno.EEXIST, 'no-replace rename not supported')
            else:
                raise NeutralityError('no-replace rename not supported')
        finally:
            with _directory(self.directory.path, private=True) as fd:
                for entry in (name, other):
                    try:
                        os.rmdir(entry, dir_fd=fd)
                    except FileNotFoundError:
                        pass
                os.fsync(fd)

    def validate_manifest(self, raw):
        m = _private_json(raw, 'format version acquisition_id origin provenance entries', 'manifest')
        require(m['format'] == _RECOVERY and type(m['version']) is int and m['version'] == 1,
                'unknown private manifest format/version')
        hex_digest(m['acquisition_id'], 32)
        origin = _exact(m['origin'], 'origin_id source_root source_dev source_ino common_dir common_dev common_ino', 'origin')
        require(hex_digest(origin['origin_id']) == self.origin and
                sha256(str(_lexical_absolute(origin['source_root'])).encode('utf-8')) == self.origin,
                'private origin mismatch')
        for key in ('source_dev', 'source_ino', 'common_dev', 'common_ino'):
            _integer(origin[key])
        require(str(_lexical_absolute(origin['common_dir'])) == str(self.common) and
                all(origin[k] == self.identity[k] for k in ('common_dev', 'common_ino')),
                'private common identity mismatch')
        p = _exact(m['provenance'], 'mode run_id task_id context_sha256 evidence_root label head before_state_sha256', 'provenance')
        require(p['mode'] in ('task', 'finish') and p['task_id'] in ('10', '19') and
                all(isinstance(p[k], str) and p[k] for k in ('run_id', 'label')), 'invalid private provenance')
        _lexical_absolute(p['evidence_root'])  # lexical only: old evidence may be gone
        for key in ('context_sha256', 'before_state_sha256'):
            hex_digest(p[key])
        require(isinstance(p['head'], str) and re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', p['head']), 'invalid private head')
        require(isinstance(m['entries'], list) and 0 < len(m['entries']) <= 100000000, 'invalid private entries')
        paths = []
        for ordinal, entry in enumerate(m['entries']):
            _exact(entry, 'ordinal path kind mode payload size sha256', 'entry')
            require(_integer(entry['ordinal']) == ordinal, 'private ordinal mismatch')
            path = relative_path(entry['path'])
            path.encode('utf-8', errors='strict')
            paths.append(path)
            require(entry['kind'] in ('file', 'symlink', 'absent'), 'invalid original kind')
            if entry['kind'] == 'absent':
                require(all(entry[k] is None for k in ('mode', 'payload', 'size', 'sha256')), 'invalid absent original')
            else:
                _integer(entry['mode'], 0, 4095)
                _integer(entry['size'])
                hex_digest(entry['sha256'])
                require(entry['payload'] == f'payloads/{ordinal:08d}.bin', 'private payload name mismatch')
        require(paths == sorted(set(paths)), 'unsorted/duplicate private paths')
        return m

    def marker(self, snapshot, name, m, digest):
        completion = name == 'completion.json'
        value = _private_json(snapshot.read(name), 'format version acquisition_id manifest_sha256' +
                              (' sequence' if completion else ''), name)
        require(value['format'] == 'openxcvr-neutrality-recovery-' + ('completion' if completion else 'ready')
                and type(value['version']) is int and value['version'] == 1 and
                value['acquisition_id'] == m['acquisition_id'] and value['manifest_sha256'] == digest,
                f'invalid private {name}')
        if completion:
            _integer(value['sequence'], 1)
        return value

    def snapshot(self, directory, *, partial=False):
        with _directory(directory.path, private=True) as fd:
            require(os.fstat(fd).st_dev == self.identity['common_dev'], 'private snapshot mount crossing')
        names = directory.names()
        require({'manifest.json', 'payloads'} <= names and
                names <= {'manifest.json', 'payloads', 'ready.json', 'completion.json'},
                f'unknown/incomplete snapshot: {directory.path}')
        raw = directory.read('manifest.json')
        m = self.validate_manifest(raw)
        payloads = _PrivateDirectory(directory.path / 'payloads')
        with _directory(payloads.path, private=True) as fd:
            require(os.fstat(fd).st_dev == self.identity['common_dev'], 'private payload mount crossing')
        expected = {Path(e['payload']).name for e in m['entries'] if e['payload'] is not None}
        found = payloads.names()
        require(found <= expected, f'unknown private payload: {payloads.path}')
        missing = expected - found
        require(partial or not missing, f'missing private payload: {payloads.path}')
        for e in m['entries']:
            if e['payload'] is None or Path(e['payload']).name in missing:
                continue
            data = payloads.read(Path(e['payload']).name)
            require(len(data) == e['size'] and sha256(data) == e['sha256'],
                    f'conflicting private payload ordinal {e["ordinal"]}: {payloads.path}')
            if e['kind'] == 'symlink':
                require('\x00' not in data.decode('utf-8', errors='strict'), 'invalid symlink original')
        if 'ready.json' in names:
            require(not missing, 'ready snapshot has missing payloads')
            self.marker(directory, 'ready.json', m, sha256(raw))
        elif not partial:
            raise NeutralityError(f'snapshot not ready: {directory.path}')
        if 'completion.json' in names:
            require('ready.json' in names, 'completion without ready')
            self.marker(directory, 'completion.json', m, sha256(raw))
        return m, raw, missing

    def originals(self, m):
        require(m['origin'] == self.identity, 'original source identity/association changed')
        root = Path(m['origin']['source_root'])
        with _directory(root) as fd:
            info = os.fstat(fd)
            require((info.st_dev, info.st_ino) == (m['origin']['source_dev'], m['origin']['source_ino']),
                    'original source root replaced')
        values = {}
        for e in m['entries']:
            path = root / e['path']
            parent = checked_path(root, str(PurePosixPath(e['path']).parent)) if '/' in e['path'] else root
            # checked_path rejects symlink ancestors, including dangling ones.
            try:
                with _directory(parent) as fd:
                    try:
                        info = os.stat(path.name, dir_fd=fd, follow_symlinks=False)
                    except FileNotFoundError:
                        info = None
                    if e['kind'] == 'absent':
                        require(info is None, f'original absence unmatched: {e["path"]}')
                        continue
                    require(info is not None and stat.S_IMODE(info.st_mode) == e['mode'],
                            f'original mode/absence unmatched: {e["path"]}')
                    if e['kind'] == 'symlink':
                        require(stat.S_ISLNK(info.st_mode), f'original kind unmatched: {e["path"]}')
                        data = os.readlink(path.name, dir_fd=fd).encode('utf-8', errors='strict')
                    else:
                        require(stat.S_ISREG(info.st_mode), f'original kind unmatched: {e["path"]}')
                        file = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
                        try:
                            require(_stable_stat(os.fstat(file)) == _stable_stat(info), f'original changed: {e["path"]}')
                            with os.fdopen(os.dup(file), 'rb') as stream:
                                data = stream.read()
                            require(_stable_stat(os.fstat(file)) == _stable_stat(info), f'original changed: {e["path"]}')
                        finally:
                            os.close(file)
                    require(_stable_stat(os.stat(path.name, dir_fd=fd, follow_symlinks=False)) == _stable_stat(info),
                            f'original changed: {e["path"]}')
                    require(len(data) == e['size'] and sha256(data) == e['sha256'],
                            f'original content unmatched: {e["path"]}')
                    values[e['payload']] = data
            except FileNotFoundError:
                require(e['kind'] == 'absent', f'original ancestor missing: {e["path"]}')
        return values

    def acquire(self, saved, before, admitted, label, head):
        self.no_pending()
        _require_evidence_disjoint(self.namespace.path, admitted['evidence'].root)
        if not saved:
            return None
        self.keep()
        self.capabilities()
        entries, payloads = [], {}
        for ordinal, (path, original) in enumerate(sorted(saved.items())):
            entry = dict(ordinal=ordinal, path=path, kind='absent', mode=None, payload=None, size=None, sha256=None)
            if original is None:
                require(path not in before, 'captured deletion disagrees with before')
            else:
                kind, data, mode = original
                data = data.encode('utf-8', errors='strict') if kind == 'symlink' else data
                require(kind != 'symlink' or b'\x00' not in data, 'invalid original target')
                require(before[path] == dict(kind=kind, mode=mode, sha256=sha256(data)), 'capture disagrees with before')
                entry.update(kind=kind, mode=mode, payload=f'payloads/{ordinal:08d}.bin', size=len(data), sha256=sha256(data))
                payloads[entry['payload']] = data
            entries.append(entry)
        context = admitted['context']
        manifest = dict(format=_RECOVERY, version=1, acquisition_id=secrets.token_hex(16), origin=self.identity,
            provenance=dict(mode=context['mode'], run_id=context['run_id'], task_id=(admitted['task'] if context['mode'] == 'finish' else admitted['identity']['task_id']),
                context_sha256=canonical_digest(context), evidence_root=str(admitted['evidence'].root),
                label=label, head=head, before_state_sha256=canonical_digest(before)), entries=entries)
        raw = _encoded(manifest)
        self.validate_manifest(raw)  # encoding/format refusal before effects
        snapshot = self.directory.mkdir(_PENDING)
        snapshot.publish('manifest.json', raw)
        files = snapshot.mkdir('payloads')
        for name, data in payloads.items():
            files.publish(Path(name).name, data)
        self.snapshot(snapshot, partial=True)
        self.originals(manifest)
        snapshot.publish('ready.json', _encoded(dict(format='openxcvr-neutrality-recovery-ready', version=1,
            acquisition_id=manifest['acquisition_id'], manifest_sha256=sha256(raw))))
        self.snapshot(snapshot)
        return manifest['acquisition_id']

    def sequence(self):
        if 'sequence.json' not in self.inventory():
            return None
        value = _private_json(self.directory.read('sequence.json'), 'format version last_reserved', 'sequence')
        require(value['format'] == 'openxcvr-neutrality-recovery-sequence' and
                type(value['version']) is int and value['version'] == 1, 'unknown private sequence')
        return _integer(value['last_reserved'])

    def selected(self, acquisition, *, partial=False):
        hex_digest(acquisition, 32)
        names = self.inventory()
        candidates = []
        if _PENDING in names:
            pending = _PrivateDirectory(self.directory.path / _PENDING)
            m, raw, missing = self.snapshot(pending, partial=partial)
            if m['acquisition_id'] == acquisition:
                candidates.append((pending, m, raw, missing, True))
        if 'completed' in names:
            completed = _PrivateDirectory(self.directory.path / 'completed')
            name = acquisition + _SUFFIX
            if name in completed.names():
                directory = _PrivateDirectory(completed.path / name)
                m, raw, missing = self.snapshot(directory)
                require(m['acquisition_id'] == acquisition and 'completion.json' in directory.names(),
                        'invalid completed snapshot identity')
                candidates.append((directory, m, raw, missing, False))
        require(len(candidates) == 1, f'missing or duplicate acquisition {acquisition}: {self.directory.path}')
        return candidates[0]

    def finalize(self, acquisition):
        self.keep()
        snapshot, m, raw, missing, pending = self.selected(acquisition, partial=True)
        if not pending:
            require(m['origin'] == self.identity, 'original source identity changed')
            return snapshot.path
        self.originals(m)  # all-entry equality before any private completion writes
        if missing:
            values = self.originals(m)
            files = _PrivateDirectory(snapshot.path / 'payloads')
            for name in sorted(missing):
                files.publish(name, values['payloads/' + name])
        if 'ready.json' not in snapshot.names():
            self.snapshot(snapshot, partial=True)
            snapshot.publish('ready.json', _encoded(dict(format='openxcvr-neutrality-recovery-ready', version=1,
                acquisition_id=acquisition, manifest_sha256=sha256(raw))))
        self.snapshot(snapshot)
        self.originals(m)
        counter = self.sequence()
        if 'completion.json' in snapshot.names():
            completion = self.marker(snapshot, 'completion.json', m, sha256(raw))
            require(counter is not None and counter == completion['sequence'], 'completion sequence mismatch')
        else:
            if counter is None:
                self.directory.publish('sequence.json', _encoded(dict(format='openxcvr-neutrality-recovery-sequence',
                    version=1, last_reserved=0)))
                counter = 0
            counter += 1
            self.directory.publish('sequence.json', _encoded(dict(format='openxcvr-neutrality-recovery-sequence',
                version=1, last_reserved=counter)), replace=True)
            snapshot.publish('completion.json', _encoded(dict(format='openxcvr-neutrality-recovery-completion',
                version=1, acquisition_id=acquisition, manifest_sha256=sha256(raw), sequence=counter)))
        completed = self.directory.mkdir('completed')
        name = acquisition + _SUFFIX
        require(name not in completed.names(), f'conflicting completed destination: {completed.path / name}')
        _rename_private(self.directory, _PENDING, completed, name)
        result = _PrivateDirectory(completed.path / name)
        self.snapshot(result)
        try:
            self.prune(acquisition)
        except (NeutralityError, OSError, UnicodeError, ValueError):
            print(f'WARNING: prune failed after completion at {result.path}', file=sys.stderr)
        return result.path

    def prune(self, newest):
        completed = _PrivateDirectory(self.directory.path / 'completed')
        keep = self.keep()
        validated = []
        sequences = {}
        for name in sorted(completed.names()):
            path = completed.path / name
            try:
                require(re.fullmatch('[0-9a-f]{32}' + re.escape(_SUFFIX), name), 'unknown completed entry')
                snapshot = _PrivateDirectory(path)
                m, raw, _ = self.snapshot(snapshot)
                require(name == m['acquisition_id'] + _SUFFIX, 'completed identity mismatch')
                marker = self.marker(snapshot, 'completion.json', m, sha256(raw))
                require(marker['sequence'] <= self.sequence(), 'unreserved completed sequence')
                sequences.setdefault(marker['sequence'], []).append(name)
                validated.append((marker['sequence'], name))
            except (NeutralityError, OSError, UnicodeError, ValueError) as error:
                print(f'WARNING: preserved unclassifiable completed state at {path}: {error}', file=sys.stderr)
        # Ambiguous sequences never authorize pruning either participant.
        validated = [(n, name) for n, name in validated if len(sequences[n]) == 1]
        retained = {name for _, name in sorted(validated, reverse=True)[:keep]}
        retained.add(newest + _SUFFIX)
        for _, name in sorted(validated):
            if name in retained:
                continue
            path = completed.path / name
            try:
                snapshot = _PrivateDirectory(path)
                self.snapshot(snapshot)
                files = _PrivateDirectory(path / 'payloads')
                with _directory(files.path, private=True) as fd:
                    for item in files.names():
                        files.read(item)
                        os.unlink(item, dir_fd=fd)
                    os.fsync(fd)
                with _directory(path, private=True) as fd:
                    os.rmdir('payloads', dir_fd=fd)
                    for item in snapshot.names():
                        snapshot.read(item)
                        os.unlink(item, dir_fd=fd)
                    os.fsync(fd)
                with _directory(completed.path, private=True) as fd:
                    os.rmdir(name, dir_fd=fd)
                    os.fsync(fd)
                print(f'private recovery pruned {name[:-len(_SUFFIX)]}', file=sys.stderr)
            except (NeutralityError, OSError, UnicodeError, ValueError) as error:
                print(f'WARNING: prune failed at {path}: {error}', file=sys.stderr)

    def export(self, acquisition, destination):
        snapshot, m, raw, _, _ = self.selected(acquisition)
        destination = _lexical_absolute(destination)
        # MUST precede any filesystem access to destination/old evidence.
        recorded_evidence = _lexical_absolute(m['provenance']['evidence_root'])
        require(not _overlap(destination, recorded_evidence), 'export overlaps recorded evidence root')
        roots = [self.common, _lexical_absolute(m['origin']['source_root'])]
        worktrees = _git(self.repo, 'worktree', 'list', '--porcelain', '-z').decode('utf-8').split('\x00')
        roots.extend(_lexical_absolute(line[9:]) for line in worktrees if line.startswith('worktree '))
        require(not any(_overlap(destination, root) for root in roots), 'export overlaps source/common metadata')
        with _directory(destination.parent, private=True):
            pass
        require(str(destination.parent.resolve(strict=True) / destination.name) == str(destination),
                'export destination not canonical')
        require(not _overlap(destination, recorded_evidence), 'export overlaps recorded evidence root')
        # Detect other Git source trees from existing ancestors without opening
        # the nonexistent destination or the old evidence tree.
        probe = subprocess.run([*_GIT, '-C', str(destination.parent), 'rev-parse', '--show-toplevel'], capture_output=True)
        require(probe.returncode != 0 and b'not a git repository' in probe.stderr.lower(),
                'export inside another Git tree or ambiguous Git discovery')
        _local_filesystem(destination.parent)
        with _directory(destination.parent, private=True) as fd:
            os.mkdir(destination.name, 0o700, dir_fd=fd)  # no reuse, even empty
            child = os.open(destination.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            try:
                os.fchmod(child, 0o700)
                os.fsync(child)
            finally:
                os.close(child)
            os.fsync(fd)
        target = _PrivateDirectory(destination)
        try:
            with _directory(target.path, private=True) as fd:
                os.fchmod(fd, 0o700)
            target.publish('manifest.json', raw)
            files = target.mkdir('payloads')
            original_files = _PrivateDirectory(snapshot.path / 'payloads')
            for name in sorted(original_files.names()):
                files.publish(name, original_files.read(name))
            for name in ('ready.json', 'completion.json'):
                if name in snapshot.names():
                    target.publish(name, snapshot.read(name))
            # Export is on another local device, so validate bytes independently
            # rather than relaxing snapshot's same-common-device admission.
            require(target.read('manifest.json') == raw, 'export manifest changed')
            require(files.names() == original_files.names(), 'export inventory mismatch')
            for e in m['entries']:
                if e['payload'] is not None:
                    data = files.read(Path(e['payload']).name)
                    require(len(data) == e['size'] and sha256(data) == e['sha256'], 'export payload mismatch')
            for name in ('ready.json', 'completion.json'):
                if name in snapshot.names():
                    require(target.read(name) == snapshot.read(name), 'export marker mismatch')
            files.sync()
            target.sync()
            with _directory(destination.parent, private=True) as fd:
                os.fsync(fd)
        except (NeutralityError, OSError, UnicodeError, ValueError) as error:
            raise NeutralityError(f'failed private export at {destination}: {error}') from error
        return destination


def _require_evidence_disjoint(namespace: Path, root: Path) -> None:
    require(not _overlap(namespace, root), 'evidence overlaps private namespace')


def _leased(function):
    @wraps(function)
    def wrapped(admitted, *args, **kwargs):
        # C5: refuse caller overrides before any private state, removal or launch.
        _refuse_launch_overrides()
        evidence = admitted['evidence']
        if '_private_recovery' in admitted:
            _require_evidence_disjoint(admitted['_private_recovery'].namespace.path, evidence.root)
            return function(admitted, *args, **kwargs)
        # C8: the evidence root is disjoint from the private namespace for every
        # run, not only one with originals to save, and before private state,
        # generation, logs, receipts or nine removal.
        common = Path(_recovery_identity(evidence.repo)['common_dir'])
        _require_evidence_disjoint(common / _RECOVERY, evidence.root)
        with PrivateRecovery(evidence.repo) as recovery:
            _require_evidence_disjoint(recovery.namespace.path, evidence.root)
            recovery.no_pending()
            return function(dict(admitted, _private_recovery=recovery), *args, **kwargs)
    return wrapped

def _gitfile_target(path: Path) -> Path | None:
    """The resolved gitdir a `.git` gitfile names, or None if it is not one."""
    try:
        text = path.read_text(encoding='utf-8')
    except (OSError, UnicodeError):
        return None
    if not text.startswith('gitdir:'):
        return None
    target = text[len('gitdir:'):].strip()
    return (path.parent / target).resolve() if target else None


def _metadata_entries(repo: Path) -> set[str]:
    """Relative paths of this worktree's own Git metadata entry, if inside it.

    Derived from Git's reported admin/common locations, never from the basename:
    a `.git` entry elsewhere is an unadmitted topology, not a new exemption. An
    unexpected `.git` symlink to real metadata stays unexpected (no target-based
    exemption is conferred). Nothing here is ever read as source evidence.
    """
    repo = Path(repo)
    git_dir = Path(_git(repo, 'rev-parse', '--absolute-git-dir').decode().strip())
    common = Path(_git(repo, 'rev-parse', '--path-format=absolute', '--git-common-dir')
                  .decode().strip())
    resolved = git_dir.resolve()
    admitted = set()
    for path in (git_dir, common):
        if path.is_relative_to(repo):
            admitted.add(path.relative_to(repo).as_posix())
    root = repo / '.git'
    try:
        mode = root.lstat().st_mode
    except FileNotFoundError:
        mode = None
    if mode is not None and stat.S_ISDIR(mode) and root.resolve() == resolved:
        admitted.add('.git')
    elif mode is not None and stat.S_ISREG(mode) and _gitfile_target(root) == resolved:
        admitted.add('.git')
    return admitted


def _working_files(repo: Path, excluded: list[Path], errors: dict | None = None) -> dict:
    """Whole repo state; recovery can collect undecidable paths without aborting."""
    result = {}
    metadata = _metadata_entries(repo)

    def failed(path: Path, error: Exception) -> None:
        if errors is None:
            raise error
        errors[path.relative_to(repo).as_posix()] = str(error)

    def unexpected_metadata(path: Path) -> None:
        # Never read, hash or descend into an unadmitted metadata entry: the
        # object is ambiguous, so it is left untouched and refused.
        failed(path, NeutralityError('unexpected .git entry (unadmitted metadata): '
                                     + path.relative_to(repo).as_posix()))

    def skip(name: str, path: Path) -> bool:
        """True when this walker entry must not be inspected further."""
        # Git's reported admin/common location is excluded whatever its
        # basename (e.g. `--separate-git-dir=<repo>/admin`); only a `.git`
        # entry that is not that location is unexpected.
        if path.relative_to(repo).as_posix() in metadata:
            return True
        if name != '.git':
            return any(path.is_relative_to(x) for x in excluded)
        unexpected_metadata(path)
        return True

    def walk_error(error: OSError) -> None:
        failed(Path(error.filename), error)

    for directory, dirs, files in os.walk(repo, followlinks=False, onerror=walk_error):
        parent = Path(directory)
        dirs[:] = [name for name in dirs if not skip(name, parent / name)]
        for name in list(dirs):
            path = parent / name
            try:
                if stat.S_ISLNK(path.lstat().st_mode):
                    files.append(name)
                    dirs.remove(name)
            except OSError as error:
                dirs.remove(name)
                failed(path, error)
        for name in files:
            path = parent / name
            if skip(name, path):
                continue
            relative = path.relative_to(repo).as_posix()
            try:
                info = path.lstat()
                mode = info.st_mode
                require(stat.S_ISREG(mode) or stat.S_ISLNK(mode),
                        f"nonregular repository source: {relative}")
                # C10: recovery unlinks and recreates a clobbered path, so a
                # hard-link alias would keep the generator's bytes. The strict
                # (pre-launch) snapshot refuses; the post-generation one, which
                # collects errors, must not turn a path into an unrecovered one.
                require(errors is not None or not stat.S_ISREG(mode) or info.st_nlink == 1,
                        f"multiply linked repository source: {relative}")
                result[relative] = {'kind': 'symlink' if stat.S_ISLNK(mode) else 'file',
                                    'mode': stat.S_IMODE(mode),
                                    'sha256': sha256(os.readlink(path).encode() if stat.S_ISLNK(mode)
                                                     else path.read_bytes())}
            except (NeutralityError, OSError, UnicodeError) as error:
                failed(path, error)
    return result


def _git_executable_modes(repo: Path, revision: str, rules: list[dict]) -> dict:
    """Git's executable bit for every generated file at a revision.

    Independent of the byte-only generated manifest, which keeps its schema: a
    receipt or accepted binding must not be issued when the generated bytes
    match but the executable mode does not.
    """
    modes = {}
    for path, (mode, kind, _) in _git_entries(Path(repo), revision).items():
        if not any(path == rule['root'] or path.startswith(rule['root'] + '/')
                   for rule in rules):
            continue
        require(mode in ('100644', '100755') and kind == 'blob',
                f'symlink/non-file generated git object: {path}')
        modes[path] = mode == '100755'
    return modes


def _worktree_executable_modes(repo: Path, rules: list[dict]) -> dict:
    """The same generated file set's executable bits in the current tree."""
    repo = Path(repo).resolve(strict=True)
    validate_rules(rules)
    modes = {}

    def visit(path: Path) -> None:
        mode = path.lstat().st_mode
        relative = path.relative_to(repo).as_posix()
        relative_path(relative)
        if stat.S_ISREG(mode):
            modes[relative] = _git_executable(mode)
        elif stat.S_ISDIR(mode):
            for child in sorted(path.iterdir()):
                visit(child)
        else:
            raise NeutralityError(f'symlink/non-regular generated output: {relative}')

    for rule in rules:
        path = checked_path(repo, rule['root'])
        if path.exists():
            if not path.is_dir():
                raise NeutralityError(f'generated root is not a directory: {rule["root"]}')
            visit(path)
    return modes


def _require_executable_modes(repo: Path, revision: str, rules: list[dict], label: str) -> None:
    """Refuse generated bytes that match while the executable mode does not.

    Applied wherever a generated tree is required to reproduce a committed
    revision exactly: task/prelaunch admission and finish admission. The
    historical accepted-code binding refuses the same drift through
    _git_executable_modes. Regeneration-induced drift is caught separately
    (before/after each generation), because the comparison base there is the
    state the regeneration started from rather than a back-to-back mode map.
    """
    expected = _git_executable_modes(repo, revision, rules)
    actual = _worktree_executable_modes(repo, rules)
    require(set(expected) == set(actual),
            f'{label}: generated file set differs from {revision}')
    for path in sorted(expected):
        require(expected[path] == actual[path],
                f'{label}: generated executable mode differs: {path}')


def _mode_drift(before: dict, after: dict, rules: list[dict]) -> list[str]:
    """Executable-mode changes a single regeneration introduced itself.

    Restricted to paths present in both states, because the nine-file exercise
    legitimately removes generated files and lets the generator recreate them.
    _exercise_nine is the caller that owns those paths: it compares the
    recreated modes against the pre-removal modes it captured before deletion.
    """
    drift = []
    for path in sorted(set(before) & set(after)):
        if not _in_domain(path, rules):
            continue
        old, new = before[path], after[path]
        if old['kind'] != 'file' or new['kind'] != 'file':
            continue
        if _git_executable(old['mode']) != _git_executable(new['mode']):
            drift.append(f'regeneration changed the generated executable mode: {path}')
    return drift


def _require_admitted_identity(admitted: dict, label: str) -> None:
    """Revalidate captured HEAD, helper/test bytes and current rule sources.

    Admission captures HEAD itself, independently of the historical dispatch
    base, so a boundary move is observable. Historical base semantics stay in
    `identity`/`envelope`; this only refuses to emit success on drift.
    """
    repo, reference = admitted['evidence'].repo, admitted['qualification']
    require(_git(repo, 'rev-parse', 'HEAD').decode().strip() == admitted['head'],
            f'{label}: verified HEAD moved')
    require(derive_scope(repo) == admitted['current_scope'],
            f'{label}: current rule sources changed')
    for role in ('helper', 'test'):
        require(sha256(absolute_path(reference[role + '_path']).read_bytes())
                == hex_digest(reference[role + '_sha256']), f'{label}: current {role} changed')


def _git_file_state(repo: Path, mode: str, oid: str) -> dict:
    require(mode in ('100644', '100755', '120000'), 'unsupported git file mode')
    return dict(kind='symlink' if mode == '120000' else 'file',
                mode={'100644': 0o644, '100755': 0o755, '120000': 0o777}[mode],
                sha256=sha256(_git(repo, 'cat-file', 'blob', oid)))


def _git_executable(mode: int) -> bool:
    """Git's regular-file executable test: the owner-execute bit decides.

    `ce_permissions` maps an owner-execute bit to 100755 and everything else to
    100644, so group/other execute bits are not executable modes to Git. The
    group-only and other-only forms are still permission differences the current
    byte comparison permits; they simply do not make a file executable.
    """
    return bool(mode & 0o100)


def _code_delta(repo: Path, base: str, working: dict) -> list[dict]:
    before = {}
    for path, (mode, kind, oid) in _git_entries(repo, base).items():
        require(kind == 'blob' and mode in ('100644', '100755', '120000'),
                f"unsupported code delta object: {path}")
        before[path] = _git_file_state(repo, mode, oid)
    result = []
    for path in sorted(set(before) | set(working)):
        old, new = before.get(path), working.get(path)
        if new is not None:
            # Git records executable/non-executable, not group-write bits, and
            # the owner-execute bit is decisive.
            new = dict(new, mode=0o777 if new['kind'] == 'symlink' else
                       (0o755 if _git_executable(new['mode']) else 0o644))
        if old != new:
            result.append(dict(path=path,
                before_sha256=old['sha256'] if old else None,
                after_sha256=new['sha256'] if new else None,
                before_kind=old['kind'] if old else None, after_kind=new['kind'] if new else None,
                before_mode=old['mode'] if old else None, after_mode=new['mode'] if new else None))
    return result


# C5: every child launched for generation or Cargo probing gets an environment
# built from this allowlist, never the caller's environment minus exclusions;
# a caller hook (BASH_ENV/ENV, an exported `BASH_FUNC_cargo%%`, a rustc
# wrapper) could otherwise reproduce the expected bytes while acting outside
# the snapshot. Add a name only with a measured need recorded beside it.
_LAUNCH_ENVIRONMENT = (
    'PATH',  # resolves cargo/rustc/rustfmt/linker/bash (the tests' fake cargo too)
    'HOME', 'TMPDIR',  # Cargo/rustup home default; scratch stays in the owned prefix
    'LANG', 'LC_ALL', 'LC_CTYPE',  # locale-dependent tool output stays identical to the caller's
    # Toolchain selection: a relocated or pinned toolchain resolves exactly as
    # it does for the caller; dropping these could select another default.
    'CARGO_HOME', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN',
)
# Diagnostic only (the allowlist is the protection): documented compiler,
# wrapper, flag and linker overrides refuse instead of being silently dropped.
_LAUNCH_OVERRIDES = frozenset(('RUSTC', 'RUSTDOC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                               'RUSTFLAGS', 'RUSTDOCFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                               'CARGO_ENCODED_RUSTDOCFLAGS'))
_LAUNCH_TARGET_OVERRIDE = re.compile(r'CARGO_TARGET_.+_(?:LINKER|RUNNER|RUSTFLAGS)')


def _launch_overrides(environ) -> list[str]:
    """The names in `environ` the launch guard refuses, sorted."""
    return sorted(k for k in environ
                  if k in _LAUNCH_OVERRIDES or _LAUNCH_TARGET_OVERRIDE.fullmatch(k)
                  or (k.startswith('CARGO_BUILD_') and k != 'CARGO_BUILD_TARGET_DIR'))


def _refuse_launch_overrides() -> None:
    """Refuse a caller compiler, wrapper, flag or linker override.

    Called explicitly at the start of every leased entry point, before any
    private acquisition, nine-file removal or generation, and again by the
    environment builder at each launch.
    """
    overrides = _launch_overrides(os.environ)
    require(not overrides, 'unsupported compiler/linker override in caller environment: '
            + ', '.join(overrides) + '; unset it for the helper run')


def _cargo_environment() -> dict:
    """The one launch environment for generation and Cargo probing.

    Not hermetic. Trusted, operator-controlled inputs this does not close:
    executables resolved through PATH (cargo, rustc, rustfmt, the linker,
    bash); Cargo configuration reachable from HOME/CARGO_HOME (for example
    `build.rustc-wrapper`) and the rustup toolchain they select; the
    repository's `.cargo/config.toml` and `rust-toolchain(.toml)`, which are
    build inputs owed to qualified source identity (follow-up 4235457051);
    and the environment of the helper's own `git` children (for example
    GIT_DIR, GIT_WORK_TREE, GIT_CONFIG_*), which report the Git metadata
    locations the snapshot excludes. The asymmetry is deliberate: an exported
    RUSTC_WRAPPER refuses, while `build.rustc-wrapper` under a passed
    CARGO_HOME is trusted, because preserving toolchain selection outweighs
    configuration routes this does not claim to close.
    Everything unlisted is dropped, including BASH_ENV/ENV, every BASH_FUNC_*
    entry, CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR.
    """
    _refuse_launch_overrides()
    return {k: os.environ[k] for k in _LAUNCH_ENVIRONMENT if k in os.environ}


def _build_exclusions(repo: Path) -> list[Path]:
    # Design L258–261: untracked files inside a source-declared Cargo target
    # directory are build output by declaration, not inferred from their bytes.
    # No cargo workspace means no build-output exemption.
    if not (repo / 'Cargo.toml').exists():
        return []
    result = subprocess.run(['cargo', 'metadata', '--offline', '--no-deps', '--format-version', '1'],
                            cwd=repo, capture_output=True, env=_cargo_environment())
    require(result.returncode == 0, 'cannot derive Cargo build-output exclusion')
    metadata = _json_bytes(result.stdout, 'cargo metadata')
    target = absolute_path(metadata.get('target_directory'), exists=False)
    require(target != repo and not repo.is_relative_to(target), 'ambiguous Cargo target exclusion')
    tracked = _git_entries(repo, 'HEAD')
    declared = []
    for name in ('.cargo/config', '.cargo/config.toml'):
        path = repo / name
        if not path.exists() and not path.is_symlink():
            continue
        require(name in tracked and tracked[name][0] in ('100644', '100755'),
                'Cargo exclusion config is not tracked')
        data = checked_path(repo, name).read_bytes()
        require(data == _git(repo, 'cat-file', 'blob', tracked[name][2]),
                'Cargo exclusion config differs from tracked declaration')
        try:
            config = tomllib.loads(data.decode('utf-8'))
        except (tomllib.TOMLDecodeError, UnicodeError) as error:
            raise NeutralityError('invalid Cargo exclusion config') from error
        build = config.get('build', {})
        require(isinstance(build, dict), 'invalid Cargo build exclusion config')
        for key in ('target-dir', 'build-dir'):
            value = build.get(key)
            if value is not None:
                require(isinstance(value, str) and bool(value) and not re.search(r'[${}]', value),
                        'unresolved Cargo exclusion declaration')
                declared.append((repo / value).resolve())
    require(len(set(declared)) <= 1, 'ambiguous Cargo exclusion declarations')
    expected = declared[0] if declared else repo / 'target'
    require(target == expected, 'Cargo target exclusion is not source-declared')
    if target.is_relative_to(repo):
        checked_path(repo, target.relative_to(repo).as_posix())
    require(not any((repo / p).is_relative_to(target) for p in tracked),
            'Cargo target exclusion overlaps tracked source')
    # C6b: the snapshot skips Git's reported metadata roots by path, so a target
    # equal to, inside or containing one would let generation write into Git's
    # administration directory unseen. Compared as resolved absolute paths.
    overlap = sorted(entry for entry in _metadata_entries(repo)
                     if target.is_relative_to(root := (repo / entry).resolve())
                     or root.is_relative_to(target))
    require(not overlap, 'Cargo target exclusion overlaps Git metadata: ' + ', '.join(overlap))
    return [target]


# Design L167–175: these identities are fixed, not discovered from current output.
NINE_PATHS = (
    'crates/openxcvr-sff8472/src/generated/page_a0h_base_memory.rs',
    'bindings/c/generated/openxcvr_generated.h',
    'bindings/go/generated/page_a0h_base_memory.go',
    'bindings/php/src/Generated/page_a0h_base_memory.php',
    'bindings/python/openxcvr/_generated/page_a0h_base_memory.py',
    'bindings/typescript/generated/page_a0h_base_memory.ts',
    'bindings/java/src/main/java/com/openxcvr/generated/PageA0hBaseMemory.java',
    'bindings/csharp/generated/page_a0h_base_memory.cs',
    'bindings/openapi/generated/pages/page_a0h_base_memory.schema.json',
)


def _in_domain(path: str, rules: list[dict]) -> bool:
    return any(path.startswith(r['root'] + '/') for r in rules)


def _nine_selection(admitted: dict) -> tuple[list[str], dict | None]:
    """Only explicit, pinned parent amendments can rename finish identities.

    Optional context nine_amendment:{path,sha256} pins schema-v1 evidence with
    run_id/repo/task_id/finish_head/reason and renames:[{from,to}]. No inference.
    """
    paths, amendment = list(NINE_PATHS), None
    context, evidence = admitted['context'], admitted['evidence']
    if 'nine_amendment' in context:
        require(context['mode'] == 'finish', 'nine amendment is finish-only')
        ref = fields(context['nine_amendment'], 'path sha256', 'nine amendment reference')
        amendment = evidence.json(ref['path'], ref['sha256'])
        _schema(amendment, 'nine amendment')
        _same_identity(amendment, dict(run_id=context['run_id'], repo=str(evidence.repo),
            task_id='10', finish_head=context['finish_head']), 'nine amendment')
        fields(amendment, 'reason renames', 'nine amendment')
        require(isinstance(amendment['reason'], str) and bool(amendment['reason'].strip()),
                'nine amendment requires recorded reason')
        renames = amendment['renames']
        require(isinstance(renames, list) and bool(renames), 'empty nine amendment')
        seen = set()
        for rename in renames:
            fields(rename, 'from to', 'nine rename')
            old, new = relative_path(rename['from']), relative_path(rename['to'])
            require(old in NINE_PATHS and old not in seen and old != new, 'invalid nine rename')
            seen.add(old)
            paths[NINE_PATHS.index(old)] = new
        require(len(set(paths)) == 9, 'duplicate amended nine paths')
    require(all(_in_domain(p, admitted['rules']) for p in paths),
            'nine path outside declared generated domains')
    for p in paths:
        require(checked_path(evidence.repo, p).is_file(),
                f'missing nine file (rename requires explicit recorded amendment): {p}')
    return paths, amendment


class _DeferredInterrupt(NeutralityError):
    """A checkpoint failure; only outermost guard exit may replay SIGINT."""


class _DeferringSIGINT:
    """Depth-counted, process-local deferral; never raise from the handler."""

    def __init__(self):
        self.depth = 0
        self.pending = False
        self.prior = None

    def _record(self, signum, frame):
        self.pending = True

    def __enter__(self):
        if self.depth == 0:
            prior = signal.getsignal(signal.SIGINT)
            require(callable(prior) or prior in (signal.SIG_DFL, signal.SIG_IGN),
                    'cannot defer SIGINT with an unknown prior disposition')
            self.prior = prior
            self.pending = False
            try:
                signal.signal(signal.SIGINT, self._record)
            except ValueError as error:
                raise NeutralityError('cannot defer SIGINT outside the main thread') from error
        self.depth += 1
        return self

    def checkpoint(self):
        if self.pending:
            raise _DeferredInterrupt('SIGINT pending; regeneration interrupted')

    def __exit__(self, kind, error, traceback):
        self.depth -= 1
        if self.depth:
            return False
        signal.signal(signal.SIGINT, self.prior)
        if self.pending:
            if error is None or isinstance(error, _DeferredInterrupt):
                # Restore first. SIG_IGN leaves the checkpoint failure intact;
                # callable/default/SIG_DFL dispositions retain their semantics.
                signal.raise_signal(signal.SIGINT)
                # C12: SIG_IGN, or a prior handler that returns, does not end
                # the run. With no primary failure (an ordinary verification
                # has no checkpoint after generation) the interrupt still
                # fails it, so no receipt can follow.
                if error is None:
                    raise _DeferredInterrupt('SIGINT pending; regeneration interrupted')
            else:
                error.add_note('SIGINT pending; not replayed beside the primary failure')
        return False


_sigint_guard = _DeferringSIGINT()


def _interrupt_guarded(function):
    @wraps(function)
    def wrapped(*args, **kwargs):
        with _sigint_guard:
            return function(*args, **kwargs)
    return wrapped


@_leased
@_interrupt_guarded
def _regenerate(admitted: dict, directory: Path, label: str, exclusions: list[Path]) -> tuple[dict, dict]:
    """Record each immediate repository state, even when generation fails."""
    evidence, repo = admitted['evidence'], admitted['evidence'].repo
    # The qualified identity must still hold immediately before this actual
    # regeneration, not only when the request was admitted.
    _require_admitted_identity(admitted, f'{label} before generation')
    for excluded in exclusions:
        require(not any((repo / r['root']).is_relative_to(excluded) or
                        excluded.is_relative_to(repo / r['root']) for r in admitted['rules']),
                'Cargo target exclusion overlaps generated domain')
    before = _working_files(repo, exclusions)
    # One immutable base for dirty classification and clean-file recovery.
    revision = _git(repo, 'rev-parse', 'HEAD').decode().strip()
    dirty = _code_delta(repo, revision, before)
    # Fallible global setup belongs before the generator can mutate anything.
    tree = _git_entries(repo, revision)
    # Preserve pre-existing source dirt if a broken generator writes into it.
    # Generated dirt has already been refused; intentional nine removals are not source dirt.
    saved = {}
    for change in dirty:
        p = change['path']
        if _in_domain(p, admitted['rules']):
            continue
        path = repo / p
        if p not in before:
            saved[p] = None  # A dirty deletion must remain deleted.
        elif before[p]['kind'] == 'symlink':
            saved[p] = ('symlink', os.readlink(path), before[p]['mode'])
        else:
            saved[p] = ('file', path.read_bytes(), before[p]['mode'])
    evidence.unchanged()
    recovery = admitted['_private_recovery']
    # L': no private snapshot is published once an interrupt is already pending,
    # so a checkpoint failure here cannot leave one unresolved for later runs.
    _sigint_guard.checkpoint()
    acquisition = recovery.acquire(saved, before, admitted, label, revision)
    evidence.unchanged()
    result = subprocess.run(['bash', 'tools/regen-all.sh'], cwd=repo, capture_output=True,
                            env=_cargo_environment(), pass_fds=(recovery.fd,),
                            start_new_session=True, stdin=subprocess.DEVNULL)
    snapshot_errors = {}
    after = _working_files(repo, exclusions, snapshot_errors)
    # A regeneration must not change the generated executable mode: bytes equal
    # is not reproducibility. Collected as a diagnostic so recovery still runs.
    mode_drift = _mode_drift(before, after, admitted['rules'])
    stdout, stderr = directory / (label + '.stdout'), directory / (label + '.stderr')
    command = dict(argv=['bash', 'tools/regen-all.sh'], cwd=str(repo), exit=result.returncode,
        stdout_path=str(stdout), stdout_sha256=sha256(result.stdout),
        stderr_path=str(stderr), stderr_sha256=sha256(result.stderr))
    changes = sorted({p for p in set(before) | set(after) if before.get(p) != after.get(p)}
                     | set(snapshot_errors))
    outside = [p for p in changes if not _in_domain(p, admitted['rules'])]
    state = dict(before=before, after=after, before_sha256=canonical_digest(before),
                 after_sha256=canonical_digest(after), out_of_domain=outside)
    evidence_error = None
    try:
        create_only(stdout, result.stdout)
        create_only(stderr, result.stderr)
        create_only(directory / (label + '.state.json'), _encoded(dict(command=command, **state)))
    except BaseException as error:
        # Evidence conflicts also fail closed, but must not prevent recovery.
        # Captured, not propagated: recovery runs either way and the original
        # failure is re-raised below with the recovery failures beside it.
        evidence_error = error
    # Clean tracked bytes come from the revision HEAD named before generation
    # started, not from whatever HEAD names now; use before-state modes, which
    # Git cannot represent exactly. Never replace the saved pre-existing dirt.
    # One path at a time: an unrecoverable path must not abort the safe restores
    # behind it, and each failure is reported beside the domain violation rather
    # than in place of it.
    failures = []
    for p in outside:
        try:
            require(not any(unknown == '.' or p == unknown or p.startswith(unknown + '/')
                            for unknown in snapshot_errors),
                    f'undecidable post-generation path; left untouched: {p}')
            if p not in saved:
                if p not in before:
                    saved[p] = None
                else:
                    require(p in tree, f'missing clean-file recovery object: {p}')
                    data = _git(repo, 'cat-file', 'blob', tree[p][2])
                    require(sha256(data) == before[p]['sha256'], f'stale recovery object: {p}')
                    kind = before[p]['kind']
                    saved[p] = (kind, os.fsdecode(data) if kind == 'symlink' else data,
                                before[p]['mode'])
            if p in saved:
                if '/' in p:
                    checked_path(repo, str(PurePosixPath(p).parent))
                path = repo / p
                try:
                    mode = path.lstat().st_mode
                except FileNotFoundError:
                    mode = None
                require(mode is None or stat.S_ISREG(mode) or stat.S_ISLNK(mode),
                        f'unsafe dirty-file recovery: {p}')
                path.unlink(missing_ok=True)
                if saved[p] is not None:
                    kind, data, mode = saved[p]
                    path.parent.mkdir(parents=True, exist_ok=True)
                    if kind == 'symlink':
                        path.symlink_to(data)
                    else:
                        with path.open('xb') as output:
                            output.write(data)
                        path.chmod(mode)
        except (NeutralityError, OSError, UnicodeError) as error:
            failures.append(f'{p}: {error}')
    diagnostics = []
    if acquisition is not None:
        try:
            recovery.finalize(acquisition)
        except (NeutralityError, OSError, UnicodeError, ValueError) as error:
            diagnostics.append(f'private originals remain unresolved ({acquisition}) at {recovery.directory.path}: {error}')
    if outside:
        diagnostics.append('regeneration wrote outside generated domains: ' + ', '.join(outside))
    if snapshot_errors:
        diagnostics.append('post-generation snapshot failed: ' + '; '.join(
            f'{p}: {error}' for p, error in sorted(snapshot_errors.items())))
    if evidence_error is not None:
        diagnostics.append(f'evidence capture failed: {evidence_error}')
    if failures:
        diagnostics.append('recovery failed for: ' + '; '.join(failures))
    if mode_drift:
        diagnostics.extend(mode_drift)
    # Post-execution qualification follows the recovery above, never preempts it.
    try:
        _require_admitted_identity(admitted, f'{label} after generation')
    except (NeutralityError, OSError, UnicodeError) as error:
        diagnostics.append(str(error))
    if diagnostics:
        raise NeutralityError('; '.join(diagnostics)) from evidence_error
    require(result.returncode == 0, f'regeneration failed (exit {result.returncode}); logs: {directory}')
    evidence.unchanged()
    return command, state


@_leased
@_interrupt_guarded
def _exercise_nine(admitted: dict, directory: Path, exclusions: list[Path],
                   paths: list[str], amendment: dict | None) -> dict:
    repo, rules = admitted['evidence'].repo, admitted['rules']
    baseline = filesystem_manifest(repo, rules)
    require(baseline == admitted['baseline'], 'dirty generated manifest before nine removal')
    saved, modes = {}, {}
    for p in paths:
        path = checked_path(repo, p)
        require(path.is_file(), f'missing nine file (rename requires explicit recorded amendment): {p}')
        saved[p] = path.read_bytes()
        modes[p] = stat.S_IMODE(path.stat().st_mode)
    proof = dict(paths=paths, amendment=amendment, before=baseline,
                 pre_removal_hashes={p: sha256(data) for p, data in saved.items()})
    # Scratch is outside the repository; never an arbitrary source exemption.
    scratch = tempfile.TemporaryDirectory(prefix='neutrality-nine-')
    copies, removed = {}, []
    try:
        for index, (p, data) in enumerate(saved.items()):
            copy = Path(scratch.name) / str(index)
            copy.write_bytes(data)
            copies[p] = copy
        _sigint_guard.checkpoint()  # 0: nothing has been removed.
        for p, data in saved.items():
            path = checked_path(repo, p)
            require(path.read_bytes() == data, f'nine file changed before removal: {p}')
            removed.append(p)
            path.unlink()
        proof['deleted'] = filesystem_manifest(repo, rules)
        command, state = _regenerate(admitted, directory, 'nine-regen', exclusions)
        _sigint_guard.checkpoint()  # 1: the pipe EOF barrier has completed.
        proof.update(command=command, working_state=state,
                     after=filesystem_manifest(repo, rules))
        proof['restored_hashes'] = {}
        for p in paths:
            path = checked_path(repo, p)
            require(path.is_file(), f'nine file not restored: {p}')
            proof['restored_hashes'][p] = sha256(path.read_bytes())
        require(proof['restored_hashes'] == proof['pre_removal_hashes'],
                'nine files not restored byte-identically')
        # R5: the nine regeneration is an actual generation boundary, and the
        # general regeneration snapshot cannot cover the paths it deleted, so
        # the recreated files are compared here against the modes captured
        # before removal. Drift enters the recovery below instead of
        # preventing it, and no receipt may be issued for a mode-drifted tree.
        for p in paths:
            require(_git_executable(stat.S_IMODE(checked_path(repo, p).stat().st_mode))
                    == _git_executable(modes[p]),
                    f'nine regeneration changed the generated executable mode: {p}')
        require(proof['after'] == baseline, 'nine exercise changed complete generated manifest')
        _sigint_guard.checkpoint()  # 2: success validation is complete.
        scratch.cleanup()
        _sigint_guard.checkpoint()  # 3: disposal is still restoration-capable.
    except BaseException as error:
        notes = []
        # No fallible evidence write may precede independently safe restoration.
        for p in sorted(removed):
            try:
                path = checked_path(repo, p)
                require(not path.exists() or path.is_file(), f'unsafe nine recovery: {p}')
                if not path.exists() or path.read_bytes() != saved[p]:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(copies[p].read_bytes())
                path.chmod(modes[p])
            except BaseException as recovery_error:
                notes.append(f'nine recovery failed for {p}: {recovery_error}')
        try:
            proof['error'] = str(error)
            create_only(directory / 'nine-failure.json', _encoded(proof))
        except BaseException as record_error:
            notes.append(f'failure evidence not recorded: {record_error}')
        try:
            scratch.cleanup()
        except BaseException as disposal_error:
            notes.append(f'nine scratch disposal failed: {disposal_error}')
        if isinstance(error, (NeutralityError, OSError, UnicodeError)):
            # The aggregate is only for a primary failure accompanied by secondary
            # notes; a lone one stays its own object, so an OSError, UnicodeError
            # or checkpoint _DeferredInterrupt is not rewritten as a plain error.
            if notes:
                raise NeutralityError('; '.join([str(error), *notes])) from error
            raise
        for note in notes:
            error.add_note(note)
        raise
    return proof


def _validate_nine(proof: dict, baseline: dict, rules: list[dict], evidence: Evidence) -> None:
    fields(proof, 'paths amendment before deleted after pre_removal_hashes restored_hashes command working_state',
           'nine exercise evidence')
    paths = proof['paths']
    require(isinstance(paths, list) and len(paths) == len(set(paths)) == 9,
            'incomplete nine exercise paths')
    require(proof['amendment'] is None and paths == list(NINE_PATHS), 'unamended historical nine paths required')
    hashes = {v['path']: v['sha256'] for v in baseline['manifest'] if v['path'] in paths}
    require(len(hashes) == 9 and proof['pre_removal_hashes'] == hashes
            and proof['restored_hashes'] == hashes, 'wrong nine restoration hashes')
    require(proof['before'] == baseline and proof['after'] == baseline, 'wrong complete nine manifests')
    deleted = _manifest([v for v in baseline['manifest'] if v['path'] not in paths])
    require(proof['deleted'] == deleted, 'wrong nine deleted manifest')
    _command_records([proof['command']], evidence)
    require(proof['command']['argv'] == ['bash', 'tools/regen-all.sh'], 'wrong nine regeneration command')
    _validate_states([proof['working_state']], rules)


def _validate_states(states: list[dict], rules: list[dict]) -> None:
    for state in states:
        fields(state, 'before after before_sha256 after_sha256 out_of_domain', 'working-state evidence')
        for role in ('before', 'after'):
            require(canonical_digest(state[role]) == state[role + '_sha256'], 'wrong working-state digest')
        changes = set(state['before']) | set(state['after'])
        require(state['out_of_domain'] == [] and all(_in_domain(p, rules) for p in changes
            if state['before'].get(p) != state['after'].get(p)), 'out-of-domain working-state evidence')


def _validate_task_receipt(receipt: dict, admitted: dict, evidence: Evidence) -> None:
    _schema(receipt, 'task receipt')
    _same_identity(receipt, admitted['identity'], 'task receipt')
    expected = dict(envelope_sha256=admitted['envelope_sha256'],
                    scope_sources=admitted['current_scope']['scope_sources'],
                    output_rules=admitted['rules'], qualification_reference=admitted['qualification'],
                    helper_sha256=admitted['qualification']['helper_sha256'],
                    test_sha256=admitted['qualification']['test_sha256'],
                    before_manifest=admitted['baseline']['manifest'],
                    before_manifest_sha256=admitted['baseline']['manifest_sha256'],
                    after_manifest=admitted['baseline']['manifest'],
                    after_manifest_sha256=admitted['baseline']['manifest_sha256'])
    _same_identity(receipt, expected, 'task receipt')
    _command_records(receipt.get('commands'), evidence)
    require(len(receipt['commands']) == 2 and all(
        c['argv'] == ['bash', 'tools/regen-all.sh'] for c in receipt['commands']),
        'missing repeat regeneration evidence')
    fields(receipt, 'code_delta code_delta_sha256 working_state', 'task receipt')
    require(canonical_digest(receipt['code_delta']) == receipt['code_delta_sha256'], 'wrong code delta digest')
    require(len(receipt['working_state']) == 2, 'missing regeneration preservation evidence')
    _validate_states(receipt['working_state'], admitted['rules'])
    if admitted.get('exercise_nine'):
        require('nine_exercise' in receipt, 'missing nine restoration evidence')
    if 'nine_exercise' in receipt:
        _validate_nine(receipt['nine_exercise'], admitted['baseline'], admitted['rules'], evidence)


@_leased
def verify_task(admitted: dict, exercise_nine: bool = False) -> dict:
    evidence = admitted['evidence']
    repo = evidence.repo
    rules = admitted['rules']
    exclusions = _build_exclusions(repo)
    working = _working_files(repo, exclusions)
    delta = _code_delta(repo, admitted['identity']['base_sha'], working)
    admitted = dict(admitted, exercise_nine=exercise_nine)
    selection = _nine_selection(admitted) if exercise_nine else None
    request = dict(identity=admitted['identity'], envelope_sha256=admitted['envelope_sha256'],
                   qualification=admitted['qualification'], code_delta=delta, exercise_nine=exercise_nine)
    directory = evidence.root / ('task-' + admitted['identity']['task_id']) / canonical_digest(request)
    require(not directory.is_relative_to(repo), 'receipt destination inside code worktree')
    checked_path(evidence.root, directory.relative_to(evidence.root).as_posix())
    receipt_path = directory / 'receipt.json'
    if directory.exists():
        require(receipt_path.is_file(), 'incomplete existing receipt/overwrite attempt')
        raw = evidence.read(str(receipt_path))
        sidecar = evidence.read(str(directory / 'receipt.sha256')).decode().strip()
        require(hex_digest(sidecar) == sha256(raw), 'task receipt sidecar mismatch')
        receipt = _json_bytes(raw, str(receipt_path))
        _validate_task_receipt(receipt, admitted, evidence)
        require(receipt['code_delta'] == delta, 'stale task receipt code delta')
        evidence.unchanged()
        return receipt
    directory.mkdir(parents=True, exist_ok=False)
    commands, states = [], []
    for index in range(2):
        require(filesystem_manifest(repo, rules) == admitted['baseline'], 'generated manifest changed before regen')
        command, state = _regenerate(admitted, directory, f'regen-{index}', exclusions)
        commands.append(command)
        states.append(state)
        require(filesystem_manifest(repo, rules) == admitted['baseline'], 'generated output is not base-neutral/reproducible')
    receipt = dict(schema_version=1, **admitted['identity'],
                   envelope_sha256=admitted['envelope_sha256'],
                   scope_sources=admitted['current_scope']['scope_sources'], output_rules=rules,
                   helper_sha256=admitted['qualification']['helper_sha256'],
                   test_sha256=admitted['qualification']['test_sha256'],
                   qualification_reference=admitted['qualification'],
                   qualification_commands=admitted['qualified']['commands'],
                   before_manifest=admitted['baseline']['manifest'],
                   before_manifest_sha256=admitted['baseline']['manifest_sha256'],
                   after_manifest=admitted['baseline']['manifest'],
                   after_manifest_sha256=admitted['baseline']['manifest_sha256'],
                   commands=commands, code_delta=delta, code_delta_sha256=canonical_digest(delta),
                   working_state=states)
    if selection is not None:
        receipt['nine_exercise'] = _exercise_nine(admitted, directory, exclusions, *selection)
    _validate_task_receipt(receipt, admitted, evidence)
    evidence.unchanged()
    raw = _encoded(receipt)
    create_only(receipt_path, raw)
    create_only(directory / 'receipt.sha256', (sha256(raw) + '\n').encode())
    return receipt


@_leased
def verify_finish(admitted: dict, exercise_nine: bool = False) -> dict:
    """Always regenerate current schemas; historical receipts are read-only pins."""
    evidence, context = admitted['evidence'], admitted['context']
    repo, baseline, rules = evidence.repo, admitted['baseline'], admitted['rules']
    exclusions = _build_exclusions(repo)
    selection = _nine_selection(admitted) if exercise_nine else None
    request = dict(finish_head=context['finish_head'], task_id=(admitted['task'] if context['mode'] == 'finish' else admitted['identity']['task_id']),
                   context_sha256=canonical_digest(context), qualification=admitted['qualification'],
                   exercise_nine=exercise_nine)
    directory = evidence.root / 'finish' / canonical_digest(request)
    checked_path(evidence.root, directory.relative_to(evidence.root).as_posix())
    directory.mkdir(parents=True, exist_ok=True)
    commands, manifests, states = [], [baseline], []
    for index in range(2):
        require(_git(repo, 'rev-parse', 'HEAD').decode().strip() == context['finish_head'], 'finish HEAD moved')
        require(derive_scope(repo) == admitted['current_scope'], 'current rule sources changed')
        require(filesystem_manifest(repo, rules) == baseline, 'current generated manifest changed before regen')
        command, state = _regenerate(admitted, directory, f'regen-{index}', exclusions)
        commands.append(command)
        states.append(state)
        actual = filesystem_manifest(repo, rules)
        require(actual == baseline, 'current generated tree is not reproducible')
        require(derive_scope(repo) == admitted['current_scope'], 'current rule sources changed during regen')
        for role in ('helper', 'test'):
            ref = admitted['qualification']
            require(sha256(absolute_path(ref[role + '_path']).read_bytes()) == ref[role + '_sha256'],
                    f'current {role} changed during regen')
        manifests.append(actual)
        evidence.unchanged()
    require(_git(repo, 'rev-parse', 'HEAD').decode().strip() == context['finish_head'], 'finish HEAD moved')
    receipt = dict(schema_version=1, mode='finish', run_id=context['run_id'], repo=str(repo),
        **request, scope_sources=admitted['current_scope']['scope_sources'], output_rules=rules,
        before_manifest=baseline['manifest'], before_manifest_sha256=baseline['manifest_sha256'],
        after_manifest=baseline['manifest'], after_manifest_sha256=baseline['manifest_sha256'],
        manifests=manifests, commands=commands, working_state=states)
    if selection is not None:
        receipt['nine_exercise'] = _exercise_nine(admitted, directory, exclusions, *selection)
    _validate_states(states, rules)
    _command_records(commands, evidence)
    evidence.unchanged()
    raw = _encoded(receipt)
    create_only(directory / 'receipt.json', raw)
    create_only(directory / 'receipt.sha256', (sha256(raw) + '\n').encode())
    return receipt


def _coerced_id(value):
    """Pinned producer `coerceId` semantics as a comparable identity key.

    The pinned producer's `coerceId = (v) => (/^-?[0-9]+$/.test(String(v)) ? Number(v) : v)`
    turns an integral spelling into a number and keeps every other value as it
    is. Booleans, null and containers are never stringified into an identity;
    they produce a key that cannot equal a real task id instead of collapsing.
    """
    if type(value) is int:
        return ('number', value)
    if type(value) is float:
        return ('number', int(value) if value.is_integer() else value)
    if isinstance(value, str):
        if re.fullmatch(r'-?\d+', value) is not None:
            return ('number', int(value))
        return ('string', value)
    return ('invalid', None)


def _task_id_list(value, label: str) -> list:
    require(isinstance(value, list), f'{label} must be a list')
    keys = [_coerced_id(item) for item in value]
    require(all(key[0] != 'invalid' for key in keys), f'{label}: unusable task identity')
    require(len(set(keys)) == len(keys), f'{label}: duplicate/conflicting task identity')
    return keys


def _completion_stamp(collection: dict) -> bool:
    """A recorded collection carries its completion time; a stamp is not success."""
    value = collection.get('completed_at')
    return isinstance(value, str) and bool(value.strip())


def _retained_result_success(collection: dict, retained: dict, requested) -> None:
    """Validate the retained dispatched result's requested-task digest.

    A record carrying `result` keeps the result it was dispatched with, whether
    that is a marker-cleared re-drive or an ordinary recorded success. The note
    is never authority for that success; the structured digest is, and its
    identity must agree with the record it is stored in. Only the source-read
    optional inner identity is compared when present: an absent inner run, wave
    or base is not a defect, and no mandatory inner field is required here.
    """
    for name in ('run_id', 'wave'):
        if name in retained:
            require(retained[name] == collection.get(name),
                    f'retained result {name} disagrees with the collection record')
    review = retained.get('review_context')
    if isinstance(review, dict) and 'base_sha' in review:
        require(review['base_sha'] == collection['review_context']['base_sha'],
                'retained result base disagrees with the collection record')
    tasks = retained.get('tasks')
    require(isinstance(tasks, list) and bool(tasks), 'retained result has no task digests')
    digests = []
    for item in tasks:
        require(isinstance(item, dict), 'malformed retained result task entry')
        digest = item.get('digest')
        digest = item if digest is None else digest
        require(isinstance(digest, dict), 'malformed retained result task digest')
        digests.append(digest)
    matches = [digest for digest in digests
               if _coerced_id(digest.get('task_id')) == requested]
    require(len(matches) == 1,
            'ambiguous/absent requested-task digest in the retained result')
    require(matches[0].get('status') == 'done',
            'retained result does not record the requested task as done')


def _recorded_task_success(collection: dict, task: str) -> None:
    """Require actual recorded completion of the requested task.

    Identity equality alone never proves collection. Two source shapes are
    recognized: the ordinary `record_outcome` membership the pinned producer
    writes when it closes `pending`, and the marker-cleared re-drive that keeps
    its dispatched result. Pending, record-error-only, a free-form note, and a
    failed/absent/ambiguous requested-task digest all refuse. A retained
    historical `record_error` beside a successful current outcome is legitimate.
    An ordinary success checks every failed entry's identity and, when the record
    also carries a `result`, that retained result's coherence.
    """
    requested = _coerced_id(task)
    require(collection.get('status') == 'recorded',
            'native collection is not recorded: no successful requested-task record')
    require(_completion_stamp(collection), 'recorded collection has no completion timestamp')
    outcome = collection.get('record_outcome')
    if isinstance(outcome, dict) and 'recorded' in outcome:
        recorded = _task_id_list(outcome['recorded'], 'record_outcome.recorded')
        require(requested in recorded, 'requested task is not in record_outcome.recorded')
        failed = outcome.get('failed', [])
        require(isinstance(failed, list), 'record_outcome.failed must be a list')
        for entry in failed:
            require(isinstance(entry, dict), 'malformed record_outcome.failed entry')
            identity = _coerced_id(entry.get('id'))
            # The pinned producer formats every failed entry with the key `id`.
            # An unusable value is refused rather than silently evaluating as
            # another task, and no alias is invented for this shape.
            require(identity[0] != 'invalid', 'unusable record_outcome.failed task identity')
            require(identity != requested,
                    'record_outcome reports the requested task as failed')
        if 'result' in collection:
            # The pinned serializer writes the supplied record verbatim and
            # inserts no default `result`, so absence stays an ordinary success
            # while a present value is a retained result that must be coherent.
            # A present null/nonobject is malformed, not another spelling of
            # absence.
            retained = collection['result']
            require(isinstance(retained, dict), 'malformed retained result beside recorded outcome')
            _retained_result_success(collection, retained, requested)
        return
    retained = collection.get('result')
    if isinstance(outcome, dict) and set(outcome) == {'note'} and isinstance(retained, dict):
        require(isinstance(outcome['note'], str) and bool(outcome['note'].strip()),
                'recovery outcome note is missing')
        _retained_result_success(collection, retained, requested)
        return
    raise NeutralityError('native collection holds no successful requested-task evidence')


def validate_binding(binding: dict, evidence: Evidence, run_id: str, task: str) -> None:
    """Consume a parent postcollection binding, never synthesize acceptance."""
    _schema(binding, 'parent binding')
    fields(binding, 'wave native_dispatch_path dispatch_snapshot_path dispatch_snapshot_sha256 base_sha '
                    'receipt_path receipt_sha256 qualification_reference accepted_code_commit accepted_code_tree '
                    'native_collection_evidence_path native_collection_evidence_sha256 code_delta_sha256 '
                    'generated_after_manifest_sha256', 'parent binding')
    _same_identity(binding, dict(run_id=run_id, repo=str(evidence.repo), task_id=task), 'parent binding')
    absolute_path(binding['native_dispatch_path'], exists=False)
    require(binding['native_dispatch_path'] != binding['dispatch_snapshot_path'],
            'binding snapshot aliases mutable native dispatch')
    snapshot = evidence.json(binding['dispatch_snapshot_path'], binding['dispatch_snapshot_sha256'])
    base, wave, subject = _dispatch(snapshot, evidence.repo, run_id, task)
    require(binding['base_sha'] == base and binding['wave'] == wave, 'binding dispatch identity mismatch')
    accepted = _commit(evidence.repo, binding['accepted_code_commit'])
    require(_git(evidence.repo, 'rev-parse', accepted + '^{tree}').decode().strip()
            == hex_digest(binding['accepted_code_tree'], 40), 'wrong accepted code tree')
    for descendant in (accepted, _git(evidence.repo, 'rev-parse', 'HEAD').decode().strip()):
        ancestor = base if descendant == accepted else accepted
        require(subprocess.run([*_GIT, '-C', str(evidence.repo), 'merge-base', '--is-ancestor', ancestor, descendant],
                               capture_output=True).returncode == 0, 'binding ancestry mismatch')
    collection = evidence.json(binding['native_collection_evidence_path'],
                               binding['native_collection_evidence_sha256'])
    _dispatch(collection, evidence.repo, run_id, task)
    require(collection['wave'] == wave and collection['review_context']['base_sha'] == base,
            'wrong native collection identity')
    for name in ('key', 'attempt', 'wave_token'):
        if name in snapshot:
            require(collection.get(name) == snapshot[name], f'wrong collection {name}')
    _recorded_task_success(collection, task)
    receipt = evidence.json(binding['receipt_path'], binding['receipt_sha256'])
    _schema(receipt, 'bound task receipt')
    _same_identity(receipt, {k: binding[k] for k in ('run_id', 'repo', 'task_id', 'wave', 'base_sha',
                   'native_dispatch_path', 'dispatch_snapshot_path', 'dispatch_snapshot_sha256',
                   'qualification_reference', 'code_delta_sha256')}, 'bound task receipt')
    require(canonical_digest(receipt['code_delta']) == binding['code_delta_sha256'], 'bound code delta mismatch')
    before_scope, scope = derive_scope(evidence.repo, base), derive_scope(evidence.repo, accepted)
    rules = union_rules(before_scope, scope)
    require(receipt['output_rules'] == rules, 'unbound historical output rules')
    baseline = git_manifest(evidence.repo, base, rules)
    reference = binding['qualification_reference']
    qualified_reference, qualified = qualification(dict(run_id=run_id), '19', base, scope,
        evidence, evidence.repo / 'tools/check-generated-neutrality.py',
        evidence.repo / 'tools/test_check_generated_neutrality.py', revision=accepted,
        reference=reference, check_link=False)
    if task == '10':
        require(reference['source_identity']['base_sha'] == base, 'stale bound qualification base')
    _validate_task_receipt(receipt, dict(identity={k: binding[k] for k in ('run_id', 'repo', 'task_id',
        'wave', 'base_sha', 'native_dispatch_path', 'dispatch_snapshot_path', 'dispatch_snapshot_sha256')},
        envelope_sha256=receipt['envelope_sha256'], current_scope=scope, rules=rules,
        baseline=baseline, qualification=qualified_reference), evidence)
    require(receipt.get('qualification_commands') == qualified['commands'], 'unbound qualification commands')
    for revision, prefix in ((base, 'before'), (accepted, 'after')):
        actual = git_manifest(evidence.repo, revision, rules)
        require(receipt[prefix + '_manifest'] == actual['manifest'] and
                receipt[prefix + '_manifest_sha256'] == actual['manifest_sha256'], 'bound generated manifest mismatch')
    require(binding['generated_after_manifest_sha256'] == receipt['after_manifest_sha256'],
            'binding generated digest mismatch')
    # Independent of the byte-only manifest: accepted generated executable-mode
    # drift is refused outright. An accurate task code delta records the change
    # (the ownership rule below still requires that accounting) but recording is
    # not neutrality, so it cannot license the accepted binding.
    base_modes = _git_executable_modes(evidence.repo, base, rules)
    accepted_modes = _git_executable_modes(evidence.repo, accepted, rules)
    drifted = sorted(path for path in set(base_modes) & set(accepted_modes)
                     if base_modes[path] != accepted_modes[path])
    require(not drifted,
            'accepted generated executable-mode drift: ' + ', '.join(drifted))
    entries = _git_entries(evidence.repo, accepted)
    actual_delta = _code_delta(evidence.repo, base, {
        p: _git_file_state(evidence.repo, entry[0], entry[2]) for p, entry in entries.items()})
    require(isinstance(receipt['code_delta'], list), 'invalid bound code delta')
    paths = [relative_path(c['path']) for c in receipt['code_delta']]
    require(paths == sorted(set(paths)), 'duplicate/unsorted bound code delta')
    require(all(c in actual_delta for c in receipt['code_delta']), 'bound before/after code delta mismatch')
    declarations = subject.get('files')
    if declarations is None:
        payload = snapshot.get('payload', [])
        matches = [v for v in payload if isinstance(v, dict) and str(v.get('id')) == task]
        require(len(matches) == 1, 'missing task intended scope')
        declarations = matches[0].get('files')
    require(isinstance(declarations, list) and bool(declarations), 'missing task intended scope')
    declared = [relative_path(p.rstrip('/')) for p in declarations]
    mandatory = {s['path'] for s in scope['scope_sources']} | {
        'tools/check-generated-neutrality.py', 'tools/test_check_generated_neutrality.py'}
    for change in actual_delta:
        p = change['path']
        owned = p in mandatory or any(p == d or p.startswith(d + '/') for d in declared)
        owned = owned or p.startswith('schemas/') or any(p.startswith(r['root'] + '/') for r in rules)
        require(not owned or change in receipt['code_delta'], f'unaccounted task change: {p}')
    for change in receipt['code_delta']:
        path = relative_path(change['path'])
        current = entries.get(path)
        actual = sha256(_git(evidence.repo, 'cat-file', 'blob', current[2])) if current else None
        require(change['after_sha256'] == actual, f'accepted code differs from tested bytes: {path}')
    reference = binding['qualification_reference']
    qualified = evidence.json(reference['qualification_receipt_path'], reference['qualification_receipt_sha256'])
    _schema(qualified, 'bound qualification')
    _command_records(qualified.get('commands'), evidence)
    for name in ('helper', 'test'):
        path = absolute_path(reference[name + '_path']).relative_to(evidence.repo).as_posix()
        require(path in entries and sha256(_git(evidence.repo, 'cat-file', 'blob', entries[path][2]))
                == reference[name + '_sha256'], f'unbound accepted {name}')
    require(scope['scope_sources'] == reference['source_identity']['scope_sources'], 'unbound accepted rule sources')

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest='command', required=True)
    verify = subcommands.add_parser('verify', help='verify explicit parent-qualified task neutrality')
    verify.add_argument('--task', choices=('10', '19'), required=True)
    verify.add_argument('--context', required=True)
    verify.add_argument('--exercise-nine', action='store_true')
    export = subcommands.add_parser('recovery-export', help='export validated originals without source writes')
    export.add_argument('--repo', required=True)
    export.add_argument('--origin', required=True)
    export.add_argument('--snapshot', required=True)
    export.add_argument('--destination', required=True)
    finalize = subcommands.add_parser('recovery-finalize', help='account for originals without restoring source')
    finalize.add_argument('--repo', required=True)
    finalize.add_argument('--snapshot', required=True)
    args = parser.parse_args(argv)
    try:
        if args.command != 'verify':
            with PrivateRecovery(Path(args.repo), getattr(args, 'origin', None)) as recovery:
                location = (recovery.export(args.snapshot, args.destination) if args.command == 'recovery-export'
                            else recovery.finalize(args.snapshot))
                print(f'OK: private {args.command}; origin {recovery.origin}; acquisition {args.snapshot}; {location}')
            return 0
        require(not args.exercise_nine or args.task == '10', 'nine exercise is task10-only')
        helper = Path(__file__).resolve()
        admitted = admit_context(args.context, args.task, Path.cwd(), helper,
                                 helper.with_name('test_check_generated_neutrality.py'))
        receipt = (verify_finish(admitted, args.exercise_nine) if admitted['context']['mode'] == 'finish'
                   else verify_task(admitted, args.exercise_nine))
        print('OK: ' + admitted['context']['mode'] + ' neutrality verified; generated manifest '
              + receipt['after_manifest_sha256'])
        return 0
    except (NeutralityError, OSError, UnicodeError, ValueError, KeyError, TypeError) as error:
        print(f'ERROR: --context or recovery input: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
