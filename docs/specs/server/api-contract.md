# Server — API Contract

The stable public surface owned by the server area: the server type and its
constructors, how a consumer supplies request handling (a data store, a handler
trait, or both), the configuration fields, and the feature flags that gate them.

---

## 1. Server type and construction

*(TBD — the exported server type(s), how a TCP server and an RTU server are
constructed, how the bound address/port is read back, and how shutdown is
requested.)*

## 2. Handler surface

*(TBD — the trait or callback a consumer implements to answer requests, its
signature, and how it signals a Modbus exception rather than a transport error.)*

## 3. Data store surface

*(TBD — how a consumer reads and writes the four register tables from outside a
request, and the concurrency guarantees that come with it.)*

## 4. Configuration and feature flags

*(TBD — unit id / slave id acceptance, connection limits, and their defaults.
Every default is observable, therefore normative.)*
