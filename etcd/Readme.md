

Commands to debug etcd

```sh
docker exec etcd-etcd-00-1 etcdctl endpoint status --endpoints=http://etcd-etcd-00-1:2379,http://etcd-etcd-01-1:2379,http://etcd-etcd-02-1:2379 -w table
```


```sh
etcdctl --endpoints=http://localhost:2379 member list
```

```sh
etcdctl --endpoints=http://localhost:2379 get --prefix /maroonOrder
etcdctl --endpoints=http://localhost:2379 del --prefix /etcdBatch   
etcdctl --endpoints=http://localhost:2379 put key value
etcdctl --endpoints=http://localhost:2379 watch /maroonOrder/
```
