FROM rust:alpine as builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release --no-default-features --no-run
COPY . .
RUN cargo build --release --all

FROM alpine:latest
LABEL maintainer="erguotou"
WORKDIR /app
COPY --from=builder /app/target/release/hs /usr/local/bin/hs
ENTRYPOINT [ "hs" ]
