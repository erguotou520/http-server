# 🚀 Rust HTTP Server

A lightning-fast HTTP server built with Rust, designed to replace `nginx` in frontend deployments with Docker. Simple yet powerful! ⚡️

If you are familiar with `nginx` or [http-server](https://www.npmjs.com/package/http-server) or [vercel serve](https://www.npmjs.com/package/serve), it should be easy to understand and use this server.

## 🎯 Key Features

- 📦 Single binary executable (`hs`)
- 🦀 Pure Rust implementation for maximum performance
- 📂 Directory listing with Index mode
- 🌐 SPA (Single Page Application) mode
- 🎨 Custom 404 page support
- 🗜️ Gzip/Deflate compression
- 💾 Smart cache control
- 📤 File upload capability
- 🔄 HTTP & WebSocket proxy support
- 🔒 Basic authentication

## 🚀 Quick Start

### 📥 Installation

```bash
# Install
curl hs.erguotou.me/install | bash

# Simple server, it will serve current directory with index mode, open http://localhost:8080 in your browser to see it
./hs
```

For more options, you can run `./hs --help` or read the following sections.

### 🛠️ Options

```bash
./hs --help
Usage: hs [OPTIONS] [COMMAND]

Commands:
  update  Update hs self
  help    Print this message or the help of the given subcommand(s)

Options:
  -m, --mode <MODE>
          Work mode [default: index] [possible values: server, spa, index]
  -f, --path <PATH>
          Folder to serve [default: .]
  -b, --base <BASE>
          Base URL path [default: ]
      --host <HOST>
          Host to listen on [default: 0.0.0.0]
      --port <PORT>
          Port to listen on [default: 8080]
  -c, --compress
          Enable compress
  -o, --open
          Automatically open the browser
      --cache
          Cache duration for static files
      --log <LOG>
          Path to save log at
      --error-log <ERROR_LOG>
          Path to save error log at
  -u, --upload
          Enable upload, recommend to enable this in Index mode
  -s, --security <SECURITY>
          Set username:password for basic auth
      --custom-404 <CUSTOM-404>
          Custom 404 page url, eg: 404.html
  -P, --proxies [<PROXY>...]
          Set proxy for requests, eg: /api->http://127.0.0.1:8080
  -W, --websocket-proxies [<WEBSOCKET-PROXY>...]
          Set proxy for websocket, eg: /ws->http://127.0.0.1:5000
      --ignore-files <IGNORE-FILES>
          files to ignore, support regex [default: ^\.]
      --disable-powered-by
          
  -h, --help
          Print help
  -V, --version
          Print version
```

Here is an example of serving a SPA application:

```bash
# APP_URL will be replaced with environment variable
hs -f /path/to/dist -m spa -P "/api->https://dogapi.dog" -P "/app->${APP_URL}" -W "/ws->wss://echo.websocket.in"
```

### 🐳 Docker Usage

We provide a docker image `erguotou/hs` which bind `hs` inside.

```bash
docker run -p 8080:8080 -v $(pwd):/app erguotou/hs
```

We only provide `linux/amd64` and `linux/arm64` images. If you need other platforms, you can build your own image by yourself.
