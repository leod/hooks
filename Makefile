TARGET="target/"
N_STRESS=5

all: build

build-release:
	mkdir -p log
	cargo build -j8 --release

build:
	mkdir -p log
	cargo build -j8 

run: build
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=info target/debug/hooks_server; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=info target/debug/hooks_client; cat"' \; \
		select-layout even-horizontal

run-release: build-release
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server | grep -v \"EPA did not converge\"; cat"' \; \
		split-window -h 'bash -c "RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_client | grep -v \"EPA did not converge\"; cat"' \; \
		select-layout even-horizontal

run-client: build
	RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_client

run-server: build
	RUST_BACKTRACE=1 RUST_LOG=debug target/debug/hooks_server

run-client-release: build-release
	RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_client

run-server-release: build-release
	RUST_BACKTRACE=1 RUST_LOG=debug target/release/hooks_server

random-bot:
	cargo run -j8 --release --example random_bot

stress: build-release
	cargo build -j8 --release --examples
	tmux \
		new-session 'bash -c "RUST_BACKTRACE=1 RUST_LOG=info target/release/hooks_server | grep -v \"EPA did not converge\"; cat"' \; \
		split-window -h 'bash -c "sleep 1; RUST_BACKTRACE=1 RUST_LOG=info target/release/hooks_client | grep -v \"EPA did not converge\"; cat"' \; \
		split-window -h 'bash -c "sleep 1; scripts/random_bots.sh '${N_STRESS}' release; cat"' \; \
		select-layout even-horizontal

fmt:
	cd hooks_game; cargo fmt
	cd hooks_server; cargo fmt
	cd hooks_client; cargo fmt
	cd hooks_util; cargo fmt
	cd hooks_show; cargo fmt
