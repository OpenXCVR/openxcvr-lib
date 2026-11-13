# Contributing

The contributor guide is [docs/contributing.md](docs/contributing.md): how the
schema-first build fits together, which files are generated, and the checks a
change has to pass before review.

This repository's public history is a series of exports from the development
repository. A pull request opened here is reviewed, then a maintainer ports
the change into the development repository. The next export carries it.

Two rules catch most first contributions:

- Generated files are outputs. Change the schema or the generator, regenerate,
  and commit both together.
- A golden file records what the decoder should produce. When a test disagrees
  with a golden, find out which one is wrong before touching either.

Report security problems privately, as described in [SECURITY.md](SECURITY.md).
