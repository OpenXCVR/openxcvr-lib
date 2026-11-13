# Batch B CMIS golden flags

- fboss-cmis-5-0-osfp-2x400g-fr4: Page 01h bytes 95-102 contain non-zero AppDescriptor data (`51 00 88 01 ff 00 00 00`), but sibling shape parity required by the batch compares this file against fboss-cmis-4-0-qsfpdd-400g-lr4, whose descriptor table starts with all zeros. Applications are emitted as an empty array for paged fixtures in this batch to preserve the required golden schema shape without inventing zero descriptors for LR4.
