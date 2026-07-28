# Transport — Requirements

Normative behavior of the transport area: TCP sockets and RTU serial ports, the
rules that determine where one ADU ends and the next begins, connection setup and
teardown, and read/write timeout semantics at the byte level.

This area is **role-agnostic**: it is used identically by the client and the
server. What a byte sequence *means* belongs to [`../frame/`](../frame/); what a
role does about it belongs to [`../client/`](../client/) or
[`../server/`](../server/).

IDs are stable and append-only (`TR-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (transport types and
configuration fields), [`edge-cases.md`](./edge-cases.md) (boundary and error
behavior, stated limitations).

---

## Requirements

*(None yet. Requirements are added through gate 1 of the workflow in
[`AGENTS.md`](../../../AGENTS.md): the "shall" text is approved before the code
is written, and each requirement is pinned by a test citing its ID.)*
