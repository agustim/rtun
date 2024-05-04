# docker build -t minitest .
# Start:
# docker run -ti --rm --privileged -v ./target/debug/:/app minitest

# Using alpine
FROM alpine:3.7

# Install iproute2, bash, netcat
RUN apk add --no-cache iproute2 bash netcat-openbsd

# Start a bash shell
CMD ["/bin/bash"]
