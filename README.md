# Bittrade websockets

A Rust server that handles websockets from various Crypto exchanges and writes their (normalized) outcome to unix sockets. The idea is to efficiently deal with heartbeats/authentication/ping-pongs/parsing/reconnects in one centralized server, and consuming bots can instead deal with simple connections to unix sockets.

## Running the server

`cargo run -- serve` sets up the server

### Env

The following environment variables can be used to control a few things:

- RUST_LOG=error This app uses env_logger so set this env variable to info/debug etc
- UNIX_SOCKET_BASE_PATH=/tmp/ The unix sockets created by adding pairs will go there. They use the pattern `exchangeName_pair.sock` e.g. `binance_XRP_USDT.sock`
- BITTRADE_SOCKET_PATH=/tmp/bittrade_websockets_control.sock The path to the control socket (full path, not directory). This allows to run multiple servers if we want to

## Control commands

- Pairs etc can be added by issuing commands via the prompt: `cargo run` (follow the steps)

## Development

### Features

- [x] Control pairs via commands issued on the command line
- [ ] Exchanges
  - [ ] Binance public
  - [ ] Binance private
  - [ ] Whitebit public
  - [ ] Whitebit private
  - [ ] Kraken public
  - [ ] Kraken private
- [ ] Read/write configuration from/to a file
- [ ] ? Write to websocket via the unix sockets