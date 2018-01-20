TARGET="target/"
N_STRESS=10

all: build

build: common server game

run: build
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_game; cat"'

run-game:
	RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_game

random-bot:
	CARGO_TARGET_DIR=${TARGET} cargo run -j4 --manifest-path=hooks_game/Cargo.toml --example random_bot

stress: build
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_game/Cargo.toml --example random_bot
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_game; cat"' \; \
		split-window -h 'bash -c "for i in {1..'${N_STRESS}'}; do echo $i; make random-bot & done ; cat"' \; \
		select-layout even-horizontal

common:
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_common/Cargo.toml

server:
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_server/Cargo.toml

game:
	CARGO_TARGET_DIR=${TARGET} cargo build -j4 --manifest-path=hooks_game/Cargo.toml

fmt:
	cd hooks_common; cargo fmt
	cd hooks_server; cargo fmt
	cd hooks_game; cargo fmt
