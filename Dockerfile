FROM rust:1.69-alpine3.17 AS builder

RUN apk add --update --no-cache \
    --repository https://dl-cdn.alpinelinux.org/alpine/v3.17/community \
    --repository https://dl-cdn.alpinelinux.org/alpine/v3.17/main \
    musl-dev=1.2.4-r0 \
    vips-dev=8.14.2-r3

WORKDIR /app
COPY . .
RUN RUSTFLAGS="-C target-feature=-crt-static $(pkg-config vips --libs)" cargo build --release

FROM alpine:3.17.3

RUN apk add --update --no-cache \
    --repository https://dl-cdn.alpinelinux.org/alpine/v3.17/community \
    --repository https://dl-cdn.alpinelinux.org/alpine/v3.17/main \
    vips=8.14.2-r3 \
    vips-heif=8.14.2-r3

WORKDIR /app
COPY --from=builder /app/target/release/canvas /app/canvas

ENV CANVAS_PORT=3000
EXPOSE 3000

ENTRYPOINT ["/app/canvas"]
