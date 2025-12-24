# Build Stage
FROM rust:1.69.0 as builder
RUN apt-get update \
    && apt-get install -y protobuf-compiler

RUN mkdir /home/.cargo

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

WORKDIR prosilient-sync

COPY . .

RUN cargo build --release

FROM debian:buster-slim
ARG APP=/usr/src/app

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

EXPOSE 3000

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder /prosilient-sync/target/release/prosilient-sync ${APP}/prosilient-sync
RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./prosilient-sync"]