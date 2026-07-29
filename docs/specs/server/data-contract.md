# Server — Data Contract

This area owns **no** data model. There are no coil, discrete-input, holding-
register or input-register tables in this crate, no addressing rules mapping a
request's address range onto them, and no initial state (SV-R-005).

---

## 1. Why

A Modbus register table is not protocol; it is the consumer's application state
wearing a Modbus address. Shipping one would mean choosing its bounds, its
sparseness, its locking granularity, its persistence and its behavior at the edge
of the address space — every one of those a decision that belongs to the device
being modelled, not to the library carrying its bytes. A consumer whose truth
lives in a PLC mirror, a database, or a set of sensor readings would then hold two
data models and a synchronization problem between them.

So the crate stops at the request. The service trait
([`api-contract.md`](./api-contract.md) §2) receives a decoded `RequestPdu` and
returns a `ResponsePdu` or an `ExceptionCode`, and what happens in between is the
implementor's.

## 2. What the consumer owns

- **The tables**, in whatever shape fits — an array, a map, a view onto hardware.
- **The addressing rules**: which addresses exist, which are read-only, and which
  exception code an absent or refused address draws. The frame area validates
  only what the wire format fixes (FR-R-021, CL-R-063); *meaning* is not validated
  by this crate at all.
- **The synchronization.** SV-R-003 gives the service to every connection by
  shared reference, so a service holding mutable state holds it behind its own
  lock. That is the intended shape, not a workaround: it is what lets requests on
  distinct connections be served at once (SV-R-030) while the state behind them
  stays consistent.

## 3. What the server guarantees about data

Only this: a request is delivered decoded and whole, with the unit identifier it
was addressed to (SV-R-010), and the answer is sent unaltered (SV-R-013). The
server imposes no ordering between requests on *different* connections — a
service that needs one must impose it itself.
