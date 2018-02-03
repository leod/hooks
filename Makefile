TARGET="target/"
N_STRESS=20

all: build

build:
	cargo build -j8 --release

run: build
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_game; cat"' \; \
		select-layout even-horizontal

run-game: game
	RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_game

run-server: server
	RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server

random-bot:
	CARGO_TARGET_DIR=${TARGET} cargo run -j8 --manifest-path=hooks_game/Cargo.toml --example random_bot

stress: build
	CARGO_TARGET_DIR=${TARGET} cargo build -j8 --manifest-path=hooks_game/Cargo.toml --example random_bot
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_game; cat"' \; \
		split-window -h 'bash -c "for i in {1..'${N_STRESS}'}; do echo $i; make random-bot & done ; cat"' \; \
		select-layout even-horizontal

fmt:
	cd hooks_common; cargo fmt
	cd hooks_server; cargo fmt
	cd hooks_game; cargo fmt
