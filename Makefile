# 开发模式，监听src目录变化并自动重启服务
dev:
	cargo watch -c -w src -x "run -- -f examples/dist -u -m index"
