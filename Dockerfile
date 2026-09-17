ARG HERMES_BASE_IMAGE=nousresearch/hermes-agent:latest

FROM rust:1.98.1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM ${HERMES_BASE_IMAGE} AS hermes
ARG HERMES_UID=10000
ARG HERMES_GID=10000
COPY --from=build /src/target/release/hquota /usr/local/bin/hquota
COPY skills/quota /opt/hquota/skills/quota
USER ${HERMES_UID}:${HERMES_GID}
ENTRYPOINT ["hermes"]
CMD ["gateway", "run"]

FROM debian:bookworm-slim AS broker
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/hquota /usr/local/bin/hquota
USER 10000:10000
ENTRYPOINT ["hquota"]
CMD ["serve"]
