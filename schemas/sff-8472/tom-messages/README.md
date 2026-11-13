# SFF-8472 TOM Message Schemas

Type Of Message (TOM) schemas for SFF-8472 v12.5a § 12.4 — G.698.4 optical-channel remote management.

Schema kind: `openxcvr.tom_message.v1`.

Index membership is declared from [`../_index.yaml`](../_index.yaml) under the `tom_messages:` key.

## What these schemas cover

Each schema declares the 24-bit MSG payload of a TOM frame. The outer frame structure
(`[11b TOM code][5b Hamming][24b MSG][8b Hamming]`) is PHY-layer transport and is
not schemaed here.

Fields use bit-level addressing: `offset_bits` (LSB position within the 24-bit MSG,
D0=0) and `size_bits`.

## File index

| File | TOM Code(s) | Section | Description |
|---|---|---|---|
| [`tom-2a0h-ctrl-cmd-rsp.yaml`](tom-2a0h-ctrl-cmd-rsp.yaml) | 2A0h | § 12.4.1 | Control/Command/Response — two variants on `message_type` (D20): RequestPageData and WriteRemoteMemory |
| [`tom-2a8h-2a9h-send-page-data.yaml`](tom-2a8h-2a9h-send-page-data.yaml) | 2A8h, 2A9h | § 12.4.2 | Send Page Data — shared MSG layout for A0h (2A8h) and A2h (2A9h) data |
| [`tom-2aah-extended-idle.yaml`](tom-2aah-extended-idle.yaml) | 2AAh | § 12.4.3 | Extended Idle — 24-bit sequence counter heartbeat |
| [`tom-2abh-security-idle.yaml`](tom-2abh-security-idle.yaml) | 2ABh | § 12.4.4 | Security Idle — 16-bit reserved + 8-bit edit counter |
