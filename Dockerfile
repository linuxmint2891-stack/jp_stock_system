FROM rust:1.75-bookworm

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    libopenblas-dev \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Install python dependencies globally (for scripts/notify.py)
RUN pip3 install --no-cache-dir requests python-dotenv --break-system-packages || \
    pip3 install --no-cache-dir requests python-dotenv

WORKDIR /app

# Copy the source code
COPY . .

# Build the project in release mode to pre-compile dependencies and binaries
RUN cargo build --release

# Set default environment variables
ENV RUST_LOG=info
ENV PYTHONUNBUFFERED=1

# The TUI requires a functional terminal
ENV TERM=xterm-256color

# Entrypoint: run the main TUI by default
CMD ["cargo", "run", "--release"]
