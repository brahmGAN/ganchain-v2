# This is the build stage for gpu. Here we create the binary in a temporary image.
FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /gpu
COPY . /gpu

RUN cargo build --locked --release

# This is the 2nd stage: a very small image where we copy the gpu binary."
FROM docker.io/library/ubuntu:20.04

LABEL description="Multistage Docker image for gpu: a platform for web3" \
	io.parity.image.type="builder" \
	io.parity.image.authors="info@gpu.org" \
	io.parity.image.vendor="gpu" \
	io.parity.image.description="gpu: a platform for web3" \
	io.parity.image.source="https://github.com/gpu/gpu/blob/${VCS_REF}/scripts/dockerfiles/gpu/gpu_builder.Dockerfile" \
	io.parity.image.documentation="https://github.com/gpu/gpu/"

COPY --from=builder /gpu/target/release/gpu /usr/local/bin

RUN useradd -m -u 1000 -U -s /bin/sh -d /gpu gpu && \
	mkdir -p /data /gpu/.local/share && \
	chown -R gpu:gpu /data && \
	ln -s /data /gpu/.local/share/gpu && \
# unclutter and minimize the attack surface
	rm -rf /usr/bin /usr/sbin && \
# check if executable works in this container
	/usr/local/bin/gpu --version

USER gpu

EXPOSE 30333 9933 9944 9615
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/gpu"]
