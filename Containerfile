FROM rust:1.92-slim as builder
RUN apt-get update && apt-get install -y --no-install-recommends \
	ca-certificates \
	pkg-config \
	libx11-dev \
	libxkbcommon-dev \
	libwayland-dev \
	libgl1-mesa-dev \
	libasound2-dev \
	libudev-dev \
	&& rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo fetch
COPY . .

# Build the binary
RUN cargo build --release

# Collect all shared library dependencies for the runtime
RUN mkdir -p /deps \
	&& ldd /app/target/release/vju | awk '/=> \/|\// {print $(NF-1)}' | xargs -I '{}' cp -v --parents '{}' /deps || true

# Distroless runtime: only the binary, no package manager
FROM gcr.io/distroless/cc-debian12
WORKDIR /app
COPY --from=builder /app/target/release/vju ./vju
COPY --from=builder /deps /deps
ENV LD_LIBRARY_PATH=/deps
ENTRYPOINT ["/app/vju"]