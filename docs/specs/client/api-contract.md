# Client — API Contract

The stable public surface owned by the client area: the client type and its
constructors, the request methods and their signatures, the configuration
fields, and the feature flags that gate them.

Per the ownership rule in [`../README.md`](../README.md), client configuration
fields are specified here; transport-level fields (baud rate, socket options)
belong to [`../transport/`](../transport/).

---

## 1. Client type and construction

*(TBD — the exported client type(s), how a TCP client and an RTU client are
constructed, and what is shared between them.)*

## 2. Request methods

*(TBD — one row per supported operation: method signature, the function code it
issues, the success type, and the failure modes.)*

| Method | Function code | Returns |
|---|---|---|
| | | |

## 3. Configuration

*(TBD — response timeout, retry count/backoff, unit id defaults, and their
default values. Every default is observable, therefore normative.)*

## 4. Feature flags

*(TBD.)*
