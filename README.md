# hooks
Multiplayer catching game.

This project is currently in a hiatus due to a lack of time. Hopefully, development will continue at some point...

Currently compiles only with older versions of rustc, e.g. `nightly-2018-04-10` works for me.

## Demo
This video shows where we are at: [https://vimeo.com/267324565](https://vimeo.com/267324565).

Multiplayer synchronization works similarly to e.g. Quake 3:
- Clients periodically send their input to the server.
- The server periodically simulates the game using the inputs and then sends snapshots of the game state to all clients.
- For a smoother display, clients interpolate between the received snapshots.
- Clients can perform local prediction. (This is roughly implemented, but still feels quite derpy due to the dynamicity of the hooks.)

## Overview
We have the following library crates:
- [`hooks_util`](hooks_util)
- [`hooks_game`](hooks_game): Crate shared between `hooks_server` and `hooks_client`.
  - [`physics`](hooks_game/src/physics): Simple 2D physics. Uses some kind of [position based dynamics](http://matthias-mueller-fischer.ch/talks/2017-EG-CourseNotes.pdf) for resolving collisions and constraints.
  - [`net`](hooks_game/src/net): Low-level networking with [enet](https://github.com/ruabmbua/enet-sys).
  - [`repl`](hooks_game/src/repl): Entity replication and game state snapshots.
  - [`game`](hooks_game/src/game): Game logic.
- [`hooks_show`](hooks_show): Display the game state with [ggez](https://github.com/ggez/ggez).

Binary crates:
- [`hooks_server`](hooks_server)
- [`hooks_client`](hooks_client)

## Running

I've tested only on Ubuntu 16.04 so far.

We use `make` for simplicity:
- `make run-release`: Run both `hooks_server` and `hooks_client` locally. Requires `tmux` to be installed.
- `make run-server-release`: Start only the server.
- `make run-client-release`: Start only the client, connecting to `localhost`.
- `make fmt`: `cargo fmt` all crates.
