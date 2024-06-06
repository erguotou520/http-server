# Benchmark

```shell
# nginx
nginx -s reload
wrk -t16 -c100 -d20s http://localhost:8081/
wrk -t16 -c100 -d20s http://localhost:8081/assets/echarts.js
wrk -t16 -c100 -d20s http://localhost:8081/api/v2/breeds
```

```shell
# hs
./target/release/hs -f /tmp/hs-test/dist -P "/api->https://dogapi.dog" --port 8082 -m spa
wrk -t16 -c100 -d20s http://localhost:8082/
wrk -t16 -c100 -d20s http://localhost:8082/assets/echarts.js
wrk -t16 -c100 -d20s http://localhost:8082/api/v2/breeds
```