FROM lukemathwalker/cargo-chef:latest-rust-1.83-alpine AS chef
WORKDIR /app

# Builds a cargo-chef plan
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build profile, release by default
ARG BUILD_PROFILE=release
ENV BUILD_PROFILE=$BUILD_PROFILE

# Extra Cargo flags
ARG RUSTFLAGS=""
ENV RUSTFLAGS="$RUSTFLAGS"

# Extra Cargo features
ARG FEATURES=""
ENV FEATURES=$FEATURES

# Builds dependencies
RUN cargo chef cook --profile $BUILD_PROFILE --features "$FEATURES" --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --profile $BUILD_PROFILE --features "$FEATURES" --locked

# ARG is not resolved in COPY so we have to hack around it by copying the
# binary to a temporary location
RUN cp /app/target/$BUILD_PROFILE/execution-probe /app/execution-probe
RUN cp /app/target/$BUILD_PROFILE/beacon-probe /app/beacon-probe

# Use Alpine as the release image
FROM alpine AS execution-probe
RUN apk add --no-cache ca-certificates libstdc++ libgcc
WORKDIR /app
# Copy execution-probe over from the build stage
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/execution-probe /usr/local/bin
ENTRYPOINT ["/usr/local/bin/execution-probe"]

# Use Alpine as the release image
FROM alpine AS beacon-probe
RUN apk add --no-cache ca-certificates libstdc++ libgcc
WORKDIR /app
# Copy beacon-probe over from the build stage
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/beacon-probe /usr/local/bin
ENTRYPOINT ["/usr/local/bin/beacon-probe"]