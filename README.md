# Running

`cargo run -- serve` sets up the server

Pairs etc can be added by issuing commands via the prompt: `cargo run` (follow the steps)

## Env

The following environment variables can be used to control a few things:

- RUST_LOG=... This app uses env_logger so set this env variable to info/debug etc
- UNIX_SOCKET_BASE_PATH=... The unix sockets created by adding pairs will go there. They use the pattern `exchangeName_pair.sock`
- BITTRADE_SOCKET_PATH=... The path to the control socket (full path, not directory)