# syntax=docker/dockerfile:1.7

FROM --platform=$TARGETPLATFORM debian:bookworm AS brainflow-builder
ARG TARGETARCH
ARG BRAINFLOW_REF=master
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git python3 python3-pip cmake ninja-build build-essential pkg-config file \
    libbluetooth-dev libusb-1.0-0-dev \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --depth 1 https://github.com/brainflow-dev/brainflow.git /opt/brainflow-src
WORKDIR /opt/brainflow-src
RUN python3 tools/build.py
RUN mkdir -p /opt/brainflow/lib \
    && find /opt/brainflow-src -name 'libBoardController.so' -exec cp {} /opt/brainflow/lib/ \; \
    && find /opt/brainflow-src -name 'libDataHandler.so' -exec cp {} /opt/brainflow/lib/ \; \
    && find /opt/brainflow-src -name 'libBrainFlowBluetooth.so' -exec cp {} /opt/brainflow/lib/ \; \
    && test -f /opt/brainflow/lib/libBoardController.so \
    && file /opt/brainflow/lib/libBoardController.so

FROM --platform=$TARGETPLATFORM rust:1-bookworm AS rust-builder
# libudev-dev + pkg-config: libudev-sys (pulled via winit -> eframe even when
# only the headless pi_edge binary is built) needs libudev.pc on the system
# library search path. libusb / libbluetooth: BrainFlow callers and other
# audio/HID crates discover them via pkg-config the same way.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libudev-dev libusb-1.0-0-dev libbluetooth-dev \
        libssl-dev clang \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /work
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin pi_edge

FROM --platform=$TARGETPLATFORM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstdc++6 libgcc-s1 libusb-1.0-0 libbluetooth3 curl file \
    && rm -rf /var/lib/apt/lists/*
COPY --from=brainflow-builder /opt/brainflow/lib /opt/brainflow/lib
COPY --from=rust-builder /work/target/release/pi_edge /usr/local/bin/pi_edge
ENV BRAINFLOW_BOARD_CONTROLLER=/opt/brainflow/lib/libBoardController.so
ENV NEUROSTICK_HOST=0.0.0.0
ENV NEUROSTICK_PORT=8765
ENV OPENBCI_SERIAL=/dev/openbci
ENV OPENBCI_BOARD_ID=2
ENV NEUROSTICK_DATA_DIR=/data
RUN file /opt/brainflow/lib/libBoardController.so
VOLUME ["/data"]
EXPOSE 8765
ENTRYPOINT ["/usr/local/bin/pi_edge"]
