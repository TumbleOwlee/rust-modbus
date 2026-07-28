# Server — Data Contract

The server's in-memory data model: the four Modbus register tables, their
addressing, widths, and the mapping from a request's address range onto them.

---

## 1. Register tables

*(TBD — coils, discrete inputs, holding registers, input registers: element
width, address space, which are read-only to a client.)*

| Table | Element | Client access |
|---|---|---|
| | | |

## 2. Addressing

*(TBD — address space bounds, how a start address plus quantity maps onto the
table, and what happens at the upper bound.)*

## 3. Initial state

*(TBD — what an unwritten address reads as.)*
