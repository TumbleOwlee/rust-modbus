# Client — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Entries under "Known limitations" are working as implemented; they are recorded
here so they are not mistaken for oversights and silently "fixed".

---

## 1. Response handling

*(TBD — no response before timeout; a response whose transaction id or unit id
does not match; a late response arriving after a timeout; an exception response;
a response for a function code that was not requested.)*

| Condition | Behavior |
|---|---|
| | |

## 2. Connection loss

*(TBD — peer closes mid-request; connect fails; reconnect policy and its bounds;
whether an in-flight request is retried or failed.)*

## 3. Known limitations

*(TBD.)*
