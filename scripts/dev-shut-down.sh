#!/bin/bash


docker compose -f maroon/docker-compose.yaml down --remove-orphans
docker compose -f etcd/docker-compose.yaml down --remove-orphans

docker network rm etcd