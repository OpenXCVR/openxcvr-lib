#!/usr/bin/env python3
"""Refuse commits that violate a supplied publication policy.

--content checks the head tree and net added text of base..head, for work in
progress. Full mode checks every commit's tree, added text, message, identity
and single-parent history. Neither mode writes to the repository.
Policy is TOML data; it is never imported or executed.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys
import tomllib


def git(repo: Path, *args: str) -> str:
    return subprocess.run(['git', '-C', str(repo), *args], capture_output=True,
                          text=True, check=True).stdout


def load_rules(path: Path) -> dict:
    try:
        with path.open('rb') as stream:
            rules = tomllib.load(stream)
        if set(rules) != {'public_root', 'public_name', 'public_email',
                          'hidden_exact', 'hidden_prefixes', 'deny', 'allow'}:
            raise ValueError('unexpected or missing policy keys')
        for key in ('public_root', 'public_name', 'public_email'):
            if not isinstance(rules[key], str) or not rules[key]:
                raise ValueError(f'invalid {key}')
        for key in ('hidden_exact', 'hidden_prefixes'):
            if not isinstance(rules[key], list) or not all(
                    isinstance(p, str) and p for p in rules[key]):
                raise ValueError(f'invalid {key}')
        if not isinstance(rules['deny'], list) or not rules['deny']:
            raise ValueError('empty deny policy')
        for rule in rules['deny']:
            if set(rule) != {'name', 'pattern', 'ignore_case'} or not isinstance(rule['ignore_case'], bool):
                raise ValueError('invalid deny rule')
            rule['regex'] = re.compile(rule['pattern'], re.I if rule['ignore_case'] else 0)
        if not isinstance(rules['allow'], list):
            raise ValueError('invalid allow policy')
        for rule in rules['allow']:
            if set(rule) != {'scope', 'pattern', 'reason'} or rule['scope'] not in ('all', 'filename', 'content'):
                raise ValueError('invalid allow rule')
            rule['regex'] = re.compile(rule['pattern'])
        return rules
    except (OSError, ValueError, TypeError, KeyError, re.error) as error:
        raise ValueError(f'--rules: {error}') from error


def deny_hits(rules: dict, text: str, scope: str) -> list[str]:
    return sorted({rule['name'] for rule in rules['deny']
                   for hit in rule['regex'].finditer(text)
                   if not any(allow['scope'] in ('all', scope) and
                              any(m.start() <= hit.start() and m.end() >= hit.end()
                                  for m in allow['regex'].finditer(text))
                              for allow in rules['allow'])})


def check(repo: Path, base: str, head: str, rules: dict, content: bool) -> list[str]:
    bad = []
    roots = git(repo, 'rev-list', '--max-parents=0', head).split()
    if roots != [rules['public_root']]:
        bad.append(f'roots {roots} != public root')

    def tree(revision):
        paths = git(repo, 'ls-tree', '-rz', '--name-only', revision).rstrip('\0').split('\0')
        hidden = [p for p in paths if p in rules['hidden_exact'] or any(
            p == prefix.rstrip('/') or p.startswith(prefix) for prefix in rules['hidden_prefixes'])]
        if hidden:
            bad.append(f'{revision} hidden paths {hidden}')
        for path in paths:
            hits = deny_hits(rules, path, 'filename')
            if hits:
                bad.append(f'{revision} filename {path!r} deny {hits}')

    def added(lines, label):
        text = '\n'.join(line[1:] for line in lines
                         if line.startswith('+') and not line.startswith('+++'))
        hits = deny_hits(rules, text, 'content')
        if hits:
            bad.append(f'{label} added-content deny {hits}')

    if content:
        tree(head)
        added(git(repo, 'diff', '-U0', '--no-color', base, head).splitlines(), 'net')
        print(f'checked net change {base}..{head} (content only)')
    else:
        commits = git(repo, 'rev-list', '--reverse', f'{base}..{head}').split()
        for commit in commits:
            author, committer, parents = git(repo, 'log', '-1', '--format=%an <%ae>|%cn <%ce>|%P', commit).strip().split('|')
            identity = f"{rules['public_name']} <{rules['public_email']}>"
            if author != identity or committer != author:
                bad.append(f'{commit[:10]} identity {[author, committer]}')
            if len(parents.split()) != 1:
                bad.append(f'{commit[:10]} is a merge')
            tree(commit)
            hits = deny_hits(rules, git(repo, 'log', '-1', '--format=%B', commit), 'content')
            if hits:
                bad.append(f'{commit[:10]} message deny {hits}')
            added(git(repo, 'show', '--format=', '-U0', '--no-color', commit).splitlines(), commit[:10])
        print(f'checked {len(commits)} commits in {base}..{head}')
    return bad


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rules', type=Path, help='Publication policy TOML')
    parser.add_argument('--content', action='store_true')
    parser.add_argument('repo', type=Path)
    parser.add_argument('base')
    parser.add_argument('head')
    args = parser.parse_args(argv)
    try:
        common = Path(git(args.repo, 'rev-parse', '--path-format=absolute', '--git-common-dir').strip())
        path = args.rules or common.parent.parent / 'openxcvr/lib/publish-rules.toml'
        rules = load_rules(path)
        bad = check(args.repo, args.base, args.head, rules, args.content)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'REFUSE {error}', file=sys.stderr)
        return 1
    for refusal in bad:
        print('REFUSE', refusal)
    return int(bool(bad))


if __name__ == '__main__':
    raise SystemExit(main())
