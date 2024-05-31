FROM rust:alpine as builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/main.rs ./src/
RUN cargo fetch
RUN apk add --no-cache build-base
COPY . .
RUN cargo build --release --all

FROM alpine:latest
LABEL maintainer="erguotou"
WORKDIR /app
COPY --from=builder /app/target/release/hs /usr/local/bin/hs
Volume /app
EXPOSE 8080
ENTRYPOINT [ "hs" ]
