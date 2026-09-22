FROM docker.io/library/rust:1-bookworm AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
    libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev \
    pkg-config && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build --release

FROM docker.io/library/debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash ca-certificates fontconfig fonts-dejavu-core \
    libasound2 libdbus-1-3 libfontconfig1 libudev1 \
    libvulkan1 libwayland-client0 libwayland-cursor0 libwayland-egl1 \
    libegl1 libxkbcommon0 \
    mesa-vulkan-drivers && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 hafthi \
    && useradd --uid 1000 --gid 1000 --create-home --shell /bin/bash hafthi \
    && mkdir -p /home/hafthi/.config/hafthi \
    && chown -R hafthi:hafthi /home/hafthi

COPY --from=build /src/target/release/hafthi /usr/local/bin/hafthi

USER hafthi
ENV HOME=/home/hafthi \
    XDG_RUNTIME_DIR=/tmp \
    WAYLAND_DISPLAY=hafthi-wayland \
    LANG=C.UTF-8
WORKDIR /home/hafthi
ENTRYPOINT ["/usr/local/bin/hafthi"]
