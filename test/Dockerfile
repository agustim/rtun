# docker build -t minitest .
# Start:
# docker run -ti --rm --privileged -v ./target/debug/:/app minitest

# Using alpine
FROM debian:latest

RUN apt-get update && apt-get install -y iproute2 bash netcat-openbsd iputils-ping net-tools

# Install iproute2, bash, netcat
#RUN apk add --no-cache iproute2 bash netcat-openbsd

# Start a bash shell
CMD ["/bin/bash"]
