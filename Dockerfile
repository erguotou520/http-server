FROM rust:alpine as builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/main.rs ./src/
RUN cargo fetch
COPY . .
RUN cargo build --release --all

FROM alpine:latest
LABEL maintainer="erguotou"
WORKDIR /app
COPY --from=builder /app/target/release/hs /usr/local/bin/hs
ENTRYPOINT [ "hs" ]
