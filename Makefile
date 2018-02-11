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

run-game: build
	RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_game

run-server: build
	RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server

random-bot:
	cargo run -j8 --release --example random_bot

stress: build
	cargo build -j8 --release --examples
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_game; cat"' \; \
		split-window -h 'bash -c "for i in {1..'${N_STRESS}'}; do echo $i; target/release/examples/random_bot & done ; cat"' \; \
		select-layout even-horizontal

fmt:
	cd hooks_common; cargo fmt
	cd hooks_server; cargo fmt
	cd hooks_game; cargo fmt
