ETCD_ENDPOINTS := http://localhost:2379,http://localhost:2381,http://localhost:2383
PORT ?= 3000
PROFILE ?= debug
NODE_URLS ?= /ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001,/ip4/127.0.0.1/tcp/3002
KEY_RANGE ?= 0
CONSENSUS_NODES ?= 2

ifeq ($(PROFILE),release)
    PROFILE_FLAG := --release
endif

.PHONY: run-local shutdown start-etcd build-mn run-compose help integtest fmt toolinstall test

toolinstall:
	cargo install taplo-cli

help:
	@echo "Available commands:"0
	@echo "  make run-local        - Run the application locally"
	@echo "  make shutdown         - Shut down all containers and clean up"
	@echo "  make start-etcd       - Start the etcd cluster"
	@echo "  make build-mn         - Build the maroon node image"
	@echo "  make run-compose N=3  - Run N maroon nodes in docker compose (default: 5)"
	@echo "  make help             - Show this help message"

run-local:
	ETCD_ENDPOINTS=${ETCD_ENDPOINTS} \
	NODE_URLS=/ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001,/ip4/127.0.0.1/tcp/3002 \
	SELF_URL=/ip4/127.0.0.1/tcp/${PORT} \
	RUST_LOG=maroon=debug \
	CONSENSUS_NODES=${CONSENSUS_NODES} \
		cargo run -p maroon $(PROFILE_FLAG)

run-gateway:
	KEY_RANGE=${KEY_RANGE} \
	NODE_URLS=${NODE_URLS} \
	RUST_LOG=gateway=debug \
		cargo run -p gateway $(PROFILE_FLAG)

test:
	cargo test --workspace --exclude integration $(PROFILE_FLAG)

integtest:
	RUST_LOG=maroon=info,gateway=debug \
		cargo test -p integration $(PROFILE_FLAG) -- --test-threads 1

shutdown:
	docker compose -f deploy/maroon/docker-compose.yaml down --remove-orphans
	docker compose -f deploy/etcd/docker-compose.yaml down --remove-orphans
	docker network rm etcd

start-etcd:
	# Create network for etcd and maroon docker composes to talk to each other
	docker network create etcd
	docker compose -f deploy/etcd/docker-compose.yaml up -d

build-mn:
	docker build . -f deploy/maroon/Dockerfile --tag=maroon-mn

run-compose:
	RUST_LOG=info \
		docker compose -f deploy/maroon/docker-compose.yaml up 

fmt:
	cargo fmt --all
	taplo format