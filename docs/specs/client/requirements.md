# Client — Requirements

Normative behavior of the async Modbus client (initiator): the public request
API, how requests are issued and responses matched, timeout semantics, retry and
reconnect policy, and how protocol exceptions are surfaced to the caller.

Wire encoding is **not** specified here — it belongs to
[`../frame/`](../frame/). Socket and serial-port behavior belongs to
[`../transport/`](../transport/). This area owns only what is specific to acting
as the initiator.

IDs are stable and append-only (`CL-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (public client
types, methods, configuration fields), [`edge-cases.md`](./edge-cases.md)
(boundary and error behavior, stated limitations).

---

## Requirements

*(None yet. Requirements are added through gate 1 of the workflow in
[`AGENTS.md`](../../../AGENTS.md): the "shall" text is approved before the code
is written, and each requirement is pinned by a test citing its ID.)*
