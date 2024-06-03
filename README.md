# Rust HTTP Server

This is a simple HTTP server implemented in Rust. It amis to replace `nginx` in frontend deployment with docker.

We provide a single binary executable file `hs` which can be used to start the server.

If you are familiar with `nginx` or (http-server)[https://www.npmjs.com/package/http-server] or (vercel serve)[https://www.npmjs.com/package/serve], it should be easy to understand and use this server.

## Features

- One single binary executable file `hs`
- Rust native HTTP server implementation
- Index mode for directory listing
- SPA mode for single page application
- Support for custom 404 page
- Compressed response with gzip or deflate encoding
- Automatic cache control
- Upload enabled
- Http proxy & websocket proxy support
- Basic authentication support

## Usage

```bash
# Install
curl hs.erguotou.me/install | bash

# Simple server, it will serve current directory with index mode, open http://localhost:8080 in your browser to see it
./hs
```

For more options, you can run `./hs --help` or read the following sections.

### Options

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
hs -f /path/to/dist -m spa -P "/api->https://dogapi.dog" -W "/ws->wss://echo.websocket.in"
```

### Docker usage

We provide a docker image `erguotou/hs` which bind `hs` inside.

```bash
docker run -p 8080:8080 -v $(pwd):/app erguotou/hs
```

We only provide `linux/amd64` and `linux/arm64` images. If you need other platforms, you can build your own image by yourself.
