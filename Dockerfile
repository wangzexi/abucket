FROM rust:1.88-bookworm AS build
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates tzdata \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /app/target/release/atree /usr/local/bin/atree

ENV TZ=Asia/Shanghai
ENV BIND=0.0.0.0:9000

RUN mkdir -p /data

EXPOSE 9000

ENTRYPOINT ["atree"]
