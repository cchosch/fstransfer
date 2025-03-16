.DEFAULT_GOAL=server

server:
	RUSTFLAGS=-Awarnings cargo watch -x "run" -i .\**\{target,.idea,dev}\**
