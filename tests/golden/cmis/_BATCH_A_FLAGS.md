# Batch A CMIS golden authoring flags

- Schema/prompt mismatch: the task text points to Page 00h byte 86 for MediaType and Page 01h bytes 95-126 for AppDescriptors, but `schemas/cmis-5.3/lower-memory.yaml` defines MediaType at Lower byte 85 and eight primary Application Descriptors at Lower bytes 86-117. The generated goldens decode applications from Lower memory per the schema and `tests/golden/README.md` Step 1.
- VendorPN bytes are all NUL in these four fboss CMIS 4.0 fixtures, so `module.module_type` is written as null instead of inventing a vendor-PN-derived label.
- The requested sibling-parity path diff between OSFP 2x400G DR4 and QSFP-DD 400G DR4 is non-empty because the OSFP fixture advertises a third Lower-memory ApplicationDescriptor before the 0xFF terminator; the QSFP-DD fixture terminates after two descriptors.
