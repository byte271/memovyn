FROM rust:1.94-alpine AS builder

RUN apk add --no-cache musl-dev perl make pkgconfig

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static

RUN rustup target add x86_64-unknown-linux-musl \
    && cargo build --release --target x86_64-unknown-linux-musl

FROM scratch

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/memovyn /memovyn

EXPOSE 7761
ENTRYPOINT ["/memovyn", "serve", "--bind", "0.0.0.0:7761"]
