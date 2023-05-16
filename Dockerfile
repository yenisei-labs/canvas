FROM rust:1.69-slim-bullseye AS builder

RUN apt update
RUN apt install -y libvips-dev

WORKDIR /app
COPY . .
RUN RUSTFLAGS="-C target-feature=-crt-static $(pkg-config vips --libs)" cargo build --release

FROM debian:bullseye-20230502-slim

RUN apt update
RUN apt install -y libvips42

WORKDIR /app
COPY --from=builder /app/target/release/canvas /app/canvas

ENV CANVAS_PORT=3000
EXPOSE 3000

ENTRYPOINT ["/app/canvas"]
