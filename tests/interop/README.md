# Interop fixtures

Interop against an independent Modbus implementation — [ferrowl][ferrowl], whose
Modbus stack is built on `tokio-modbus`. Neither direction runs in CI: both need
that binary, so `tests/interop_tcp.rs` is `#[ignore]`d and the server direction is
driven by hand from an example.

[ferrowl]: https://github.com/oweitman/ferrowl

## Direction A — our client against ferrowl's server

```sh
ferrowl run --module name=interop-srv,device=tests/interop/ferrowl-server.toml,\
  transport=tcp,ip=127.0.0.1,port=5020,role=server --duration 240
cargo test --test interop_tcp -- --ignored --test-threads=1
```

`--test-threads=1` is required: every test in that file addresses the same unit 1
on the same live server, so run in parallel they clobber each other's registers.

Covers FC 1, 2, 3, 4, 5, 6, 15, 16 in both request and response direction, plus
two documented deviations of ferrowl's from the specification — see
`docs/specs/client/edge-cases.md`.

## Direction B — ferrowl's client against our server

```sh
cargo run --example interop_server -- 127.0.0.1:5030 30
ferrowl run --module name=interop-cli,device=tests/interop/ferrowl-client.toml,\
  transport=tcp,ip=127.0.0.1,port=5030,role=client --duration 20 --exit-on-error
```

The example serves all four tables on unit 1, addresses 0–15, and prints every
request, connect, disconnect and error. Its second argument is optional: given a
number of seconds, it shuts down through `ServerHandle` after that long, which is
also the drain of SV-R-044 in action.

Expected: ferrowl polls the four read function codes every 500 ms and logs each
answer. Values are `holding[n] == n`, `input[n] == 100 + n`, `discrete[n] == n`
even, coils all clear — so the log reads

```
ReadDiscreteInputs request to read [0, 2) successful. Received values [0001 0000].
ReadHoldingRegisters request to read [0, 4) successful. Received values [0000 0001 0002 0003].
ReadInputRegisters request to read [0, 4) successful. Received values [0064 0065 0066 0067].
```

and ours ends with `disconnected: Closed`, the clean close of SV-R-052.

This direction covers the read function codes only. ferrowl's client writes on an
operator action (`:set`) — a Lua `C_Register:Set` writes ferrowl's own memory, not
the wire — so there is no headless way to make it issue FC 5/6/15/16. Writes
reaching a server are covered by `tests/server_tcp.rs` (our client, our server)
and by direction A (our client, ferrowl's server).
