# SFF-8472 Schemas

SFF-8472 SFP/SFP+ memory pages and TOM messages for runtime revisions 9.3,
9.5, 10.2, 10.4, 11.0, 11.3, 11.4, 12.3, 12.4, and 12.5, modelled as revision
windows in this directory. There is no 12.0 compliance code. The index
document revision is 12.5a (`docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf`). The
runtime revision is 12.5; `12.5a` is the cited document's label and is not a
runtime revision. Cached documents are Rev 12.3, Rev 12.4, and Rev 12.5a.
Revisions 9.3 through 11.4 have no cached document; their windows are derived
from the later documents' change history. These schemas import shared
SFF-8024 enum registries.

Entry point: [`_index.yaml`](_index.yaml).
