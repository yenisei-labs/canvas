FROM rust:1.69-slim-bullseye AS builder

RUN apt update && apt install -y pkg-config libvips-dev

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-20230502-slim

RUN apt update && apt install -y pkg-config libvips-dev libjpeg62-turbo-dev libexif-dev libwebp-dev libpng-dev

WORKDIR /app
COPY --from=builder /app/target/release/canvas /app/canvas

ENV CANVAS_PORT=3000
EXPOSE 3000

ENTRYPOINT ["/app/canvas"]
