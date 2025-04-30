ETCD_ENDPOINTS := http://localhost:2379,http://localhost:2381,http://localhost:2383

.PHONY: run-local shutdown start-etcd build-mn run-compose help

help:
	@echo "Available commands:"
	@echo "  make run-local        - Run the application locally"
	@echo "  make shutdown         - Shut down all containers and clean up"
	@echo "  make start-etcd       - Start the etcd cluster"
	@echo "  make build-mn         - Build the maroon node image"
	@echo "  make run-compose N=3  - Run N maroon nodes in docker compose (default: 5)"
	@echo "  make help             - Show this help message"

run-local:
	ETCD_ENDPOINTS=${ETCD_ENDPOINTS} \
	NODE_URLS=/ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001 \
	SELF_URL=/ip4/127.0.0.1/tcp/3001 \
	RUST_LOG=debug \
		cargo run

shutdown:
	docker compose -f maroon/docker-compose.yaml down --remove-orphans
	docker compose -f etcd/docker-compose.yaml down --remove-orphans
	docker network rm etcd

start-etcd:
	# Create network for etcd and maroon docker composes to talk to each other
	docker network create etcd
	docker compose -f etcd/docker-compose.yaml up -d

build-mn:
	docker build . -f maroon/Dockerfile --tag=maroon-mn

run-compose:
	N=${N:-5} docker compose -f maroon/docker-compose.yaml up 