# TLS test fixtures

Static, checked in — not generated at test time (see `AGENTS.md`, stage s1 of
`tls-transport`). Regenerate with:

```sh
# CA
openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -out ca.crt -days 3650 \
  -subj "/CN=rust-modbus test CA"

# An unrelated CA, used to prove Verify/Require reject a cert they did not issue.
openssl req -x509 -newkey rsa:2048 -nodes -keyout other-ca.key -out other-ca.crt -days 3650 \
  -subj "/CN=rust-modbus other test CA"

# Server identity, signed by ca.crt. subjectAltName = IP:127.0.0.1 because
# tests connect to loopback by IP, so hostname/IP verification is exercised
# for real, not bypassed.
cat > server.ext <<EOF
subjectAltName = IP:127.0.0.1
EOF
openssl req -newkey rsa:2048 -nodes -keyout server.key -out server.csr -subj "/CN=127.0.0.1"
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out server.crt -days 3650 -extfile server.ext

# Client identity for mTLS, signed by ca.crt (reused as the client-cert root too).
# rustls/webpki requires an X.509v3 certificate; -extfile is what gives it a
# version 3 (openssl x509 -req with no extensions produces v1).
cat > client.ext <<EOF
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
EOF
openssl req -newkey rsa:2048 -nodes -keyout client.key -out client.csr \
  -subj "/CN=rust-modbus test client"
openssl x509 -req -in client.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out client.crt -days 3650 -extfile client.ext

# A client identity issued by the *unrelated* CA, used to prove
# ClientCertPolicy::Require rejects a client cert from an untrusted issuer.
openssl req -newkey rsa:2048 -nodes -keyout unrelated-client.key -out unrelated-client.csr \
  -subj "/CN=rust-modbus unrelated client"
openssl x509 -req -in unrelated-client.csr -CA other-ca.crt -CAkey other-ca.key \
  -CAcreateserial -out unrelated-client.crt -days 3650 -extfile client.ext

rm -f *.csr *.srl *.ext
```

Files:

- `ca.crt`/`ca.key` — self-signed CA; issues `server.crt` and `client.crt`.
- `other-ca.crt`/`other-ca.key` — an unrelated self-signed CA; issues `unrelated-client.crt`.
- `server.crt`/`server.key` — server identity, `subjectAltName = IP:127.0.0.1`.
- `client.crt`/`client.key` — client identity for mTLS, trusted (issued by `ca.crt`).
- `unrelated-client.crt`/`unrelated-client.key` — client identity issued by `other-ca.crt`, untrusted under a `ClientCertPolicy::Require(RootStore` loaded with `ca.crt`)`.
