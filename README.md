# hooks
Multiplayer catching game.

This project is currently in a hiatus due to a lack of time. Hopefully, development will continue at some point...

## Demo
This video shows where we are at: [https://vimeo.com/267324565](https://vimeo.com/267324565).

Multiplayer synchronization works similarly to e.g. Quake 3:
- Clients periodically send their input to the server.
- The server periodically simulates the game using the inputs and then sends snapshots of the game state to all clients.
- For a smoother display, clients interpolate between the received snapshots.
- Clients can perform local prediction. (This is roughly implemented, but still feels quite derpy due to the dynamicity of the hooks.)

## Overview
We have the following library crates:
- `hooks_util`
- `hooks_game`: Code shared between `hooks_server` and `hooks_client`.
  - `physics`: Simple 2D physics. Uses some kind of [position based dynamics](http://matthias-mueller-fischer.ch/talks/2017-EG-CourseNotes.pdf) for resolving collisions and constraints.
  - `net`: Low-level networking with [enet](https://github.com/ruabmbua/enet-sys).
  - `repl`: Entity replication and game state snapshots.
  - `game`: Game logic.
- `hooks_show`: Display the game state with [ggez](https://github.com/ggez/ggez).

Binary crates:
- `hooks_server`
- `hooks_client`

## Running

I've tested only on Ubuntu 16.04 so far.

We use `make` for simplicity:
- `make run-release`: Run both `hooks_server` and `hooks_client` locally. Requires `tmux` to be installed.
- `make run-server-release`: Start just the server.
- `make run-client-release`: Start just the client, connecting to `localhost`.
- `make stress`: Run server, client, and connect 5 `random_bot` clients that just send random input. Requires `tmux` to be installed.
- `make fmt`: `cargo fmt` all crates.
