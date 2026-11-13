# Security policy

## What counts

openxcvr-lib decodes memory read from pluggable modules, and that memory is
untrusted input: a faulty or hostile module controls every byte. A report is in
scope when crafted module memory, a crafted capture file, or a crafted schema
input makes the library or a language binding:

- crash, panic, hang, or read or write outside the buffers it was given;
- report a value the schema says cannot be derived from the supplied bytes;
- plan a write, password, or firmware operation the caller did not request.

The library never performs module I/O on its own initiative. Problems in the
kernel driver, the daemon, or the command-line tools belong to their own
repositories.

## Supported versions

The project is pre-1.0. Fixes land on `main` and ship in the next release;
earlier releases are not patched.

## Reporting

Use GitHub's private vulnerability reporting for this repository (the
**Report a vulnerability** button under the **Security** tab). Do not open a
public issue for a suspected vulnerability.

Include the affected revision, the specification family and revision involved
(CMIS, SFF-8636, or SFF-8472), and the smallest input that reproduces the
problem. A module memory dump or a fuzz corpus entry is ideal; remove serial
numbers first if the dump comes from a production module.
