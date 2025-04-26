#!/bin/bash

# is needed so etcd and maroon docker composes can talk to each other
docker network create etcd

docker compose -f etcd/docker-compose.yaml up -d