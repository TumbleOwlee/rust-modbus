# Transport — Edge Cases and Known Limitations

Boundary behavior, error semantics, and the constraints that are **intentional**.
Entries under "Known limitations" are working as implemented; they are recorded
here so they are not mistaken for oversights and silently "fixed".

---

## 1. Framing

*(TBD — RTU inter-frame silence and how a frame boundary is determined; a partial
frame followed by silence; two frames arriving in one read; a TCP read that
splits an MBAP header.)*

| Condition | Behavior |
|---|---|
| | |

## 2. Connection lifecycle

*(TBD — connect refused, connect timeout, peer reset, half-close, serial device
disappearing mid-session.)*

## 3. Known limitations

*(TBD — e.g. no RTU-over-TCP gateway emulation, no UDP.)*

- **ASCII framing exists, an ASCII transport does not.** The frame layer encodes
  and decodes Modbus ASCII (FR-R-110…119); whether a serial transport can be
  *operated* in ASCII mode — including that mode's 1-second inter-character
  timeout — is TBD in this area.
