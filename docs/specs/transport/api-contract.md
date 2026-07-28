# Transport — API Contract

The stable public surface owned by the transport area: the transport
abstraction, the TCP and RTU implementations, and every configuration field that
controls a socket or a serial port.

Per the ownership rule in [`../README.md`](../README.md), serial parameters and
socket options are specified here, not in the areas that happen to expose them.

---

## 1. Transport abstraction

*(TBD — the seam client and server are written against, and what a consumer may
substitute for it. This seam is what makes both roles testable without hardware,
so its shape is a contract, not an implementation detail.)*

## 2. TCP configuration

*(TBD — address, connect timeout, socket options, and their defaults.)*

## 3. RTU serial configuration

*(TBD — path, baud rate, data bits, parity, stop bits, flow control, and their
defaults.)*

## 4. Feature flags

*(TBD — e.g. whether serial support is gated behind a feature.)*
