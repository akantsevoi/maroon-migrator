FROM quay.io/coreos/etcd:v3.5.0

RUN apt-get update && \
    apt-get install -y iproute2 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* 