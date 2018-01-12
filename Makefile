TARGET := "target/"

all: build

build: common server game

run: build
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_game; cat"'

run-game:
	RUST_BACKTRACE=1 target/debug/hooks_game

common:
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_common/Cargo.toml

server:
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_server/Cargo.toml

game:
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_game/Cargo.toml
