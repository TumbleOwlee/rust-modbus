# Server — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Entries under "Known limitations" are working as implemented; they are recorded
here so they are not mistaken for oversights and silently "fixed".

---

## 1. Request handling

*(TBD — unsupported function code; address or quantity out of range; a write to a
read-only table; a request addressed to a different unit/slave id; a malformed
request. Each row names the Modbus exception code returned, or states that the
frame is dropped without a response.)*

| Condition | Behavior |
|---|---|
| | |

## 2. Connections

*(TBD — bind failure; a client that connects and sends nothing; a client that
disconnects mid-frame; behavior at the connection limit; in-flight requests at
shutdown.)*

## 3. Known limitations

*(TBD — e.g. the store is in-memory only and does not survive a restart.)*
