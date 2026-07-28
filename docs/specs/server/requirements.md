# Server — Requirements

Normative behavior of the async Modbus server (responder): binding and accepting,
per-connection handling, dispatching decoded requests, the in-memory data store
(coils, discrete inputs, holding registers, input registers), exception
generation, and shutdown.

Wire encoding is **not** specified here — it belongs to
[`../frame/`](../frame/). Socket and serial-port behavior belongs to
[`../transport/`](../transport/). This area owns only what is specific to acting
as the responder.

IDs are stable and append-only (`SV-R-nnn`). See [`../README.md`](../README.md).

Companion documents: [`api-contract.md`](./api-contract.md) (public server
types, handler surface, configuration fields),
[`data-contract.md`](./data-contract.md) (the data store's register model and
addressing), [`edge-cases.md`](./edge-cases.md) (boundary and error behavior,
stated limitations).

---

## Requirements

*(None yet. Requirements are added through gate 1 of the workflow in
[`AGENTS.md`](../../../AGENTS.md): the "shall" text is approved before the code
is written, and each requirement is pinned by a test citing its ID.)*
